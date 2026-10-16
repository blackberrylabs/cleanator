//! Byte sizes: parsing of rule values such as `"500MB"` and human-readable formatting.
//!
//! Rules use binary units (1 MB = 1 048 576 bytes), like `du -h`. Docker reports decimal
//! units (`1.86GB` = 1.86 × 10⁹); `parse_docker` handles that separately so the two are
//! never mixed.

use std::fmt;

const KIB: u64 = 1024;
const MIB: u64 = KIB * 1024;
const GIB: u64 = MIB * 1024;
const TIB: u64 = GIB * 1024;

/// Parse a size written in a rule file: `"100MB"`, `"1GB"`, `"512 KiB"`, `"4096"`.
/// Units are binary whatever the spelling (`MB`, `MiB`, `M` all mean 1 048 576).
pub fn parse_size(text: &str) -> Option<u64> {
    let t = text.trim();
    let split = t
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ','))
        .unwrap_or(t.len());
    let (num, unit) = t.split_at(split);
    let value: f64 = num.replace(',', ".").parse().ok()?;
    let mult = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1.0,
        "k" | "kb" | "kib" => KIB as f64,
        "m" | "mb" | "mib" => MIB as f64,
        "g" | "gb" | "gib" => GIB as f64,
        "t" | "tb" | "tib" => TIB as f64,
        _ => return None,
    };
    Some((value * mult).round() as u64)
}

/// Parse a size as Docker prints it (decimal units): `"1.184GB"`, `"676.9MB"`, `"32B"`,
/// `"12.52kB"`.
pub fn parse_docker(text: &str) -> Option<u64> {
    let t = text.trim();
    let split = t
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(t.len());
    let (num, unit) = t.split_at(split);
    let value: f64 = num.parse().ok()?;
    let mult = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1.0,
        "kb" => 1e3,
        "mb" => 1e6,
        "gb" => 1e9,
        "tb" => 1e12,
        _ => return None,
    };
    Some((value * mult).round() as u64)
}

/// Format bytes the way `du -h` does: `1.2G`, `640M`, `12K`.
pub fn human(bytes: u64) -> String {
    Human(bytes).to_string()
}

pub struct Human(pub u64);

impl fmt::Display for Human {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let b = self.0;
        if b >= TIB {
            write!(f, "{:.1}T", b as f64 / TIB as f64)
        } else if b >= GIB {
            write!(f, "{:.1}G", b as f64 / GIB as f64)
        } else if b >= MIB {
            write!(f, "{:.0}M", b as f64 / MIB as f64)
        } else if b >= KIB {
            write!(f, "{:.0}K", b as f64 / KIB as f64)
        } else {
            write!(f, "{b}B")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rule_sizes_in_binary_units() {
        assert_eq!(parse_size("500MB"), Some(500 * MIB));
        assert_eq!(parse_size("1GB"), Some(GIB));
        assert_eq!(parse_size("1.5 GiB"), Some(GIB + GIB / 2));
        assert_eq!(parse_size("4096"), Some(4096));
        assert_eq!(parse_size("200M"), Some(200 * MIB));
        assert_eq!(parse_size("nope"), None);
    }

    #[test]
    fn parses_docker_sizes_in_decimal_units() {
        assert_eq!(parse_docker("1.184GB"), Some(1_184_000_000));
        assert_eq!(parse_docker("676.9MB"), Some(676_900_000));
        assert_eq!(parse_docker("12.52kB"), Some(12_520));
        assert_eq!(parse_docker("32B"), Some(32));
        assert_eq!(parse_docker("0B"), Some(0));
    }

    #[test]
    fn formats_like_du() {
        assert_eq!(human(0), "0B");
        assert_eq!(human(1536), "2K");
        assert_eq!(human(640 * MIB), "640M");
        assert_eq!(human(13 * GIB + GIB / 5), "13.2G");
    }
}

/// `YYYY-MM-DD` for a Unix timestamp (UTC), without pulling a date library into callers.
pub fn ymd(ts: i64) -> String {
    let days = ts.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod date_tests {
    #[test]
    fn converts_known_dates() {
        assert_eq!(super::ymd(0), "1970-01-01");
        assert_eq!(super::ymd(1_792_108_800), "2026-10-16");
        assert_eq!(super::ymd(951_782_400), "2000-02-29");
    }
}
