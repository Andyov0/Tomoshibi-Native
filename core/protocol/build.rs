//! Which deployment the apps are built for (`api::HOME`): TOMOSHIBI_HOST from
//! the environment, else from deployment.local at the repository's root,
//! which is not committed, else from deployment.example beside it, whose
//! meet.example.com reaches nothing. Kept out of the source so that the
//! published source names no deployment of anybody's (the owner, 2026-10-22)
//! while every build of the apps still talks to one alone.

use std::{env, fs, path::Path};

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let local = root.join("deployment.local");
    let example = root.join("deployment.example");
    println!("cargo:rerun-if-env-changed=TOMOSHIBI_HOST");
    println!("cargo:rerun-if-changed={}", local.display());
    println!("cargo:rerun-if-changed={}", example.display());

    let host = env::var("TOMOSHIBI_HOST")
        .ok()
        .map(|host| host.trim().to_owned())
        .filter(|host| !host.is_empty())
        .or_else(|| said_in(&local))
        .or_else(|| said_in(&example))
        .unwrap_or_else(|| "meet.example.com".to_owned());

    // A host name alone: a scheme, a path or a port here would make HOME a
    // different deployment from the one the links are for.
    let host = host.to_ascii_lowercase();
    assert!(
        !host.is_empty()
            && host.split('.').count() > 1
            && host.split('.').all(|label| {
                !label.is_empty()
                    && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                    && !label.starts_with('-')
                    && !label.ends_with('-')
            }),
        "TOMOSHIBI_HOST is {host:?}, which is not a host name (see deployment.example)"
    );
    println!("cargo:rustc-env=TOMOSHIBI_HOST={host}");
}

/// The TOMOSHIBI_HOST a file of the xcconfig kind sets, `// comments` allowed.
fn said_in(file: &Path) -> Option<String> {
    fs::read_to_string(file).ok()?.lines().find_map(|line| {
        let line = line.split("//").next()?.trim();
        let (key, value) = line.split_once('=')?;
        (key.trim() == "TOMOSHIBI_HOST")
            .then(|| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    })
}
