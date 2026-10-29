//! Which deployment the apps are built for (`api::HOME`): TOMOSHIBI_HOST from
//! the environment, else from deployment.local at the repository's root,
//! which is not committed, else from deployment.example beside it, whose
//! meet.example.com reaches nothing. Kept out of the source so that the
//! published source names no deployment of anybody's (the owner, 2026-10-22)
//! while every build of the apps still talks to one alone.
//!
//! And the key that deployment's builds are signed with
//! (`TOMOSHIBI_UPDATE_KEY`, api/updates.rs), found the same way. It belongs
//! to a deployment, so the example deployment has none: a build for
//! meet.example.com takes a key from the environment alone, which is how the
//! tests are built with one of their own, and never the real deployment's
//! from a deployment.local that happens to be beside it, which every update
//! the tests sign would then fail. A build whose debug assertions are off is
//! one that talks to HOME alone (api::admitted), the build that is given
//! out; for a real deployment it is refused here without a key, because an
//! app given out without one installs whatever the deployment hands it.

use std::{env, fs, path::Path};

/// The placeholder deployment, which reaches nothing and signs nothing.
const EXAMPLE_HOST: &str = "meet.example.com";

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let local = root.join("deployment.local");
    let example = root.join("deployment.example");
    println!("cargo:rerun-if-env-changed=TOMOSHIBI_HOST");
    println!("cargo:rerun-if-env-changed=TOMOSHIBI_UPDATE_KEY");
    println!("cargo:rerun-if-changed={}", local.display());
    println!("cargo:rerun-if-changed={}", example.display());

    let host = from_environment("TOMOSHIBI_HOST")
        .or_else(|| said_in(&local, "TOMOSHIBI_HOST"))
        .or_else(|| said_in(&example, "TOMOSHIBI_HOST"))
        .unwrap_or_else(|| EXAMPLE_HOST.to_owned());

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

    let key = from_environment("TOMOSHIBI_UPDATE_KEY")
        .or_else(|| {
            if host == EXAMPLE_HOST {
                return None;
            }
            said_in(&local, "TOMOSHIBI_UPDATE_KEY")
                .or_else(|| said_in(&example, "TOMOSHIBI_UPDATE_KEY"))
        })
        .unwrap_or_default()
        .to_ascii_lowercase();
    // "none" says outright that a build checks no signatures: a debug build
    // pointed at a local deployment whose manifest nobody signed. Written
    // rather than left out, so that a key forgotten is not taken for one
    // declined.
    let key = if key == "none" { String::new() } else { key };
    assert!(
        key.is_empty() || (key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit())),
        "TOMOSHIBI_UPDATE_KEY is {key:?}, which is not the 64 hexadecimal digits of an \
         Ed25519 public key (see deployment.example)"
    );

    // The target's debug assertions, which are what api::admitted and the
    // release check go by; the build script's own cfg!(debug_assertions)
    // would be the host's.
    let given_out = env::var_os("CARGO_CFG_DEBUG_ASSERTIONS").is_none();
    assert!(
        !(given_out && key.is_empty() && host != EXAMPLE_HOST),
        "this is a release build for {host} with no TOMOSHIBI_UPDATE_KEY: its apps would \
         install any build the deployment hands them. Put the public half of the \
         deployment's release key beside TOMOSHIBI_HOST (see deployment.example)"
    );
    println!("cargo:rustc-env=TOMOSHIBI_UPDATE_KEY={key}");
}

fn from_environment(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// The value a file of the xcconfig kind gives `name`, `// comments` allowed.
fn said_in(file: &Path, name: &str) -> Option<String> {
    fs::read_to_string(file).ok()?.lines().find_map(|line| {
        let line = line.split("//").next()?.trim();
        let (key, value) = line.split_once('=')?;
        (key.trim() == name)
            .then(|| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    })
}
