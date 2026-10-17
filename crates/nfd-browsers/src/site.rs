//! Which website a cache key belongs to.
//!
//! Every store keys its entries by URL, sometimes with the site that loaded it:
//!
//! * Chromium HTTP cache: `1/0/_dk_<top-frame site> <frame site> <url>` with the
//!   split cache (network isolation), or just `<url>` in older profiles.
//! * Chromium Code Cache (`js`, `wasm`): `_key<url> \n<origin lock>`.
//! * CacheStorage: the origin (and its storage key) from `index.txt`.
//! * Firefox cache2: `O^partitionKey=%28https%2Cexample.com%29,a,:<url>`.
//!
//! The site is the top-level one when the key names it (what the user visited —
//! a CDN's files count for the page that used them), else the URL's host. Only
//! `http` / `https` count; anything else (extensions, `chrome://`) has no site and is
//! never offered.

/// Host of an `http(s)://` URL, lower case, without port, user info or a leading
/// `www.`; `None` for other schemes or an empty host.
#[must_use]
pub fn host_of(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .or_else(|| {
            // scheme case-insensitively
            let lower = url.get(..8)?.to_ascii_lowercase();
            if lower.starts_with("https://") {
                url.get(8..)
            } else if lower.starts_with("http://") {
                url.get(7..)
            } else {
                None
            }
        })?;
    let end = rest
        .find(['/', '?', '#', ' ', '\n', '\t'])
        .unwrap_or(rest.len());
    let mut authority = &rest[..end];
    if let Some(at) = authority.rfind('@') {
        authority = &authority[at + 1..];
    }
    let host = if authority.starts_with('[') {
        // IPv6 literal: keep the brackets, drop the port
        authority.find(']').map(|i| &authority[..=i])?
    } else {
        authority.split(':').next().unwrap_or("")
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let host = host.strip_prefix("www.").map(str::to_owned).unwrap_or(host);
    if host.is_empty()
        || !host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-._[]:".contains(c))
    {
        return None;
    }
    Some(host)
}

/// The first `http(s)://…` token of `text` (tokens split on whitespace), with any
/// prefix glued to it (`_dk_`, `_key`, `s_`) cut off.
fn url_in(token: &str) -> Option<&str> {
    let lower = token.to_ascii_lowercase();
    let i = lower.find("https://").or_else(|| lower.find("http://"))?;
    Some(&token[i..])
}

/// Site of a Chromium HTTP cache key.
#[must_use]
pub fn http_cache_site(key: &str) -> Option<String> {
    let tokens: Vec<&str> = key.split_whitespace().collect();
    let url = tokens.iter().rev().find_map(|t| url_in(t))?;
    let site = if tokens.len() > 1 {
        // `_dk_<top-frame site> <frame site> <url>`: the first one is what the user
        // visited
        tokens.iter().find_map(|t| url_in(t)).unwrap_or(url)
    } else {
        url
    };
    host_of(site).or_else(|| host_of(url))
}

/// Site of a Chromium Code Cache key (`_key<url> \n<origin lock>`).
#[must_use]
pub fn code_cache_site(key: &str) -> Option<String> {
    let tokens: Vec<&str> = key.split_whitespace().collect();
    let url = tokens.first().and_then(|t| url_in(t))?;
    let lock = tokens.iter().skip(1).rev().find_map(|t| url_in(t));
    lock.and_then(host_of).or_else(|| host_of(url))
}

/// Site of a CacheStorage folder: the top-level site of a partitioned storage key
/// (`https://a.example/^0https://b.example`), else the origin.
#[must_use]
pub fn cache_storage_site(origin: Option<&str>, storage_key: Option<&str>) -> Option<String> {
    if let Some(key) = storage_key {
        if let Some(i) = key.find("^0") {
            if let Some(h) = host_of(&key[i + 2..]) {
                return Some(h);
            }
        }
        if let Some(h) = host_of(key) {
            return Some(h);
        }
    }
    origin.and_then(host_of)
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(v) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Site of a Firefox cache2 key: the partition key's site, else the URL's host.
#[must_use]
pub fn firefox_site(key: &str) -> Option<String> {
    // the URL is everything after the first ':' tag (`a,:https://…`, `:https://…`)
    let url = if let Some(rest) = key.strip_prefix(':') {
        rest
    } else {
        &key[key.find(",:")? + 2..]
    };
    let tags = &key[..key.len() - url.len()];
    for tag in tags.split(',') {
        let Some(i) = tag.find("partitionKey=") else {
            continue;
        };
        // `(https,example.com)` or `(https,example.com,8443)`
        let value = percent_decode(&tag[i + "partitionKey=".len()..]);
        let inner = value.trim_start_matches('(').trim_end_matches(')');
        let mut parts = inner.split(',');
        if let (Some(scheme), Some(host)) = (parts.next(), parts.next()) {
            if let Some(h) = host_of(&format!("{scheme}://{host}/")) {
                return Some(h);
            }
        }
    }
    host_of(url)
}

/// `true` when `site` is `wanted` or, with `subdomains`, one of its subdomains
/// (`m.youtube.com` for `youtube.com`). Both are compared as [`host_of`] gives them.
#[must_use]
pub fn site_matches(site: &str, wanted: &str, subdomains: bool) -> bool {
    let wanted = wanted.trim().trim_end_matches('.').to_ascii_lowercase();
    let wanted = wanted.strip_prefix("www.").unwrap_or(&wanted);
    site == wanted
        || (subdomains
            && site.len() > wanted.len()
            && site.ends_with(wanted)
            && site.as_bytes()[site.len() - wanted.len() - 1] == b'.')
}

/// Normalize what the user typed (`https://www.YouTube.com/watch`, `youtube.com`)
/// into a host, or `None` when it is not one.
#[must_use]
pub fn normalize_host(input: &str) -> Option<String> {
    let t = input.trim();
    if t.is_empty() || t.contains(char::is_whitespace) {
        return None;
    }
    if t.contains("://") {
        return host_of(t);
    }
    host_of(&format!("https://{t}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts() {
        assert_eq!(
            host_of("https://www.YouTube.com/watch?v=1").as_deref(),
            Some("youtube.com")
        );
        assert_eq!(
            host_of("http://user:pw@example.com:8080/x").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            host_of("HTTPS://Example.COM").as_deref(),
            Some("example.com")
        );
        assert_eq!(host_of("chrome-extension://abc/x.js"), None);
        assert_eq!(host_of("https:///x"), None);
        assert_eq!(host_of("https://[::1]:8080/").as_deref(), Some("[::1]"));
    }

    #[test]
    fn http_cache_keys() {
        assert_eq!(
            http_cache_site(
                "1/0/_dk_https://youtube.com https://youtube.com https://i.ytimg.com/vi/x.jpg"
            )
            .as_deref(),
            Some("youtube.com")
        );
        assert_eq!(
            http_cache_site("https://example.com/a.css").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            http_cache_site("1/0/https://cdn.example.org/a.js").as_deref(),
            Some("cdn.example.org")
        );
        assert_eq!(http_cache_site("1/0/chrome://newtab/"), None);
    }

    #[test]
    fn code_cache_keys() {
        assert_eq!(
            code_cache_site("_keyhttps://assets.higgsfield.ai/a.js \nhttps://higgsfield.ai/")
                .as_deref(),
            Some("higgsfield.ai")
        );
        assert_eq!(
            code_cache_site("_keyhttps://example.com/a.js \n").as_deref(),
            Some("example.com")
        );
        assert_eq!(code_cache_site("_keychrome-extension://id/a.js \n"), None);
    }

    #[test]
    fn storage_keys() {
        assert_eq!(
            cache_storage_site(Some("https://www.youtube.com/"), None).as_deref(),
            Some("youtube.com")
        );
        assert_eq!(
            cache_storage_site(
                Some("https://embed.example/"),
                Some("https://embed.example/^0https://news.example")
            )
            .as_deref(),
            Some("news.example")
        );
    }

    #[test]
    fn firefox_keys() {
        assert_eq!(
            firefox_site("a,:https://www.mozilla.org/en-US/").as_deref(),
            Some("mozilla.org")
        );
        assert_eq!(
            firefox_site(":http://example.com/").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            firefox_site("O^partitionKey=%28https%2Cnews.example%29,a,:https://cdn.example/x.js")
                .as_deref(),
            Some("news.example")
        );
        assert_eq!(firefox_site("garbage"), None);
    }

    #[test]
    fn matching() {
        assert!(site_matches("youtube.com", "youtube.com", false));
        assert!(site_matches("m.youtube.com", "www.youtube.com", true));
        assert!(!site_matches("m.youtube.com", "youtube.com", false));
        assert!(!site_matches("notyoutube.com", "youtube.com", true));
        assert_eq!(
            normalize_host(" https://www.YouTube.com/watch ").as_deref(),
            Some("youtube.com")
        );
        assert_eq!(
            normalize_host("youtube.com").as_deref(),
            Some("youtube.com")
        );
        assert_eq!(normalize_host("  "), None);
        assert_eq!(normalize_host("foo bar"), None);
    }
}
