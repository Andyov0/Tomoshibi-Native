//! What the web client's JavaScript does to a value without saying so, done
//! the same way here.
//!
//! Every message on the `watch` and `jukebox` topics is written by
//! `JSON.stringify` and read by `JSON.parse` in each browser, and the web
//! client turns an odd field into a string or a number with `String(...)` or
//! `Number(...)` rather than refusing it. A native participant reads the same
//! bytes and has to arrive at the same values: a seek a browser holder takes
//! from `{"to": "12"}` is one a native holder has to take too, and a key a
//! browser cuts at a hundred UTF-16 units is one a native client cuts at the
//! same place. These are the few conversions the two protocols lean on, and
//! nothing more general; both modules use them, which is why they live here
//! and are visible to the crate.

use serde::Serializer;
use serde_json::Value;

/// `JSON.parse(new TextDecoder().decode(payload))`, kept only where the result
/// is an object whose `t` is a string, which is all either protocol's `parse`
/// accepts.
///
/// The decoder replaces bytes that are not UTF-8 rather than failing, and
/// drops a leading byte-order mark, as `TextDecoder` does by default.
pub(crate) fn decode(payload: &[u8]) -> Option<Value> {
    let bytes = payload.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(payload);
    let text = String::from_utf8_lossy(bytes);
    let value: Value = serde_json::from_str(&text).ok()?;
    matches!(value.get("t"), Some(Value::String(_))).then_some(value)
}

/// The message's `t`, once [`decode`] has said there is one.
pub(crate) fn kind(message: &Value) -> &str {
    message.get("t").and_then(Value::as_str).unwrap_or_default()
}

/// A string's `length` in JavaScript: UTF-16 code units, not characters.
pub(crate) fn length(text: &str) -> usize {
    text.encode_utf16().count()
}

/// `text.slice(0, units)`.
///
/// Counted in UTF-16 code units, as JavaScript counts. Where the cut would
/// fall inside a surrogate pair, the whole character is left out: JavaScript
/// would keep half of it, which is not a string Rust can hold, and a lone
/// surrogate in a title is drawn as a replacement mark either way.
pub(crate) fn cut(text: &str, units: usize) -> String {
    let mut taken = 0;
    let mut end = 0;
    for (at, character) in text.char_indices() {
        taken += character.len_utf16();
        if taken > units {
            break;
        }
        end = at + character.len_utf8();
    }
    text[..end].to_string()
}

/// `text.trim()`.
///
/// JavaScript's whitespace is not quite Unicode's: it includes the byte-order
/// mark U+FEFF and leaves out U+0085, the one difference from `str::trim`
/// that a pasted query can carry.
pub(crate) fn trim(text: &str) -> &str {
    let space = |c: char| c == '\u{FEFF}' || (c.is_whitespace() && c != '\u{85}');
    text.trim_matches(space)
}

/// `String(value)`, where `None` is a field that is not there.
pub(crate) fn string(value: Option<&Value>) -> String {
    match value {
        None => "undefined".into(),
        Some(Value::Null) => "null".into(),
        Some(Value::Bool(yes)) => yes.to_string(),
        Some(Value::Number(number)) => number_text(number.as_f64().unwrap_or(f64::NAN)),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| match item {
                Value::Null => String::new(),
                other => string(Some(other)),
            })
            .collect::<Vec<_>>()
            .join(","),
        Some(Value::Object(_)) => "[object Object]".into(),
    }
}

/// `Number(value)`, where `None` is a field that is not there.
pub(crate) fn number(value: Option<&Value>) -> f64 {
    match value {
        None => f64::NAN,
        Some(Value::Null) => 0.0,
        Some(Value::Bool(yes)) => f64::from(u8::from(*yes)),
        Some(Value::Number(number)) => number.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(text)) => number_from(text),
        // An array becomes the string its items join to, and then a number.
        Some(array @ Value::Array(_)) => number_from(&string(Some(array))),
        Some(Value::Object(_)) => f64::NAN,
    }
}

/// `Number(text)` for a string: blank is nought, a decimal literal is itself,
/// `0x`, `0o` and `0b` read in their radix, and anything else is not a number.
fn number_from(text: &str) -> f64 {
    let text = trim(text);
    if text.is_empty() {
        return 0.0;
    }
    match text {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    for (prefix, radix) in [("0x", 16), ("0X", 16), ("0o", 8), ("0O", 8), ("0b", 2), ("0B", 2)] {
        if let Some(digits) = text.strip_prefix(prefix) {
            return u128::from_str_radix(digits, radix).map_or(f64::NAN, |n| n as f64);
        }
    }
    // Rust also reads "inf" and "nan", which JavaScript does not: only a
    // literal JavaScript would accept goes to the parser.
    let bytes = text.as_bytes();
    let mut at = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let start = at;
    let mut digits = 0;
    while bytes.get(at).is_some_and(u8::is_ascii_digit) {
        at += 1;
        digits += 1;
    }
    if bytes.get(at) == Some(&b'.') {
        at += 1;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
            digits += 1;
        }
    }
    if digits == 0 || at == start {
        return f64::NAN;
    }
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        at += 1;
        if matches!(bytes.get(at), Some(b'+' | b'-')) {
            at += 1;
        }
        let before = at;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        if at == before {
            return f64::NAN;
        }
    }
    if at != bytes.len() {
        return f64::NAN;
    }
    text.parse().unwrap_or(f64::NAN)
}

/// A number as JavaScript prints one: a whole number without a decimal point.
fn number_text(value: f64) -> String {
    if value.is_nan() {
        "NaN".into()
    } else if value.is_infinite() {
        if value > 0.0 { "Infinity" } else { "-Infinity" }.into()
    } else if value.fract() == 0.0 && value.abs() < 1e21 {
        format!("{}", value as i128)
    } else {
        format!("{value}")
    }
}

/// Writes a number the way `JSON.stringify` does: a whole number as an
/// integer, so a position of ten goes out as `10` and not `10.0`. Either
/// parses to the same value in a browser; this keeps the bytes the same as
/// well, which is what a packet capture beside a browser's compares.
pub(crate) fn json_number<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        serializer.serialize_i64(*value as i64)
    } else {
        // Not finite is `null`, in serde_json as in JavaScript.
        serializer.serialize_f64(*value)
    }
}

/// A key for something added, as the web makes one:
/// `${Date.now().toString(36)}-${(counter++).toString(36)}`, from the clock
/// the caller passes in rather than one read here.
pub(crate) fn new_key(now_ms: u64, counter: &mut u64) -> String {
    let key = format!("{}-{}", base36(now_ms), base36(*counter));
    *counter += 1;
    key
}

fn base36(mut value: u64) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".into();
    }
    let mut out = Vec::new();
    while value > 0 {
        out.push(DIGITS[(value % 36) as usize]);
        value /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_numbers_as_javascript_does() {
        assert_eq!(number(Some(&json!("12"))), 12.0);
        assert_eq!(number(Some(&json!(" 12 "))), 12.0);
        assert_eq!(number(Some(&json!(""))), 0.0);
        assert_eq!(number(Some(&json!(null))), 0.0);
        assert_eq!(number(Some(&json!(true))), 1.0);
        assert_eq!(number(Some(&json!([5]))), 5.0);
        assert_eq!(number(Some(&json!([]))), 0.0);
        assert_eq!(number(Some(&json!("0x10"))), 16.0);
        assert_eq!(number(Some(&json!(".5"))), 0.5);
        assert_eq!(number(Some(&json!("1e3"))), 1000.0);
        assert!(number(None).is_nan());
        assert!(number(Some(&json!("inf"))).is_nan());
        assert!(number(Some(&json!("nan"))).is_nan());
        assert!(number(Some(&json!("12px"))).is_nan());
        assert!(number(Some(&json!({}))).is_nan());
        assert!(number(Some(&json!("."))).is_nan());
    }

    #[test]
    fn writes_strings_as_javascript_does() {
        assert_eq!(string(None), "undefined");
        assert_eq!(string(Some(&json!(null))), "null");
        assert_eq!(string(Some(&json!(42))), "42");
        assert_eq!(string(Some(&json!(42.0))), "42");
        assert_eq!(string(Some(&json!(0.5))), "0.5");
        assert_eq!(string(Some(&json!([1, null, "a"]))), "1,,a");
        assert_eq!(string(Some(&json!({"a": 1}))), "[object Object]");
    }

    #[test]
    fn cuts_and_trims_as_javascript_does() {
        assert_eq!(cut("abcdef", 3), "abc");
        assert_eq!(cut("ab", 3), "ab");
        // A character outside the basic plane is two units long.
        assert_eq!(cut("a\u{1F600}b", 2), "a");
        assert_eq!(cut("a\u{1F600}b", 3), "a\u{1F600}");
        assert_eq!(length("a\u{1F600}"), 3);
        assert_eq!(trim("\u{FEFF} x \u{3000}"), "x");
        assert_eq!(trim("\u{85}x"), "\u{85}x");
    }

    #[test]
    fn makes_keys_as_the_web_does() {
        let mut counter = 0;
        assert_eq!(new_key(1_700_000_000_000, &mut counter), "loyw3v28-0");
        assert_eq!(new_key(1_700_000_000_000, &mut counter), "loyw3v28-1");
    }

    #[test]
    fn decodes_what_text_decoder_and_json_parse_would() {
        assert!(decode(b"\xEF\xBB\xBF{\"t\":\"hello\"}").is_some());
        assert!(decode(b"{\"t\":5}").is_none());
        assert!(decode(b"[1]").is_none());
        assert!(decode(b"null").is_none());
        assert!(decode(b"not json").is_none());
    }
}
