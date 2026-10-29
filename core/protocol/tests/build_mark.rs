//! What a packaging script reads out of a built core (api::BUILD_MARK) says
//! the same as what an app asks the core, in the shape the scripts look for:
//! `tomoshibi-core-build:<profile>:<host>:<update key>;`.

use tomoshibi_protocol::api::{core_is_release, home_deployment, home_host, BUILD_MARK, HOST};

#[test]
fn the_mark_says_what_the_core_answers() {
    let body = BUILD_MARK
        .strip_prefix("tomoshibi-core-build:")
        .and_then(|rest| rest.strip_suffix(';'))
        .unwrap_or_else(|| panic!("{BUILD_MARK:?} is not in the shape the scripts read"));
    let fields: Vec<&str> = body.split(':').collect();
    assert_eq!(fields.len(), 3, "{BUILD_MARK:?}");

    assert_eq!(fields[0], if core_is_release() { "release" } else { "debug" });
    assert_eq!(core_is_release(), !cfg!(debug_assertions));
    assert_eq!(fields[1], home_host());
    assert_eq!(home_host(), HOST);
    assert_eq!(home_deployment(), format!("https://{HOST}/"));
    assert!(
        fields[2].is_empty() || (fields[2].len() == 64 && fields[2].bytes().all(|b| b.is_ascii_hexdigit())),
        "the update key field is {:?}",
        fields[2]
    );
}

/// The scripts' pattern, which a host or key with any other character
/// would slip past: a host is lower-case letters, digits, dots and dashes
/// (build.rs refuses anything else), and a key is hexadecimal.
#[test]
fn the_mark_has_nothing_the_scripts_pattern_would_stop_at() {
    assert!(BUILD_MARK
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'.' | b':' | b';')));
    assert_eq!(BUILD_MARK.matches(';').count(), 1);
}
