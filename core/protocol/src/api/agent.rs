//! What the apps call themselves to the deployment: the user agent on every
//! request a `Deployment` sends.
//!
//! The deployment's management pages say what each person joined from (the
//! server's internal/app/device.go), because "what are you using" is the first
//! thing anybody helping somebody asks. A browser says it in its user agent.
//! The apps sent none, reqwest adding nothing of its own, so every app looked
//! alike, and like no browser. Now each says
//!
//! ```text
//! Tomoshibi/0.1.0.8 (iPhone; iOS 27.0.1; iPhone17,1)
//! ```
//!
//! with the version in the dotted form the download page uses (0.1.0 build 8
//! is 0.1.0.8), then the platform, the system and the model. The platform is
//! the one place the server reads a fixed word from: iPhone, iPad, Mac,
//! Android or Windows.
//!
//! Set here once rather than by each app at each request, so that one change
//! named every platform the core runs on. An app that never calls
//! `describe_client` still says which platform it was built for, from the
//! target, and the core's own version; it cannot say iPad, which is the same
//! target as iPhone, so the Apple apps describe themselves.

use std::sync::RwLock;

/// What the app said about itself, if it has.
static DESCRIBED: RwLock<Option<Described>> = RwLock::new(None);

#[derive(Clone, Default)]
struct Described {
    platform: String,
    version: String,
    system: String,
    device: String,
}

/// Says which app this is, for the user agent every `Deployment` made after it
/// sends: `platform` one of iPhone, iPad, Mac, Android or Windows, `version`
/// the app's in dotted form (0.1.0.8), `system` such as "iOS 27.0.1", and
/// `device` the model, such as "iPhone17,1". Any of them may be empty. Called
/// once at start, before the first deployment is made; a deployment reads it
/// when it is made and keeps it.
#[uniffi::export]
pub fn describe_client(platform: String, version: String, system: String, device: String) {
    let described = Described { platform, version, system, device };
    // A poisoned lock is somebody's panic elsewhere, and a user agent is not a
    // reason to make it two.
    match DESCRIBED.write() {
        Ok(mut held) => *held = Some(described),
        Err(poisoned) => *poisoned.into_inner() = Some(described),
    }
}

/// The user agent a deployment made now sends.
pub(crate) fn user_agent() -> String {
    let described = match DESCRIBED.read() {
        Ok(held) => held.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    };
    agent_of(described.as_ref())
}

fn agent_of(described: Option<&Described>) -> String {
    let built = Described {
        platform: built_for().into(),
        version: env!("CARGO_PKG_VERSION").into(),
        ..Default::default()
    };
    let one = described.unwrap_or(&built);

    let version = match clean(&one.version) {
        version if version.is_empty() => clean(&built.version),
        version => version.replace(' ', "."),
    };
    let platform = match clean(&one.platform) {
        platform if platform.is_empty() => built.platform.clone(),
        platform => platform,
    };

    let mut detail = vec![platform];
    for part in [&one.system, &one.device] {
        let part = clean(part);
        if !part.is_empty() {
            detail.push(part);
        }
    }

    format!("Tomoshibi/{version} ({})", detail.join("; "))
}

/// The platform the core was compiled for, which is all it knows by itself.
fn built_for() -> &'static str {
    if cfg!(target_os = "ios") {
        "iPhone"
    } else if cfg!(target_os = "macos") {
        "Mac"
    } else if cfg!(target_os = "android") {
        "Android"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else {
        "Other"
    }
}

/// A part fit for the header: printable ASCII only, since a header value that
/// is not refuses to be built and would take the deployment with it, and none
/// of the three characters the server splits on. A phone named in another
/// script loses the name and keeps the rest.
fn clean(part: &str) -> String {
    let kept: String = part
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .filter(|c| !matches!(c, '(' | ')' | ';'))
        .collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(platform: &str, version: &str, system: &str, device: &str) -> Described {
        Described { platform: platform.into(), version: version.into(), system: system.into(), device: device.into() }
    }

    #[test]
    fn an_app_that_describes_itself_says_so() {
        assert_eq!(
            agent_of(Some(&said("iPhone", "0.1.0.8", "iOS 27.0.1", "iPhone17,1"))),
            "Tomoshibi/0.1.0.8 (iPhone; iOS 27.0.1; iPhone17,1)"
        );
        assert_eq!(
            agent_of(Some(&said("Android", "0.1.0.1", "Android 16", "samsung SM-F956B"))),
            "Tomoshibi/0.1.0.1 (Android; Android 16; samsung SM-F956B)"
        );
    }

    #[test]
    fn an_app_that_does_not_still_says_its_platform() {
        let agent = agent_of(None);
        assert!(agent.starts_with(&format!("Tomoshibi/{} (", env!("CARGO_PKG_VERSION"))), "{agent}");
        assert!(agent.ends_with(&format!("({})", built_for())), "{agent}");
    }

    #[test]
    fn what_cannot_go_in_a_header_is_left_out() {
        // Parentheses and semicolons would split the platform wrongly at the
        // server; anything not ASCII would stop the client being built.
        let agent = agent_of(Some(&said("Android", "0.1 (2)", "Android 16; beta", "小米 (Pro)")));
        assert_eq!(agent, "Tomoshibi/0.1.2 (Android; Android 16 beta; Pro)");
        assert!(reqwest::header::HeaderValue::from_str(&agent).is_ok());
    }

    #[test]
    fn empty_parts_are_not_said() {
        assert_eq!(agent_of(Some(&said("Windows", "0.1.0.1", "", ""))), "Tomoshibi/0.1.0.1 (Windows)");
        let agent = agent_of(Some(&said("", "", "", "")));
        assert_eq!(agent, format!("Tomoshibi/{} ({})", env!("CARGO_PKG_VERSION"), built_for()));
    }
}
