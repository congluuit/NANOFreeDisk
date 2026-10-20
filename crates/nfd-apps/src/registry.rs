//! What the inventory and the leftover scan read from the registry, behind a trait
//! so tests (and the GUI smoke test's fixture) use an in-memory registry instead of
//! the machine's.

use std::collections::BTreeMap;

pub use nfd_platform::registry::{Hive, Value};
use serde::{Deserialize, Serialize};

/// Read-only registry access (64-bit view; 32-bit keys through `WOW6432Node`).
pub trait Registry: Sync {
    /// Subkey names of `hive\path` (empty when the key is missing).
    fn subkeys(&self, hive: Hive, path: &str) -> Vec<String>;
    /// The values of `hive\path`, `None` when the key is missing.
    fn values(&self, hive: Hive, path: &str) -> Option<Vec<(String, Value)>>;
    /// Last write of `hive\path` (Unix seconds).
    fn last_write(&self, hive: Hive, path: &str) -> Option<i64>;

    /// `hive\path` exists.
    fn exists(&self, hive: Hive, path: &str) -> bool {
        self.values(hive, path).is_some()
    }
}

/// The machine's registry.
#[derive(Debug, Default, Clone, Copy)]
pub struct LiveRegistry;

impl Registry for LiveRegistry {
    fn subkeys(&self, hive: Hive, path: &str) -> Vec<String> {
        nfd_platform::registry::subkeys(hive, path).unwrap_or_default()
    }

    fn values(&self, hive: Hive, path: &str) -> Option<Vec<(String, Value)>> {
        nfd_platform::registry::values(hive, path).ok()
    }

    fn last_write(&self, hive: Hive, path: &str) -> Option<i64> {
        nfd_platform::registry::last_write(hive, path).ok()
    }

    fn exists(&self, hive: Hive, path: &str) -> bool {
        nfd_platform::registry::exists(hive, path)
    }
}

/// One key of a [`MemRegistry`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct MemKey {
    /// The path as written (case kept).
    path: String,
    values: Vec<(String, Value)>,
    written: i64,
}

/// An in-memory registry (tests, the GUI smoke test's fixture). Keys compare
/// case-insensitively, like the real one.
#[derive(Debug, Clone, Default)]
pub struct MemRegistry {
    keys: BTreeMap<(Hive, String), MemKey>,
}

fn lower(path: &str) -> String {
    path.trim_matches('\\').to_lowercase()
}

impl MemRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create `hive\path` and its parents.
    pub fn add_key(&mut self, hive: Hive, path: &str) -> &mut Self {
        let path = path.trim_matches('\\');
        let mut acc = String::new();
        for part in path.split('\\') {
            if !acc.is_empty() {
                acc.push('\\');
            }
            acc.push_str(part);
            self.keys
                .entry((hive, lower(&acc)))
                .or_insert_with(|| MemKey {
                    path: acc.clone(),
                    ..MemKey::default()
                });
        }
        self
    }

    /// Set one value (the key is created).
    pub fn set(&mut self, hive: Hive, path: &str, name: &str, value: Value) -> &mut Self {
        self.add_key(hive, path);
        if let Some(k) = self.keys.get_mut(&(hive, lower(path))) {
            k.values.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
            k.values.push((name.to_owned(), value));
        }
        self
    }

    /// A string value.
    pub fn sz(&mut self, hive: Hive, path: &str, name: &str, value: &str) -> &mut Self {
        self.set(hive, path, name, Value::Sz(value.to_owned()))
    }

    /// Remove `hive\path` and everything below it.
    pub fn remove(&mut self, hive: Hive, path: &str) {
        let p = lower(path);
        self.keys
            .retain(|(h, k), _| *h != hive || (k != &p && !k.starts_with(&format!("{p}\\"))));
    }

    /// Remove one value.
    pub fn remove_value(&mut self, hive: Hive, path: &str, name: &str) {
        if let Some(k) = self.keys.get_mut(&(hive, lower(path))) {
            k.values.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
        }
    }

    /// Load the JSON form ([`MemSnapshot`]).
    ///
    /// # Errors
    /// Malformed JSON.
    pub fn from_json(text: &str) -> serde_json::Result<Self> {
        let snap: MemSnapshot = serde_json::from_str(text)?;
        let mut r = Self::new();
        for k in snap.keys {
            let Some((hive, path)) = Hive::split(&k.key) else {
                continue;
            };
            r.add_key(hive, path);
            for (name, v) in k.values {
                let value = match v {
                    MemValue::Text(s) => Value::Sz(s),
                    MemValue::Number(n) => Value::Dword(u32::try_from(n).unwrap_or(u32::MAX)),
                };
                r.set(hive, path, &name, value);
            }
        }
        Ok(r)
    }
}

/// JSON form of a [`MemRegistry`]: `{"keys": [{"key": "HKCU\\Software\\…",
/// "values": {"DisplayName": "Acme", "EstimatedSize": 1024}}]}`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemSnapshot {
    pub keys: Vec<MemSnapshotKey>,
}

/// One key of a [`MemSnapshot`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemSnapshotKey {
    pub key: String,
    #[serde(default)]
    pub values: BTreeMap<String, MemValue>,
}

/// A value of a [`MemSnapshot`]: text (`REG_SZ`) or a number (`REG_DWORD`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MemValue {
    Text(String),
    Number(u64),
}

impl Registry for MemRegistry {
    fn subkeys(&self, hive: Hive, path: &str) -> Vec<String> {
        let p = lower(path);
        let prefix = if p.is_empty() {
            String::new()
        } else {
            format!("{p}\\")
        };
        self.keys
            .iter()
            .filter(|((h, k), _)| *h == hive && k.starts_with(&prefix) && k.len() > prefix.len())
            .filter(|((_, k), _)| !k[prefix.len()..].contains('\\'))
            .map(|(_, v)| v.path.rsplit('\\').next().unwrap_or(&v.path).to_owned())
            .collect()
    }

    fn values(&self, hive: Hive, path: &str) -> Option<Vec<(String, Value)>> {
        self.keys
            .get(&(hive, lower(path)))
            .map(|k| k.values.clone())
    }

    fn last_write(&self, hive: Hive, path: &str) -> Option<i64> {
        self.keys.get(&(hive, lower(path))).map(|k| k.written)
    }
}

/// A string value by name (case-insensitive) from a value list.
#[must_use]
pub fn text<'a>(values: &'a [(String, Value)], name: &str) -> Option<&'a str> {
    values
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .and_then(|(_, v)| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// A number value by name from a value list.
#[must_use]
pub fn number(values: &[(String, Value)], name: &str) -> Option<u64> {
    values
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .and_then(|(_, v)| v.as_u64())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_registry_behaves_like_the_real_one() {
        let mut r = MemRegistry::new();
        r.sz(Hive::CurrentUser, r"Software\Acme\Tool", "Path", r"C:\x")
            .add_key(Hive::CurrentUser, r"Software\Acme\Other");
        assert!(r.exists(Hive::CurrentUser, r"software\ACME"));
        let mut subs = r.subkeys(Hive::CurrentUser, r"Software\Acme");
        subs.sort();
        assert_eq!(subs, vec!["Other".to_owned(), "Tool".to_owned()]);
        assert_eq!(
            r.subkeys(Hive::CurrentUser, "Software"),
            vec!["Acme".to_owned()]
        );
        let v = r
            .values(Hive::CurrentUser, r"Software\Acme\Tool")
            .expect("key");
        assert_eq!(text(&v, "path"), Some(r"C:\x"));
        r.remove(Hive::CurrentUser, r"Software\Acme");
        assert!(!r.exists(Hive::CurrentUser, r"Software\Acme\Tool"));
        assert!(r.exists(Hive::CurrentUser, "Software"));
        assert!(r.values(Hive::LocalMachine, "Software").is_none());
    }

    #[test]
    fn json_form_loads() {
        let r = MemRegistry::from_json(
            r#"{"keys":[{"key":"HKLM\\SOFTWARE\\Acme","values":{"DisplayName":"Acme","EstimatedSize":12}}]}"#,
        )
        .expect("json");
        let v = r.values(Hive::LocalMachine, r"SOFTWARE\Acme").expect("key");
        assert_eq!(text(&v, "DisplayName"), Some("Acme"));
        assert_eq!(number(&v, "EstimatedSize"), Some(12));
    }
}
