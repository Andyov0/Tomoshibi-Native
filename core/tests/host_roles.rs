//! Who may run a room from inside a call, with the token the apps send.
//!
//! The media server replaces a call's token with one of its own as soon as the
//! call is up. The apps sent that one to the host's controls, and a deployment
//! that takes only tokens it signed itself (a proof in the JWT ID; the
//! server's host.go `whoIs`) answered "not yours" to the host of a room from
//! the first second. And an administrator runs somebody else's room there only
//! by a session, never by the passphrase typed into the join form. Here every
//! role sits in one real call and tries every host endpoint with what
//! `Call::token` gives, and with what the media server holds, against
//! whichever deployment is running: one handing out guest proofs is taken to
//! have both changes (they shipped together), one without them neither.
//!
//! Against a local deployment, `TOMOSHIBI_LOCAL_DEPLOYMENT` or
//! http://127.0.0.1:8080, with rooms opened and joined by anyone, and two of
//! its administrators: one by passphrase alone
//! (`TOMOSHIBI_TEST_ADMIN_PASSPHRASE`) and one signed in to its account
//! (`TOMOSHIBI_TEST_ADMIN_ACCOUNT`, `TOMOSHIBI_TEST_ADMIN_ACCOUNT_PASSPHRASE`).
//! Skipped, saying so, without them. Only a loopback address is ever used.
//! `TOMOSHIBI_TEST_LATE=1` waits until the join's token has been given up
//! (the deployment's `token_ttl`, five minutes by default) and asks again.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use tomoshibi_core::call::{join_call, Call, CallEvent, CallObserver, CallOptions};
use tomoshibi_protocol::{api::Deployment, ProtocolError};

struct Quiet;

impl CallObserver for Quiet {
    fn on_event(&self, _event: CallEvent) {}
}

struct Setting {
    base: String,
    admin_passphrase: String,
    account: String,
    account_passphrase: String,
}

async fn live() -> Option<Setting> {
    let base = std::env::var("TOMOSHIBI_LOCAL_DEPLOYMENT").unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    let host = reqwest::Url::parse(&base).ok()?.host_str()?.to_owned();
    assert!(
        matches!(host.as_str(), "127.0.0.1" | "localhost" | "[::1]"),
        "{base} is not a local deployment, and nothing here is sent anywhere else"
    );
    let env = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
    let (Some(admin_passphrase), Some(account), Some(account_passphrase)) = (
        env("TOMOSHIBI_TEST_ADMIN_PASSPHRASE"),
        env("TOMOSHIBI_TEST_ADMIN_ACCOUNT"),
        env("TOMOSHIBI_TEST_ADMIN_ACCOUNT_PASSPHRASE"),
    ) else {
        eprintln!("skipped: no administrators named for {base}");
        return None;
    };
    match Deployment::new(base.clone()).unwrap().deployment_info().await {
        Ok(_) => Some(Setting { base, admin_passphrase, account, account_passphrase }),
        Err(ProtocolError::Network { detail }) => {
            eprintln!("skipped: no deployment answering at {base} ({detail})");
            None
        }
        Err(other) => panic!("the deployment at {base} answered oddly: {other:?}"),
    }
}

/// One participant: the deployment it talks to (with its own cookie jar,
/// holding the account session where there is one), its call, and the token
/// the join handed it.
struct Seat {
    role: &'static str,
    deployment: Arc<Deployment>,
    call: Arc<Call>,
    joined_with: String,
    proof: Option<String>,
}

async fn seat(base: &str, room: &str, role: &'static str, passphrase: &str, account: Option<(&str, &str)>) -> Seat {
    let deployment = Deployment::new(base.to_owned()).unwrap();
    if let Some((name, passphrase)) = account {
        deployment.clone().sign_in(name.to_owned(), passphrase.to_owned()).await.expect("sign-in");
    }
    let grant = deployment
        .clone()
        .join(room.to_owned(), role.to_owned(), passphrase.to_owned(), None, None, String::new(), None)
        .await
        .expect("join");
    let call = join_call(
        CallOptions { url: grant.url, token: grant.token.clone(), room: grant.room, sealing_word: None, forward: None },
        Arc::new(Quiet),
    )
    .await
    .expect("call");
    Seat { role, deployment, call, joined_with: grant.token, proof: grant.proof }
}

/// What a deployment said to one request: `ok`, or the refusal's status and
/// code. Authorised and then failing for another reason (`no_such_relay`,
/// `no_such_person`, `media_unreachable`) is reported as it came, and read as
/// let through.
fn said<T>(answer: Result<T, ProtocolError>) -> String {
    match answer {
        Ok(_) => "ok".into(),
        Err(ProtocolError::Refused { status, code }) => format!("{status} {code}"),
        Err(other) => format!("failed: {other}"),
    }
}

fn let_through(answer: &str) -> bool {
    answer == "ok" || answer.ends_with("no_such_relay") || answer.ends_with("no_such_person") || answer.ends_with("media_unreachable")
}

/// Every host endpoint, asked by `seat` with `bearer`: the standing, then each
/// control, with removal and muting aimed at `victim`, who is a guest in the
/// call (and gone afterwards if the removal was let through).
async fn ask_everything(seat: &Seat, room: &str, bearer: &str, victim: &Seat) -> Vec<(&'static str, String)> {
    let d = &seat.deployment;
    let (room, bearer) = (room.to_owned(), bearer.to_owned());
    let standing = match d.clone().host_standing(room.clone(), bearer.clone()).await {
        Ok(standing) => format!("yours={} admin={}", standing.yours, standing.admin),
        other => said(other),
    };
    let target = victim.call.local_identity();
    vec![
        ("standing", standing),
        ("invite", said(d.clone().create_invite(room.clone(), bearer.clone()).await)),
        ("door", said(d.clone().knocking(room.clone(), bearer.clone()).await)),
        ("revoke", said(d.clone().revoke_invites(room.clone(), Some(bearer.clone())).await)),
        ("move", said(d.clone().move_room(room.clone(), bearer.clone(), "nowhere".into()).await)),
        ("handover", said(d.clone().hand_over(room.clone(), bearer.clone(), "x".into()).await)),
        ("mute", said(d.clone().silence(room.clone(), bearer.clone(), target.clone(), "TR_none".into()).await)),
        ("remove", said(d.clone().turn_out(room.clone(), bearer.clone(), target).await)),
    ]
}

/// What each role may do with each token, on a deployment with the proof
/// (`hardened`) and on one without. `standing` is the panel's question, which
/// only a token the deployment signed can ask; the rest are the controls.
fn expected(hardened: bool, role: &str, media_token: bool, late: bool) -> (Option<(bool, bool)>, bool) {
    // A token the deployment accepts as its own, for this request.
    let vouched = !hardened || (!media_token && !late);
    match (role, hardened) {
        ("host", _) => (vouched.then_some((true, false)), vouched),
        ("guest", _) => (vouched.then_some((false, false)), false),
        // Only a passphrase: an administrator by token on the old deployment,
        // nobody's host on the new one.
        ("passphrase admin", false) => (Some((true, true)), true),
        ("passphrase admin", true) => (vouched.then_some((false, false)), false),
        // Signed in: the session settles it whatever the token, though the
        // panel's question still needs one the deployment signed.
        ("signed-in admin", _) => (vouched.then_some((true, true)), true),
        _ => unreachable!(),
    }
}

fn check(hardened: bool, seat: &Seat, media_token: bool, late: bool, answers: &[(&'static str, String)]) -> Vec<String> {
    let (standing, controls) = expected(hardened, seat.role, media_token, late);
    let token = if media_token { "media server's token" } else { "Call::token" };
    let mut wrong = Vec::new();
    for (what, answer) in answers {
        let fine = if *what == "standing" {
            match standing {
                Some((yours, admin)) => *answer == format!("yours={yours} admin={admin}"),
                None => answer == "401 not_yours",
            }
        } else {
            let_through(answer) == controls && (controls || answer == "403 not_yours")
        };
        println!("{:>16} | {:<20} | {:<8} | {answer}{}", seat.role, token, what, if fine { "" } else { "   <-- not as expected" });
        if !fine {
            wrong.push(format!("{} with the {token}: {what} answered {answer}", seat.role));
        }
    }
    wrong
}

async fn wait_for(what: &str, limit: Duration, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + limit;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn every_role_can_do_what_the_deployment_lets_it_with_the_token_the_apps_send() {
    let Some(setting) = live().await else { return };
    let base = setting.base.as_str();
    let room = format!("roles-{}", std::process::id());
    let late = std::env::var("TOMOSHIBI_TEST_LATE").is_ok_and(|value| value == "1");

    // The host first: a passphrase opening a room nobody has used is its host.
    let host_passphrase = format!("host-roles {}", tomoshibi_protocol::identity::generate_room_name());
    let host = seat(base, &room, "host", &host_passphrase, None).await;
    let guest = seat(base, &room, "guest", "", None).await;
    let passphrase_admin = seat(base, &room, "passphrase admin", &setting.admin_passphrase, None).await;
    let signed_in_admin =
        seat(base, &room, "signed-in admin", "", Some((&setting.account, &setting.account_passphrase))).await;
    let hardened = guest.proof.is_some();
    println!("deployment at {base}: {}", if hardened { "with the proofs" } else { "without the proofs" });
    assert_eq!(host.proof, None, "a passphrase keeps its identity without a proof");
    assert_eq!(signed_in_admin.proof, None, "a session keeps its identity without a proof");

    // The media server's own token arrives straight after the connect.
    let seats = [&host, &guest, &passphrase_admin, &signed_in_admin];
    wait_for("the media server's token", Duration::from_secs(20), || {
        seats.iter().all(|seat| seat.call.media_token() != seat.joined_with)
    })
    .await;
    for seat in seats {
        assert_eq!(seat.call.token(), seat.joined_with, "{}: the join's own token, while it is good", seat.role);
    }

    let mut wrong = Vec::new();
    let mut victims = Vec::new();
    let round = |late: bool| {
        let (seats, base, room) = (seats, base.to_owned(), room.clone());
        async move {
            let mut wrong = Vec::new();
            let mut victims = Vec::new();
            for seat in seats {
                for media_token in [true, false] {
                    let victim = self::seat(&base, &room, "guest", "", None).await;
                    let bearer = if media_token { seat.call.media_token() } else { seat.call.token() };
                    let answers = ask_everything(seat, &room, &bearer, &victim).await;
                    wrong.extend(check(hardened, seat, media_token, late, &answers));
                    victims.push(victim);
                }
            }
            (wrong, victims)
        }
    };

    let (said_wrong, left) = round(false).await;
    wrong.extend(said_wrong);
    victims.extend(left);

    if late {
        // Until the join's token is given up for the media server's, which
        // the deployment's token_ttl decides.
        println!("waiting for the join's token to be given up");
        wait_for("the join's token to be given up", Duration::from_secs(15 * 60), || {
            seats.iter().all(|seat| seat.call.token() != seat.joined_with)
        })
        .await;
        let (said_wrong, left) = round(true).await;
        wrong.extend(said_wrong);
        victims.extend(left);
    }

    // The host ends the meeting, with the join's token on a deployment with
    // the proof only while it is good.
    let closed = said(host.deployment.clone().dissolve(room.clone(), host.call.token()).await);
    println!("{:>16} | {:<20} | {:<8} | {closed}", "host", "Call::token", "close");
    let should_close = !(hardened && late);
    if (closed == "ok") != should_close {
        wrong.push(format!("host: close answered {closed}"));
    }

    for seat in seats.into_iter().chain(victims.iter()) {
        seat.call.clone().leave().await;
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}
