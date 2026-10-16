//! Byte-size parsing and formatting.
//!
//! Sizes follow the Windows convention: every suffix is binary (1 KB = 1 KiB = 1024 B),
//! so `10MB` typed by a user means the same as Explorer's "10 MB". Output always uses
//! the unambiguous IEC labels (`KiB`, `MiB`, …).

use thiserror::Error;

/// A size string that could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error(
    "invalid size '{0}': expected a number with an optional unit, e.g. 4096, 512K, 10MB, 1.5GiB"
)]
pub struct ParseSizeError(pub String);

/// Parse `"4096"`, `"512k"`, `"10MB"`, `"1.5 GiB"`, `"2T"` into bytes (case-insensitive).
///
/// # Errors
/// Returns [`ParseSizeError`] for empty input, unknown units, negative or overflowing values.
pub fn parse_size(input: &str) -> Result<u64, ParseSizeError> {
    let err = || ParseSizeError(input.to_owned());
    let s = input.trim();
    let split = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(s.len());
    let (number, unit) = s.split_at(split);
    if number.is_empty() {
        return Err(err());
    }
    let shift = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 0,
        "k" | "kb" | "kib" => 10,
        "m" | "mb" | "mib" => 20,
        "g" | "gb" | "gib" => 30,
        "t" | "tb" | "tib" => 40,
        _ => return Err(err()),
    };
    if let Ok(whole) = number.parse::<u64>() {
        return whole.checked_mul(1u64 << shift).ok_or_else(err);
    }
    let value: f64 = number.parse().map_err(|_| err())?;
    let bytes = value * (1u64 << shift) as f64;
    if !bytes.is_finite() || bytes < 0.0 || bytes >= u64::MAX as f64 {
        return Err(err());
    }
    Ok(bytes.round() as u64)
}

/// Format bytes with one decimal and an IEC unit: `0 B`, `512 B`, `1.5 KiB`, `12.3 GiB`.
#[must_use]
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// Format an integer with thousands separators: `1234567` → `1,234,567`.
#[must_use]
pub fn format_count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_units() {
        assert_eq!(parse_size("4096"), Ok(4096));
        assert_eq!(parse_size("512k"), Ok(512 * 1024));
        assert_eq!(parse_size("10MB"), Ok(10 << 20));
        assert_eq!(parse_size("10 MiB"), Ok(10 << 20));
        assert_eq!(parse_size("1.5GiB"), Ok(3 << 29));
        assert_eq!(parse_size("2T"), Ok(2 << 40));
        assert_eq!(parse_size(" 7 b "), Ok(7));
    }

    #[test]
    fn rejects_garbage() {
        for bad in [
            "",
            "MB",
            "-1",
            "10XB",
            "1.2.3",
            "99999999999999999999",
            "20000000T",
        ] {
            assert!(parse_size(bad).is_err(), "{bad} should not parse");
        }
    }

    #[test]
    fn formats() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1536), "1.5 KiB");
        assert_eq!(format_size(10 << 30), "10.0 GiB");
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1_234_567), "1,234,567");
    }
}
