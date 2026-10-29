//! The deployment API against a real server: the shared local deployment at
//! http://127.0.0.1:8080 (full role, rooms opened and joined by anyone, no
//! administrators, no watch gateway and no music library configured).
//!
//! Skipped, saying so, where nothing answers there. Only a loopback address is
//! ever used, whatever `TOMOSHIBI_LOCAL_DEPLOYMENT` says, so these cannot be
//! pointed at the fleet.
//!
//! The deployment is shared, and two of its budgets are per address, which is
//! the same address for everybody on this machine: every join carrying a
//! passphrase, and every sign-in, is charged to the guessing budget (ten a
//! minute; internal/guess). So one run spends exactly two: one passphrase join,
//! which every host control below shares, and one refused sign-in.

use std::sync::Arc;

use base64::Engine;
use serde_json::Value;
use tomoshibi_protocol::{
    api::{Deployment, DoorState, Joining, MusicQuality, Opening},
    identity::{generate_room_name, signature_of, valid_identity},
    ProtocolError,
};

async fn live() -> Option<Arc<Deployment>> {
    let base = std::env::var("TOMOSHIBI_LOCAL_DEPLOYMENT")
        .unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    let host = url::Url::parse(&base).ok()?.host_str()?.to_owned();
    assert!(
        matches!(host.as_str(), "127.0.0.1" | "localhost" | "[::1]"),
        "{base} is not a local deployment, and nothing here is sent anywhere else"
    );

    let deployment = Deployment::new(base.clone()).unwrap();
    match deployment.clone().deployment_info().await {
        Ok(_) => Some(deployment),
        Err(ProtocolError::Network { detail: message }) => {
            eprintln!("skipped: no deployment answering at {base} ({message})");
            None
        }
        Err(other) => panic!("{other}"),
    }
}

fn refused(status: u16, code: &str) -> ProtocolError {
    ProtocolError::Refused {
        status,
        code: code.into(),
    }
}

fn s(value: &str) -> String {
    value.to_owned()
}

/// What a join token says about its bearer: the claims in its middle part.
fn claims(token: &str) -> Value {
    let middle = token.split('.').nth(1).expect("a JWT");
    serde_json::from_slice(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(middle)
            .unwrap(),
    )
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_deployment_says_what_it_is() {
    let Some(d) = live().await else { return };

    let info = d.clone().deployment_info().await.unwrap();
    assert_eq!(
        (info.opened_by, info.joined_by),
        (Opening::Anyone, Joining::Anyone)
    );
    assert!(info.source.starts_with("https://"), "{}", info.source);

    // A full deployment holds its own media and offers nothing to measure.
    let relays = d.relays().await.unwrap();
    assert!(relays.relays.is_empty() && !relays.measure);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_join_is_granted_and_the_identity_sent_back_is_kept() {
    let Some(d) = live().await else { return };
    let room = generate_room_name();

    let first = d
        .clone()
        .join(room.clone(), s("  Native Test  "), s(""), None, None, s(""), None)
        .await
        .unwrap();
    assert_eq!(first.room, room);
    assert!(first.url.starts_with("ws"), "{}", first.url);
    assert!(valid_identity(first.identity.clone()), "{}", first.identity);
    let mark = signature_of(first.identity.clone()).unwrap();
    assert!(
        !mark.proven && !mark.account,
        "nobody gave a passphrase, so the mark was issued"
    );
    assert_eq!(
        (first.relay.as_deref(), first.forward.as_ref()),
        (None, None)
    );

    // The name is signed into the token, trimmed as the server trims it.
    let said = claims(&first.token);
    assert_eq!(said["sub"], first.identity.as_str());
    assert_eq!(said["name"], "Native Test");
    assert_eq!(said["video"]["room"], room.as_str());

    // Sent back with the proof it came with, it is kept: a reconnect is the
    // same participant. A server older than proofs hands out none and keeps
    // the identity on its own.
    let again = d
        .clone()
        .join(
            room.clone(),
            s("Native Test"),
            s(""),
            Some(first.identity.clone()),
            first.proof.clone(),
            s(""),
            None,
        )
        .await
        .unwrap();
    assert_eq!(again.identity, first.identity);
    assert_eq!(again.proof, first.proof, "the same identity, the same proof");

    // Not sent, a new one is minted; and an invented one is replaced.
    let fresh = d
        .clone()
        .join(room.clone(), s("Native Test"), s(""), None, None, s(""), None)
        .await
        .unwrap();
    assert_ne!(fresh.identity, first.identity);

    // Where the server hands out proofs, an issued identity sent back without
    // its own is somebody who read it off the roster, and gets a fresh one:
    // with none, and with another guest's (internal/room/room.go
    // `GuestProof`). Where it does not, the identity is kept as before.
    for (proof, what) in [(None, "no proof"), (fresh.proof.clone(), "another guest's proof")] {
        let copied = d
            .clone()
            .join(
                room.clone(),
                s("Native Test"),
                s(""),
                Some(first.identity.clone()),
                proof,
                s(""),
                None,
            )
            .await
            .unwrap();
        assert_eq!(
            copied.identity == first.identity,
            first.proof.is_none(),
            "{what}: {} for {}",
            copied.identity,
            first.identity
        );
    }
    let invented = d
        .clone()
        .join(
            room.clone(),
            s("Native Test"),
            s(""),
            Some(s("alice")),
            None,
            s(""),
            None,
        )
        .await
        .unwrap();
    assert!(valid_identity(invented.identity.clone()) && invented.identity != "alice");

    // Upper case is lowered by the server, which says which room it granted.
    let lowered = d
        .clone()
        .join(
            room.to_uppercase(),
            s("Native Test"),
            s(""),
            None,
            None,
            s(""),
            None,
        )
        .await
        .unwrap();
    assert_eq!(lowered.room, room);

    assert_eq!(
        d.clone()
            .join(s("not a room"), s("A"), s(""), None, None, s(""), None)
            .await
            .unwrap_err(),
        refused(400, "invalid_room")
    );
    // A scoped room is for its members, and nobody here is one.
    assert_eq!(
        d.join(
            format!("{room}@nativetest"),
            s("A"),
            s(""),
            None,
            None,
            s(""),
            None
        )
        .await
        .unwrap_err(),
        refused(403, "not_in_scope")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_host_invites_answers_the_door_and_is_refused_what_is_not_theirs() {
    let Some(host) = live().await else { return };
    let room = generate_room_name();

    // The one passphrase join of the run. A proven mark opening a room nobody
    // has used is that room's host.
    let passphrase = format!("native-api-test {}", generate_room_name());
    let held = host
        .clone()
        .join(
            room.clone(),
            s("Native Host"),
            passphrase,
            None,
            None,
            s(""),
            None,
        )
        .await
        .unwrap();
    let bearer = held.token.clone();
    assert!(signature_of(held.identity.clone()).unwrap().proven);
    // The passphrase keeps this identity, and nothing else is needed for it.
    assert_eq!(held.proof, None);

    let standing = host
        .clone()
        .host_standing(room.clone(), bearer.clone())
        .await
        .unwrap();
    assert!(standing.yours && !standing.admin, "{standing:?}");
    assert_eq!(
        host.clone()
            .host_standing(room.clone(), s("not.a.token"))
            .await
            .unwrap_err(),
        refused(401, "not_yours")
    );

    // An invite, made with the join token, read back, and used.
    let made = host
        .clone()
        .create_invite(room.clone(), bearer.clone())
        .await
        .unwrap();
    assert_eq!(made.room, room);
    assert_eq!(
        made.link,
        host.invite_link(made.token.clone(), room.clone())
    );
    assert!(
        made.until.is_some(),
        "an ordinary invite lasts a day at most"
    );
    let read = host.clone().invite_info(made.token.clone()).await.unwrap();
    assert_eq!(
        (read.room.as_deref(), read.error.as_deref()),
        (Some(room.as_str()), None)
    );

    let guest = Deployment::new(host.base_url()).unwrap();
    let invited = guest
        .clone()
        .join(
            room.clone(),
            s("Native Guest"),
            s(""),
            None,
            None,
            s(""),
            Some(made.token.clone()),
        )
        .await
        .unwrap();
    assert!(
        guest
            .cookie_header()
            .unwrap_or_default()
            .contains(&format!("meet-live.invite={}", made.token)),
        "the deployment keeps the invite in a cookie for the next join"
    );

    // A guest's token is for this room and runs nothing in it.
    let theirs = guest
        .clone()
        .host_standing(room.clone(), invited.token.clone())
        .await
        .unwrap();
    assert!(!theirs.yours && !theirs.admin);
    assert_eq!(
        guest
            .clone()
            .create_invite(room.clone(), invited.token.clone())
            .await
            .unwrap_err(),
        refused(403, "not_yours")
    );

    // Somebody at the door, seen from inside, let in, and told so.
    let stranger = Deployment::new(host.base_url()).unwrap();
    let knock = stranger
        .clone()
        .knock(room.clone(), s("Native Knocker"))
        .await
        .unwrap();
    assert_eq!(knock.state, DoorState::Knocking);
    assert_eq!(
        stranger
            .clone()
            .at_the_door(room.clone(), knock.token.clone())
            .await
            .unwrap()
            .state,
        DoorState::Knocking
    );

    let waiting = host
        .clone()
        .knocking(room.clone(), bearer.clone())
        .await
        .unwrap();
    let at_door = waiting
        .iter()
        .find(|one| one.name == "Native Knocker")
        .expect("the knock is seen from inside");
    assert_eq!(
        host.clone()
            .answer_knock(room.clone(), at_door.id.clone(), bearer.clone(), true)
            .await
            .unwrap(),
        DoorState::Admitted
    );
    let answered = stranger
        .clone()
        .at_the_door(room.clone(), knock.token.clone())
        .await
        .unwrap();
    assert_eq!(answered.state, DoorState::Admitted);
    assert!(answered.invite.is_some());

    // A knock asked about under another room's name is the same as a refusal.
    let elsewhere = stranger
        .at_the_door(generate_room_name(), knock.token)
        .await
        .unwrap();
    assert_eq!(elsewhere.state, DoorState::Refused);

    // What the host may not do, refused with the server's own codes.
    assert_eq!(
        host.clone()
            .hand_over(room.clone(), bearer.clone(), invited.identity.clone())
            .await
            .unwrap_err(),
        refused(400, "not_proven")
    );
    assert_eq!(
        host.clone()
            .hand_over(room.clone(), bearer.clone(), s("nobody"))
            .await
            .unwrap_err(),
        refused(400, "no_such_person")
    );
    assert_eq!(
        host.clone()
            .turn_out(room.clone(), bearer.clone(), held.identity.clone())
            .await
            .unwrap_err(),
        refused(400, "no_such_person"),
        "a host cannot turn themselves out"
    );
    assert_eq!(
        host.clone()
            .move_room(room.clone(), bearer.clone(), s("no-such-relay"))
            .await
            .unwrap_err(),
        refused(400, "no_such_relay")
    );

    // Every link stops working at once.
    assert!(
        host.clone()
            .revoke_invites(room.clone(), Some(bearer))
            .await
            .unwrap()
            >= 1
    );
    let dead = host.invite_info(made.token).await.unwrap();
    assert!(dead.error.is_some(), "{dead:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_an_account_the_account_doors_are_shut() {
    let Some(d) = live().await else { return };

    assert_eq!(d.clone().me().await.unwrap(), None);
    // The one refused sign-in of the run.
    assert_eq!(
        d.clone()
            .sign_in(s("native-api-test-nobody"), s("not a passphrase"))
            .await
            .unwrap_err(),
        refused(401, "not_yours")
    );
    assert_eq!(d.account_cookie(), None);

    assert_eq!(
        d.clone()
            .change_passphrase(s("a"), s("bbbbbbbbbb"))
            .await
            .unwrap_err(),
        refused(401, "not_yours")
    );
    assert_eq!(
        d.clone()
            .set_avatar(s("data:image/png;base64,AAAA"))
            .await
            .unwrap_err(),
        refused(401, "not_yours")
    );
    assert_eq!(
        d.clone().remove_avatar().await.unwrap_err(),
        refused(401, "not_yours")
    );
    assert_eq!(
        d.clone().room_live(generate_room_name()).await.unwrap(),
        None
    );

    assert_eq!(
        d.clone().arrangements().await.unwrap_err(),
        refused(401, "not_signed_in")
    );
    assert_eq!(
        d.clone()
            .arrange_meeting(generate_room_name(), 4_102_444_800_000, s(""))
            .await
            .unwrap_err(),
        refused(401, "not_signed_in")
    );
    assert_eq!(
        d.clone().cancel_meeting(s("nothing")).await.unwrap_err(),
        refused(401, "not_signed_in")
    );
    assert_eq!(
        d.clone().arranged(s("no-such-meeting")).await.unwrap_err(),
        refused(404, "no_such_meeting")
    );

    let unknown = d
        .clone()
        .invite_info(s("no-such-invite-native"))
        .await
        .unwrap();
    assert_eq!(unknown.error.as_deref(), Some("no_such_invite"));

    // Signing out with nothing to sign out of is still a clean answer.
    d.sign_out().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn watching_and_music_are_absent_or_for_somebody_signed_in() {
    let Some(d) = live().await else { return };
    let absent = |said: ProtocolError| {
        assert!(
            said == refused(404, "not_found") || said == refused(401, "not_signed_in"),
            "{said:?}"
        );
    };

    assert!(!d.clone().watch_ready().await.unwrap());
    absent(
        d.clone()
            .resolve_video(s("https://example.invalid/v.mp4"), false)
            .await
            .unwrap_err(),
    );
    absent(d.clone().media_servers().await.unwrap_err());
    absent(d.clone().music_sources().await.unwrap_err());
    absent(
        d.clone()
            .music_search(s("a"), s("sunny"), 1)
            .await
            .unwrap_err(),
    );
    absent(
        d.clone()
            .music_track(s("a"), s("1"), MusicQuality::Best)
            .await
            .unwrap_err(),
    );
    absent(
        d.clone()
            .music_link(s("https://music.example.invalid/s/1"))
            .await
            .unwrap_err(),
    );

    // A ticket is for anybody in a call, so it is asked about without a
    // session; with no gateway there is nothing to ask.
    match d.video_info(s("Abc_def-1234567890xyz")).await {
        Err(said) => absent(said),
        Ok(found) => panic!("a deployment with no gateway described a ticket: {found:?}"),
    }
}
