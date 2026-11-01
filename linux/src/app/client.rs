//! What this app tells the deployment it is, for the deployment's
//! management pages: Linux, the version with its build, the system and the
//! machine. The core puts it in the user agent of every request
//! (core/protocol/src/api/agent.rs), as each app's ClientDescription does,
//! and it must be said before the first Deployment is made, which reads it
//! when it is made.

/// The product's version (Cargo.toml), the same on every platform.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The build within the version. The owner's rule (2026-10-22): a small fix
/// keeps the version and takes the next build; a large update takes a new
/// version and starts again at 1. Raised with the other platforms' numbers.
pub const BUILD: &str = "1";

/// The version as the update check compares it and the deployment offers it:
/// 0.1.7 for build 1, 0.1.7.2 for build 2 (the core's version_with_build).
pub fn dotted_version() -> String {
    tomoshibi_protocol::api::version_with_build(VERSION.to_owned(), BUILD.to_owned())
}

/// The version as people are told it: 0.1.7 (2).
pub fn version_label() -> String {
    tomoshibi_protocol::api::version_label(dotted_version())
}

/// Says which app this is, once, at start.
pub fn tell() {
    tomoshibi_protocol::api::describe_client("Linux".into(), dotted_version(), system(), device());
}

/// The distribution as it names itself (`PRETTY_NAME` in os-release), or the
/// kernel's name and release where there is none.
pub fn system() -> String {
    let release = std::fs::read_to_string("/etc/os-release")
        .or_else(|_| std::fs::read_to_string("/usr/lib/os-release"))
        .unwrap_or_default();
    pretty_name(&release).unwrap_or_else(|| "Linux".to_owned())
}

fn pretty_name(os_release: &str) -> Option<String> {
    os_release.lines().find_map(|line| {
        let value = line.strip_prefix("PRETTY_NAME=")?;
        let value = value.trim().trim_matches('"').trim_matches('\'');
        (!value.is_empty()).then(|| value.to_owned())
    })
}

/// The maker and the model, as the firmware tells the kernel them, and the
/// architecture; the agent drops what cannot go in a header.
pub fn device() -> String {
    let read = |name: &str| {
        std::fs::read_to_string(format!("/sys/class/dmi/id/{name}"))
            .map(|text| text.trim().to_owned())
            .unwrap_or_default()
    };
    let mut parts: Vec<String> = [read("sys_vendor"), read("product_name")]
        .into_iter()
        .filter(|part| !part.is_empty() && !part.eq_ignore_ascii_case("to be filled by o.e.m."))
        .collect();
    parts.push(std::env::consts::ARCH.to_owned());
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_build_is_the_version_alone() {
        assert_eq!(
            dotted_version(),
            if BUILD == "1" {
                VERSION.to_owned()
            } else {
                format!("{VERSION}.{BUILD}")
            }
        );
    }

    #[test]
    fn the_system_is_its_pretty_name() {
        assert_eq!(
            pretty_name("NAME=\"Ubuntu\"\nPRETTY_NAME=\"Ubuntu 24.04.4 LTS\"\n").as_deref(),
            Some("Ubuntu 24.04.4 LTS")
        );
        assert_eq!(pretty_name("NAME=Arch\n"), None);
    }
}
