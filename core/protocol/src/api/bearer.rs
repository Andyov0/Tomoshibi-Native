//! Which of a call's two tokens a deployment takes at its host endpoints.
//!
//! A call holds two: the token the join returned, which the control node
//! signed and marked with its proof, and the one the media server replaces it
//! with as soon as the call is up and every five minutes after
//! (livekit-server `RoomManager.refreshToken`). Deployments disagree about
//! them. One that checks the proof (the server's host.go `whoIs`) takes only
//! the join's, past its expiry too while its holder is in the room, and never
//! the media server's. One that does not takes any token that verifies, so the
//! join's only until it expires (five minutes and the verifier's minute), and
//! the media server's, refreshed, for the whole call. A web page sends the
//! join's and lost its controls after five minutes on the second kind; an
//! app sending only the media server's was refused everything on the first.
//!
//! So the apps send the join's (`Call::token`, the key the host requests are
//! made with) and the call registers both here. A host request that is
//! refused as "not yours" may be refused for its token or for its person, and
//! both deployments answer the two with the same 403 and code; only the
//! standing question tells them apart, answering 401 to a token it does not
//! take and an answer to one it does. So such a refusal is followed by that
//! question with the same token: a 401, and the request is made once more with
//! the other token, which is then the one this call sends first; anything else,
//! and the refusal stands as it was, for it is about the person.

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock, Weak,
    },
};

use super::Answer;

/// One call's two tokens, and which one the deployment took last.
pub struct CallTokens {
    join: String,
    media: Box<dyn Fn() -> Option<String> + Send + Sync>,
    /// Whether the media server's token is the one to send first: set when it
    /// was taken after the join's was refused, cleared when the join's is
    /// taken again.
    media_first: AtomicBool,
}

fn calls() -> &'static Mutex<HashMap<String, Weak<CallTokens>>> {
    static CALLS: OnceLock<Mutex<HashMap<String, Weak<CallTokens>>>> = OnceLock::new();
    CALLS.get_or_init(Default::default)
}

/// Makes a call's tokens known to the host requests, under its join token.
/// The call keeps what this returns; when it goes, so does the entry.
/// `media` says the media server's current token, or nothing once the call is
/// gone.
pub fn register(join: String, media: impl Fn() -> Option<String> + Send + Sync + 'static) -> Arc<CallTokens> {
    let tokens = Arc::new(CallTokens {
        join: join.clone(),
        media: Box::new(media),
        media_first: AtomicBool::new(false),
    });
    let mut calls = calls().lock().unwrap_or_else(|poison| poison.into_inner());
    calls.retain(|_, held| held.strong_count() > 0);
    calls.insert(join, Arc::downgrade(&tokens));
    tokens
}

/// The tokens of the call whose join token this is, while that call lasts.
pub(crate) fn of(bearer: &str) -> Option<Arc<CallTokens>> {
    calls().lock().unwrap_or_else(|poison| poison.into_inner()).get(bearer)?.upgrade()
}

impl CallTokens {
    /// The token to send first.
    pub(crate) fn first(&self) -> String {
        if self.media_first.load(Ordering::SeqCst) {
            if let Some(media) = (self.media)() {
                return media;
            }
        }
        self.join.clone()
    }

    /// The token that was not `sent`, where there is another one.
    pub(crate) fn other(&self, sent: &str) -> Option<String> {
        let other = if sent == self.join { (self.media)()? } else { self.join.clone() };
        (other != sent).then_some(other)
    }

    /// The deployment took `sent`: it is the one to send first from now on.
    pub(crate) fn took(&self, sent: &str) {
        self.media_first.store(sent != self.join, Ordering::SeqCst);
    }
}

/// Whether an answer is the refusal a deployment gives a token it does not
/// take, or a person it does not let act, which look the same: a 401 or 403
/// with `not_yours`, or with no code at all.
pub(crate) fn not_yours(answer: &Answer) -> bool {
    matches!(answer.status, 401 | 403) && answer.code().is_none_or(|code| code == "not_yours")
}
