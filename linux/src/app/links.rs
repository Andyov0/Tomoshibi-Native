//! A link opened with the app, taken apart into what says where it goes: the
//! origin it is for, and the room, invitation and arranged meeting it
//! carries (`https://<deployment>/?invite=T#/room`, `?meeting=T#/room`, or
//! the same under the `tomoshibi:` scheme), as Refusals.cs `OpenedLink` and
//! Android's AppModel.kt `OpenedLink` read it.

/// The parts of a link that say where it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenedLink {
    /// `scheme://host[:port]`, lower case, without a default port; the
    /// app's own scheme read as https.
    pub origin: String,
    /// The fragment's room, decoded; empty where there is none.
    pub room: String,
    pub invite: Option<String>,
    pub meeting: Option<String>,
}

impl OpenedLink {
    /// Whether this is a link into the deployment at `base`: compared
    /// without the trailing slash a deployment's address always has and a
    /// link's origin never does.
    pub fn is_for(&self, base: &str) -> bool {
        self.origin == base.trim_end_matches('/').to_ascii_lowercase()
    }

    pub fn parse(link: &str) -> Option<OpenedLink> {
        let link = link.trim();
        let (scheme, rest) = link.split_once("://")?;
        let mut scheme = scheme.to_ascii_lowercase();
        if !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
            || scheme.is_empty()
        {
            return None;
        }
        if scheme == "tomoshibi" {
            scheme = "https".into();
        }
        let (before_fragment, fragment) = match rest.split_once('#') {
            Some((before, fragment)) => (before, fragment),
            None => (rest, ""),
        };
        let (before_query, query) = match before_fragment.split_once('?') {
            Some((before, query)) => (before, query),
            None => (before_fragment, ""),
        };
        let authority = before_query.split('/').next().unwrap_or_default();
        // Whoever is named before an `@` is not part of where it goes.
        let host_port = authority.rsplit('@').next().unwrap_or_default();
        let (host, port) = split_port(host_port)?;
        if host.is_empty() || host.contains(char::is_whitespace) {
            return None;
        }
        let host = host.to_ascii_lowercase();
        let port = port.filter(|port| {
            !((scheme == "https" && *port == 443) || (scheme == "http" && *port == 80))
        });
        let origin = match port {
            Some(port) => format!("{scheme}://{host}:{port}"),
            None => format!("{scheme}://{host}"),
        };
        let room = decode(fragment.strip_prefix('/').unwrap_or(fragment));
        let first = |name: &str| {
            query
                .split('&')
                .filter(|item| !item.is_empty())
                .map(|item| match item.split_once('=') {
                    Some((key, value)) => (decode(key), decode(value)),
                    None => (decode(item), String::new()),
                })
                .find(|(key, _)| key == name)
                .map(|(_, value)| value)
        };
        Some(OpenedLink {
            origin,
            room,
            invite: first("invite"),
            meeting: first("meeting"),
        })
    }
}

/// `host` and `port` from `host:port`, an IPv6 literal kept in its brackets.
fn split_port(authority: &str) -> Option<(&str, Option<u16>)> {
    if authority.starts_with('[') {
        let close = authority.find(']')?;
        let host = &authority[..=close];
        let rest = &authority[close + 1..];
        return match rest.strip_prefix(':') {
            Some(port) if !port.is_empty() => Some((host, Some(port.parse().ok()?))),
            _ => Some((host, None)),
        };
    }
    match authority.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() => Some((host, Some(port.parse().ok()?))),
        Some((host, _)) => Some((host, None)),
        None => Some((authority, None)),
    }
}

/// Percent-decoding as a URL's parts are read: a plus is a plus, not a
/// space, and a sequence that does not decode is kept as written.
pub fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' && at + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[at + 1..at + 3]).ok();
            if let Some(byte) = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                out.push(byte);
                at += 3;
                continue;
            }
        }
        out.push(bytes[at]);
        at += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_invitation_is_its_origin_room_and_token() {
        let link =
            OpenedLink::parse("https://Meet.Example.com/?invite=abc%2Bd#/team-sync").unwrap();
        assert_eq!(link.origin, "https://meet.example.com");
        assert_eq!(link.room, "team-sync");
        assert_eq!(link.invite.as_deref(), Some("abc+d"));
        assert_eq!(link.meeting, None);
        assert!(link.is_for("https://meet.example.com/"));
        assert!(!link.is_for("https://other.example.com/"));
    }

    #[test]
    fn the_apps_scheme_is_https_and_default_ports_go() {
        let link = OpenedLink::parse("tomoshibi://meet.example.com:443/?meeting=m1#/r").unwrap();
        assert_eq!(link.origin, "https://meet.example.com");
        assert_eq!(link.meeting.as_deref(), Some("m1"));
        let local = OpenedLink::parse("http://127.0.0.1:20100/#/room-1").unwrap();
        assert_eq!(local.origin, "http://127.0.0.1:20100");
        assert!(local.is_for("http://127.0.0.1:20100/"));
        assert_eq!(local.invite, None);
    }

    #[test]
    fn a_plus_is_a_plus_and_a_broken_escape_is_kept() {
        assert_eq!(decode("a+b%20c"), "a+b c");
        assert_eq!(decode("100%"), "100%");
        assert_eq!(decode("%zz"), "%zz");
        assert_eq!(decode("%E4%BC%9A"), "\u{4f1a}");
    }

    #[test]
    fn what_is_not_a_link_is_nothing() {
        assert_eq!(OpenedLink::parse("meet.example.com"), None);
        assert_eq!(OpenedLink::parse("https:///#/room"), None);
        assert_eq!(OpenedLink::parse("https://host:notaport/"), None);
    }
}
