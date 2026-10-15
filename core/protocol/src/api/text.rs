//! The handful of string rules the web client inherits from JavaScript,
//! written out so that what reaches the deployment is byte for byte what a
//! browser sends, and what is read back is cut where a browser cuts it.
//!
//! Each of these looks like something the standard library already does, and
//! each differs from it somewhere that shows: `str::trim` strips U+0085 where
//! `String.prototype.trim` keeps it and keeps U+FEFF where JavaScript strips
//! it; `url`'s path encoding leaves `@` alone where `encodeURIComponent`
//! escapes it, so a scoped room's join would reach the access log in a shape no
//! browser produces; and `slice` counts UTF-16 units, not characters.

/// `encodeURIComponent`: everything but `A-Z a-z 0-9 - _ . ! ~ * ' ( )` is
/// escaped, as UTF-8, in upper-case hex.
pub(crate) fn encode_uri_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());

    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }

    out
}

/// What `String.prototype.trim` removes: the ECMAScript WhiteSpace and
/// LineTerminator sets, which are Unicode's space separators plus a few
/// controls and U+FEFF, and not U+0085.
fn js_space(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'
            | '\u{000A}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// `String.prototype.trim`.
pub(crate) fn js_trim(value: &str) -> &str {
    value.trim_matches(js_space)
}

/// `value.slice(0, units)`, where a length is counted in UTF-16 code units.
///
/// A cut through the middle of a surrogate pair leaves JavaScript holding half
/// of one, which `URLSearchParams` sends and React draws as U+FFFD; that is
/// what is put in its place here, since a Rust string cannot hold the half.
pub(crate) fn utf16_prefix(value: &str, units: usize) -> String {
    let mut out = String::new();
    let mut used = 0;

    for c in value.chars() {
        let width = c.len_utf16();
        if used + width > units {
            if used < units {
                out.push('\u{FFFD}');
            }
            break;
        }
        out.push(c);
        used += width;
    }

    out
}

/// `^[\w-]{min,max}$` as JavaScript reads it without the `u` flag: ASCII word
/// characters and the dash, counted in units that are, for these, bytes.
pub(crate) fn word_dash(value: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Days since 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's
/// `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);

    (year, month, day)
}

/// The inverse of `civil_from_days`.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year.rem_euclid(400);
    let month = i64::from(month);
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;

    era * 146_097 + doe - 719_468
}

/// `new Date(ms).toISOString()`: `2026-09-02T08:44:00.000Z`.
///
/// The arranged meeting is sent in exactly this shape because it is the shape
/// every browser sends; `None` outside the four-digit years, where JavaScript
/// switches to an expanded form the server would refuse anyway.
pub(crate) fn iso_from_ms(ms: i64) -> Option<String> {
    let days = ms.div_euclid(86_400_000);
    let within = ms.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);

    if !(0..=9999).contains(&year) {
        return None;
    }

    let (hour, minute, second, milli) = (
        within / 3_600_000,
        within / 60_000 % 60,
        within / 1000 % 60,
        within % 1000,
    );

    Some(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{milli:03}Z"
    ))
}

/// An RFC 3339 instant, as the server writes them, in milliseconds since the
/// epoch; `None` for anything else, where `new Date(...)` would give NaN.
///
/// Fractions past the millisecond are dropped rather than rounded, as V8 does.
pub(crate) fn ms_from_rfc3339(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    let digits = |from: usize, len: usize| -> Option<i64> {
        let part = bytes.get(from..from + len)?;
        if !part.iter().all(u8::is_ascii_digit) {
            return None;
        }
        std::str::from_utf8(part).ok()?.parse().ok()
    };
    let at = |index: usize, wanted: &[u8]| bytes.get(index).is_some_and(|b| wanted.contains(b));

    let year = digits(0, 4)?;
    let month = digits(5, 2)?;
    let day = digits(8, 2)?;
    if !at(4, b"-") || !at(7, b"-") || !at(10, b"Tt") {
        return None;
    }
    let hour = digits(11, 2)?;
    let minute = digits(14, 2)?;
    let second = digits(17, 2)?;
    if !at(13, b":") || !at(16, b":") {
        return None;
    }

    let mut index = 19;
    let mut milli = 0;
    if at(index, b".") {
        index += 1;
        let start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == start {
            return None;
        }
        let fraction = &value[start..index.min(start + 3)];
        milli = format!("{fraction:0<3}").parse().ok()?;
    }

    let offset = match bytes.get(index) {
        Some(b'Z' | b'z') if index + 1 == bytes.len() => 0,
        Some(sign @ (b'+' | b'-')) if index + 6 == bytes.len() && at(index + 3, b":") => {
            let minutes = digits(index + 1, 2)? * 60 + digits(index + 4, 2)?;
            if *sign == b'+' {
                minutes
            } else {
                -minutes
            }
        }
        _ => return None,
    };

    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let longest = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    if day < 1 || day > longest || hour > 23 || minute > 59 || second > 59 {
        return None;
    }

    let days = days_from_civil(year, month as u32, day as u32);

    Some(((days * 24 + hour) * 60 + minute - offset) * 60_000 + second * 1000 + milli)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_what_encode_uri_component_escapes() {
        assert_eq!(encode_uri_component("standup@acme"), "standup%40acme");
        assert_eq!(
            encode_uri_component("a b/c?d#e&f=g"),
            "a%20b%2Fc%3Fd%23e%26f%3Dg"
        );
        assert_eq!(encode_uri_component("-_.!~*'()"), "-_.!~*'()");
        assert_eq!(encode_uri_component("已"), "%E5%B7%B2");
    }

    #[test]
    fn trims_what_javascript_trims() {
        assert_eq!(js_trim("\u{FEFF} Alice \u{3000}"), "Alice");
        // NEL is not JavaScript white space, though it is Unicode's.
        assert_eq!(js_trim("\u{0085}Alice"), "\u{0085}Alice");
    }

    #[test]
    fn cuts_where_javascript_cuts() {
        assert_eq!(utf16_prefix("abc", 2), "ab");
        assert_eq!(utf16_prefix("a\u{1F600}b", 3), "a\u{1F600}");
        assert_eq!(utf16_prefix("a\u{1F600}b", 2), "a\u{FFFD}");
        assert_eq!(utf16_prefix("short", 300), "short");
    }

    #[test]
    fn writes_and_reads_instants() {
        assert_eq!(iso_from_ms(0).as_deref(), Some("1970-01-01T00:00:00.000Z"));
        assert_eq!(
            iso_from_ms(1_788_338_640_000).as_deref(),
            Some("2026-09-02T08:44:00.000Z")
        );
        assert_eq!(
            iso_from_ms(951_782_400_123).as_deref(),
            Some("2000-02-29T00:00:00.123Z")
        );
        assert_eq!(
            ms_from_rfc3339("2026-09-02T08:44:00Z"),
            Some(1_788_338_640_000)
        );
        assert_eq!(
            ms_from_rfc3339("2026-09-02T08:44:00.000Z"),
            Some(1_788_338_640_000)
        );
        assert_eq!(
            ms_from_rfc3339("2026-09-02T16:44:00+08:00"),
            Some(1_788_338_640_000)
        );
        assert_eq!(
            ms_from_rfc3339("2000-02-29T00:00:00.1239Z"),
            Some(951_782_400_123)
        );
        for bad in [
            "",
            "soon",
            "2026-02-30T00:00:00Z",
            "2026-09-02 08:44:00Z",
            "2026-09-02T08:44:00",
        ] {
            assert_eq!(ms_from_rfc3339(bad), None, "{bad}");
        }
    }
}
