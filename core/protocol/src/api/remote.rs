//! What the deployment tells the apps between their releases: wording and
//! translations laid over the apps' own phrase tables, features switched off
//! for some platforms or versions, numbers the apps read where they would
//! otherwise use their own, and a notice for the lobby. The owner, 2026-10-22:
//! a small change should not each time need a new build.
//!
//! `GET /api/apps/remote` (internal/app/remote.go in the deployment's source)
//! answers for the app asking, by the platform and version in its user agent:
//! the deployment's rules are resolved there, and what arrives is what
//! applies to this app. An app keeps the last answer (`kept`) and reads it
//! back with `parse_remote_config` when the deployment cannot be reached, so
//! that a feature switched off stays off on a train and a translation laid
//! over stays laid over.
//!
//! Read leniently, a section at a time. An answer with one section written
//! wrongly still says the others; a phrase, a number or a notice that is not
//! what it should be is left out rather than failing the rest, because what
//! is lost then is a change of wording, where failing the whole would be
//! every switch the deployment had turned off.

use std::{
    collections::HashMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use reqwest::Method;
use serde_json::Value;

use super::{updates::notes_in, Api, Deployment};
use crate::{runtime::on_core, ProtocolError};

/// What the deployment says to this app (see the module).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RemoteConfig {
    /// The deployment's name for this answer: two with the same revision say
    /// the same thing.
    pub revision: String,
    /// Phrases by the English the apps' tables are keyed by, then by
    /// language tag (zh-Hans, zh-Hant, ja, en). See `remote_phrases`.
    pub phrases: HashMap<String, HashMap<String, String>>,
    /// The features switched off for this app, among screen-share,
    /// background-blur, watch-together, music and app-updates. A name an app
    /// does not know is a feature a later app has, and is nothing to it.
    pub off: Vec<String>,
    /// Numbers by name, among screen_share_max_fps, screen_share_max_height,
    /// screen_share_hot_fps, update_check_hours and relay_probe_rounds; a name
    /// not here is the app's own default.
    pub params: HashMap<String, f64>,
    /// The notice for the lobby, where there is one and it has not run out.
    pub notice: Option<RemoteNotice>,
    /// The answer as it came, for the app to keep and read again.
    pub kept: String,
}

/// A notice the lobby shows above everything else in it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RemoteNotice {
    /// Stays the same while the notice is the same one, so that an app can
    /// tell a new notice from the one somebody has already put away.
    pub id: String,
    /// A notice to take care about (the deployment's "warning") rather than
    /// only to know.
    pub warning: bool,
    /// By language tag. See `remote_notice_text`.
    pub text: HashMap<String, String>,
    /// Seconds since 1970 after which it is not shown; none for a notice
    /// shown until it is taken down.
    pub until: Option<i64>,
}

/// An answer kept from before, read again; none where it is not one at all.
/// A notice that has run out since is left out.
#[uniffi::export]
pub fn parse_remote_config(kept: String) -> Option<RemoteConfig> {
    read_config(&kept, now())
}

/// The notice's words for somebody reading `language`, by the same rule as an
/// update's notes: that language, else it without its region or script, else
/// English, else none, which is no notice rather than one in a language the
/// reader may not read.
#[uniffi::export]
pub fn remote_notice_text(notice: RemoteNotice, language: String) -> Option<String> {
    notes_in(&notice.text, &language)
}

/// The phrases laid over the app's own for somebody reading `language`: by
/// English key, each in that language or in it without its region or
/// script. No English stand-in, unlike a notice: where the deployment has not
/// reworded a phrase in the reader's language, the app's own translation is
/// the one to show, not the deployment's English.
#[uniffi::export]
pub fn remote_phrases(config: RemoteConfig, language: String) -> HashMap<String, String> {
    config
        .phrases
        .iter()
        .filter_map(|(key, by)| in_language(by, &language).map(|text| (key.clone(), text)))
        .collect()
}

#[uniffi::export]
impl Deployment {
    /// Asks the deployment what it says to this app now (see the module). An
    /// answer that is not one is an error, and the app goes on with what it
    /// kept.
    pub async fn remote_config(self: Arc<Self>) -> Result<RemoteConfig, ProtocolError> {
        on_core(async move { self.api.remote_config().await }).await
    }
}

impl Api {
    async fn remote_config(&self) -> Result<RemoteConfig, ProtocolError> {
        let answer = self
            .send(self.request(Method::GET, "/api/apps/remote")?)
            .await?
            .accepted("")?;
        let raw = String::from_utf8(answer.body)
            .map_err(|_| super::unexpected("what the deployment says to the apps is not text"))?;
        read_config(&raw, now()).ok_or_else(|| {
            super::unexpected("what the deployment says to the apps is not JSON it can read")
        })
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or(0)
}

fn read_config(raw: &str, now: i64) -> Option<RemoteConfig> {
    let body: Value = serde_json::from_str(raw).ok()?;
    let body = body.as_object()?;

    let phrases = body
        .get("phrases")
        .and_then(Value::as_object)
        .map(|phrases| {
            phrases
                .iter()
                .filter_map(|(key, by)| {
                    let by: HashMap<String, String> = by
                        .as_object()?
                        .iter()
                        .filter_map(|(tag, text)| Some((tag.clone(), text.as_str()?.to_owned())))
                        .filter(|(_, text)| !text.trim().is_empty())
                        .collect();
                    (!by.is_empty()).then(|| (key.clone(), by))
                })
                .collect()
        })
        .unwrap_or_default();

    let mut off: Vec<String> = body
        .get("off")
        .and_then(Value::as_array)
        .map(|off| {
            off.iter()
                .filter_map(Value::as_str)
                .map(|feature| feature.trim().to_owned())
                .filter(|feature| !feature.is_empty())
                .collect()
        })
        .unwrap_or_default();
    off.sort();
    off.dedup();

    let params = body
        .get("params")
        .and_then(Value::as_object)
        .map(|params| {
            params
                .iter()
                .filter_map(|(name, value)| {
                    Some((name.clone(), value.as_f64().filter(|n| n.is_finite())?))
                })
                .collect()
        })
        .unwrap_or_default();

    let notice = body
        .get("notice")
        .and_then(|notice| read_notice(notice, now));

    Some(RemoteConfig {
        revision: body
            .get("revision")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        phrases,
        off,
        params,
        notice,
        kept: raw.to_owned(),
    })
}

/// A notice, or none where it has no id, says nothing, says when it runs out
/// in a way that cannot be read (shown for ever would be worse than not at
/// all), or has run out.
fn read_notice(notice: &Value, now: i64) -> Option<RemoteNotice> {
    let id = notice.get("id")?.as_str()?.trim();
    if id.is_empty() {
        return None;
    }

    let text: HashMap<String, String> = notice
        .get("text")?
        .as_object()?
        .iter()
        .filter_map(|(tag, text)| Some((tag.clone(), text.as_str()?.trim().to_owned())))
        .filter(|(_, text)| !text.is_empty())
        .collect();
    if text.is_empty() {
        return None;
    }

    let until = match notice.get("until") {
        None | Some(Value::Null) => None,
        Some(Value::String(said)) if said.trim().is_empty() => None,
        Some(Value::String(said)) => Some(rfc3339_seconds(said)?),
        Some(_) => return None,
    };
    if until.is_some_and(|until| until <= now) {
        return None;
    }

    Some(RemoteNotice {
        id: id.to_owned(),
        warning: notice.get("level").and_then(Value::as_str) == Some("warning"),
        text,
        until,
    })
}

/// A phrase's words in `language` or in it without its region or script, and
/// nothing else.
fn in_language(by: &HashMap<String, String>, language: &str) -> Option<String> {
    let mut tag = language.trim();
    while !tag.is_empty() {
        if let Some((_, text)) = by.iter().find(|(key, _)| key.eq_ignore_ascii_case(tag)) {
            return Some(text.clone());
        }
        match tag.rfind('-') {
            Some(cut) => tag = &tag[..cut],
            None => break,
        }
    }
    None
}

/// Seconds since 1970 of an RFC 3339 time, as the deployment writes a
/// notice's end: 2026-10-23T12:00:00Z, with a fraction of a second or an
/// offset from UTC allowed.
fn rfc3339_seconds(said: &str) -> Option<i64> {
    let said = said.trim();
    let number = |from: usize, to: usize| -> Option<i64> {
        let part = said.get(from..to)?;
        part.bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| part.parse().ok())?
    };
    let bytes = said.as_bytes();
    if bytes.len() < 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !matches!(bytes[10], b'T' | b't' | b' ')
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }
    let (year, month, day) = (number(0, 4)?, number(5, 7)?, number(8, 10)?);
    let (hour, minute, second) = (number(11, 13)?, number(14, 16)?, number(17, 19)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }

    let mut rest = &said[19..];
    if let Some(fraction) = rest.strip_prefix('.') {
        let digits = fraction.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        rest = &fraction[digits..];
    }
    let offset = match rest {
        "Z" | "z" => 0,
        _ => {
            let sign = match rest.as_bytes().first()? {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let zone = &rest[1..];
            if zone.len() != 5 || zone.as_bytes()[2] != b':' {
                return None;
            }
            let (h, m): (i64, i64) = (zone[..2].parse().ok()?, zone[3..].parse().ok()?);
            if h > 23 || m > 59 {
                return None;
            }
            sign * (h * 3600 + m * 60)
        }
    };

    // Days from 1970-01-01 to the date (Howard Hinnant's days_from_civil).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;

    Some(days * 86400 + hour * 3600 + minute * 60 + second - offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_792_627_200; // 2026-10-22T00:00:00Z

    #[test]
    fn times_are_read_as_rfc_3339() {
        assert_eq!(rfc3339_seconds("2026-10-22T00:00:00Z"), Some(NOW));
        assert_eq!(rfc3339_seconds("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(rfc3339_seconds("2026-10-22T08:00:00+08:00"), Some(NOW));
        assert_eq!(rfc3339_seconds("2026-10-21T19:00:00-05:00"), Some(NOW));
        assert_eq!(rfc3339_seconds("2026-10-22T00:00:00.250Z"), Some(NOW));
        assert_eq!(
            rfc3339_seconds("2024-02-29T00:00:00Z"),
            Some(1_709_164_800),
            "a leap day"
        );
        for wrong in [
            "",
            "tomorrow",
            "2026-10-22",
            "2026-10-22T00:00:00",
            "2026-13-01T00:00:00Z",
            "2026-10-22T00:00:00+0800",
            "2026-10-22T00:00:00.Z",
        ] {
            assert_eq!(rfc3339_seconds(wrong), None, "{wrong:?}");
        }
    }

    #[test]
    fn a_notice_that_has_run_out_or_cannot_say_when_is_not_shown() {
        let notice = |until: Value| serde_json::json!({ "id": "n1", "level": "warning", "text": { "en": " Down tonight. " }, "until": until });
        let shown =
            read_notice(&notice(Value::String("2026-10-22T01:00:00Z".into())), NOW).unwrap();
        assert!(shown.warning);
        assert_eq!(shown.text["en"], "Down tonight.");
        assert_eq!(shown.until, Some(NOW + 3600));
        assert!(
            read_notice(&notice(Value::String("2026-10-22T00:00:00Z".into())), NOW).is_none(),
            "at its end"
        );
        assert!(read_notice(&notice(Value::String("next week".into())), NOW).is_none());
        assert!(read_notice(&notice(serde_json::json!(5)), NOW).is_none());
        assert_eq!(
            read_notice(&notice(Value::Null), NOW).unwrap().until,
            None,
            "until taken down"
        );
        assert!(
            read_notice(&serde_json::json!({ "text": { "en": "x" } }), NOW).is_none(),
            "no id"
        );
        assert!(
            read_notice(
                &serde_json::json!({ "id": "n2", "text": { "en": "  " } }),
                NOW
            )
            .is_none(),
            "nothing said"
        );
        assert!(
            !read_notice(
                &serde_json::json!({ "id": "n3", "level": "loud", "text": { "en": "x" } }),
                NOW
            )
            .unwrap()
            .warning
        );
    }

    #[test]
    fn a_section_written_wrongly_costs_only_itself() {
        let raw = r#"{ "revision": "ab12", "phrases": { "Sign out": { "zh-Hans": "退出", "ja": 3, "en": "" }, "Gone": "not a map" },
                        "off": ["music", 4, " screen-share ", "music", ""], "params": { "screen_share_max_fps": 30, "x": "fast" },
                        "notice": { "id": "", "text": { "en": "x" } } }"#;
        let config = read_config(raw, NOW).unwrap();
        assert_eq!(config.revision, "ab12");
        assert_eq!(config.phrases.len(), 1);
        assert_eq!(
            config.phrases["Sign out"],
            HashMap::from([("zh-Hans".to_owned(), "退出".to_owned())])
        );
        assert_eq!(config.off, ["music", "screen-share"]);
        assert_eq!(
            config.params,
            HashMap::from([("screen_share_max_fps".to_owned(), 30.0)])
        );
        assert_eq!(config.notice, None);
        assert_eq!(config.kept, raw);

        let broken = read_config(
            r#"{ "phrases": [], "off": {}, "params": 7, "notice": "x" }"#,
            NOW,
        )
        .unwrap();
        assert!(
            broken.phrases.is_empty()
                && broken.off.is_empty()
                && broken.params.is_empty()
                && broken.notice.is_none()
        );
        assert!(read_config("not json", NOW).is_none());
        assert!(read_config("[1]", NOW).is_none());
    }

    #[test]
    fn phrases_are_laid_over_in_the_readers_language_only() {
        let config = read_config(r#"{ "phrases": { "Sign out": { "zh-Hans": "退出", "en": "Leave" }, "Cancel": { "ja": "やめる" } } }"#, NOW).unwrap();
        let zh = remote_phrases(config.clone(), "zh-Hans".into());
        assert_eq!(
            zh,
            HashMap::from([("Sign out".to_owned(), "退出".to_owned())]),
            "no English stand-in for Cancel"
        );
        let ja = remote_phrases(config.clone(), "ja-JP".into());
        assert_eq!(
            ja,
            HashMap::from([("Cancel".to_owned(), "やめる".to_owned())]),
            "the language without its region"
        );
        assert_eq!(
            remote_phrases(config, "en".into()),
            HashMap::from([("Sign out".to_owned(), "Leave".to_owned())])
        );
    }

    #[test]
    fn a_notice_is_said_in_the_readers_language_or_english() {
        let notice = RemoteNotice {
            id: "n".into(),
            warning: false,
            text: HashMap::from([
                ("zh-Hans".to_owned(), "今晚维护".to_owned()),
                ("en".to_owned(), "Down tonight".to_owned()),
            ]),
            until: None,
        };
        assert_eq!(
            remote_notice_text(notice.clone(), "zh-Hans".into()).as_deref(),
            Some("今晚维护")
        );
        assert_eq!(
            remote_notice_text(notice, "ja".into()).as_deref(),
            Some("Down tonight")
        );
    }
}
