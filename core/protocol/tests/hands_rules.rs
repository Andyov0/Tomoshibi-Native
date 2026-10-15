//! Hands, reactions and watching (web/src/live/hands.test.ts, and the rules of
//! hands.ts and watching.ts the web exercises through React).
//!
//! The two halves are different kinds of thing and the difference is the
//! design. A raised hand is state: an attribute, true until lowered, visible to
//! somebody who joins later, gone with the participant. A reaction is an event:
//! a data message, seen and over. Sending either the other way round is a real
//! fault: a hand as an event is invisible to late arrivals, and a reaction as
//! state is a thumbs-up stuck to a tile for the rest of the call. And every
//! value here is read by browsers in the same room, so the strings are the
//! protocol and are pinned to the byte.

use std::collections::HashMap;

use tomoshibi_protocol::hands::{self, RosterMember, StagePicture};

fn member(identity: &str, attributes: &[(&str, &str)]) -> RosterMember {
    RosterMember {
        identity: identity.into(),
        attributes: attributes.iter().map(|(key, value)| ((*key).to_owned(), (*value).to_owned())).collect(),
    }
}

#[test]
fn keeps_a_hand_as_an_attribute_and_a_reaction_as_a_message() {
    // The attribute and the topic, exactly as a browser writes and reads them.
    assert_eq!(hands::HAND, "hand");
    assert_eq!(hands::HAND_UP, "up");
    assert_eq!(hands::REACTION_TOPIC, "reaction");
    const { assert!(hands::REACTION_RELIABLE) };

    // A hand is written as the attribute's value, and a reaction as a
    // payload; neither is the other.
    assert_eq!(hands::hand_to_write(None, true).as_deref(), Some("up"));
    assert_eq!(hands::reaction_payload("\u{1F44D}".into()).unwrap(), "\u{1F44D}".as_bytes());
}

#[test]
fn offers_a_short_list_of_reactions_rather_than_any_text_at_all() {
    let choices = hands::reaction_choices();

    assert!(!choices.is_empty());
    assert!(choices.len() <= 8);

    // Each is one glyph a tile badge can hold.
    for one in &choices {
        assert!(one.chars().count() <= 3, "{one:?}");
    }
}

// The list is the protocol: a browser drops anything not on it, so a native
// client offering a different heart sends reactions nobody else sees.
#[test]
fn offers_the_web_clients_reactions_to_the_code_point() {
    let points: Vec<Vec<u32>> =
        hands::reaction_choices().iter().map(|one| one.chars().map(u32::from).collect()).collect();

    assert_eq!(
        points,
        vec![
            vec![0x1F44D],
            vec![0x1F44F],
            vec![0x1F602],
            vec![0x1F389],
            vec![0x2764, 0xFE0F],
            vec![0x1F62E],
        ]
    );
}

#[test]
fn shows_a_reaction_for_long_enough_to_be_seen_and_not_long_enough_to_linger() {
    let shown = hands::reaction_shown_for_ms();

    assert!((2000..=8000).contains(&shown));
    assert_eq!(shown, 4000);
}

#[test]
fn shows_only_reactions_on_the_list_from_a_participant_on_the_reaction_topic() {
    let topic = || Some("reaction".to_owned());
    let from = || Some("alice".to_owned());
    let heart = "\u{2764}\u{FE0F}";

    assert_eq!(hands::received_reaction(topic(), from(), heart.into()).as_deref(), Some(heart));

    // Somewhere else, or from nobody (the server itself).
    assert_eq!(hands::received_reaction(Some("chat".into()), from(), heart.into()), None);
    assert_eq!(hands::received_reaction(None, from(), heart.into()), None);
    assert_eq!(hands::received_reaction(topic(), None, heart.into()), None);

    // Not on the list: a heart without its variation selector is a different
    // string, and arbitrary text is not something to put on a tile.
    assert_eq!(hands::received_reaction(topic(), from(), "\u{2764}".into()), None);
    assert_eq!(hands::received_reaction(topic(), from(), format!("{heart} ").into_bytes()), None);
    assert_eq!(hands::received_reaction(topic(), from(), b"<b>hi</b>".to_vec()), None);
    assert_eq!(hands::received_reaction(topic(), from(), vec![0xF0, 0x9F, 0x91]), None);

    // A browser's TextDecoder drops a leading byte order mark, so a browser
    // shows this one, and so does this.
    let mut marked = vec![0xEF, 0xBB, 0xBF];
    marked.extend_from_slice("\u{1F389}".as_bytes());
    assert_eq!(hands::received_reaction(topic(), from(), marked).as_deref(), Some("\u{1F389}"));
}

#[test]
fn sends_only_reactions_on_the_list() {
    for one in hands::reaction_choices() {
        assert_eq!(hands::reaction_payload(one.clone()).unwrap(), one.into_bytes());
    }

    assert!(hands::reaction_payload("\u{2764}".into()).is_err());
    assert!(hands::reaction_payload("hello".into()).is_err());
}

// An attribute is broadcast to the whole room, so writing the value it already
// has is a message to everybody that says nothing.
#[test]
fn writes_a_hand_only_when_it_changes() {
    assert_eq!(hands::hand_to_write(None, true).as_deref(), Some("up"));
    assert_eq!(hands::hand_to_write(Some("".into()), true).as_deref(), Some("up"));
    assert_eq!(hands::hand_to_write(Some("up".into()), false).as_deref(), Some(""));

    assert_eq!(hands::hand_to_write(Some("up".into()), true), None);
    assert_eq!(hands::hand_to_write(None, false), None);
    assert_eq!(hands::hand_to_write(Some("".into()), false), None);

    // The web asks "is it up", and only "up" is up.
    assert_eq!(hands::hand_to_write(Some("UP".into()), false), None);
    assert_eq!(hands::hand_to_write(Some("UP".into()), true).as_deref(), Some("up"));
}

#[test]
fn reads_raised_hands_off_the_roster_in_its_order() {
    let roster = vec![
        member("me", &[("hand", "up")]),
        member("bob", &[]),
        member("carol", &[("hand", "up"), ("watching", "me")]),
        member("dave", &[("hand", "")]),
        member("erin", &[("hand", "Up")]),
    ];

    assert_eq!(hands::raised_hands(roster), ["me", "carol"]);

    assert!(hands::hand_is_up(HashMap::from([("hand".to_owned(), "up".to_owned())])));
    assert!(!hands::hand_is_up(HashMap::from([("hand".to_owned(), " up".to_owned())])));
    assert!(!hands::hand_is_up(HashMap::new()));
}

// Only a share actually on the stage counts. One sitting in the strip as a card
// is a thing nobody opened, and counting it would tell the sharer twelve people
// are watching a picture nobody has looked at.
#[test]
fn watches_only_somebody_elses_share_on_the_stage() {
    let stage = |owner: &str, screen: bool, local: bool| Some(StagePicture { owner: owner.into(), screen, local });

    assert_eq!(hands::watched_sharer(stage("alice", true, false)), "alice");

    assert_eq!(hands::watched_sharer(stage("alice", false, false)), "");
    assert_eq!(hands::watched_sharer(stage("me", true, true)), "");
    assert_eq!(hands::watched_sharer(None), "");
}

#[test]
fn writes_watching_only_when_it_changes() {
    assert_eq!(hands::watching_to_write(None, "alice".into()).as_deref(), Some("alice"));
    assert_eq!(hands::watching_to_write(Some("alice".into()), "bob".into()).as_deref(), Some("bob"));
    assert_eq!(hands::watching_to_write(Some("alice".into()), "".into()).as_deref(), Some(""));

    assert_eq!(hands::watching_to_write(Some("alice".into()), "alice".into()), None);
    // Absent and empty are the same nothing.
    assert_eq!(hands::watching_to_write(None, "".into()), None);
    assert_eq!(hands::watching_to_write(Some("".into()), "".into()), None);
}

#[test]
fn a_sharer_is_watched_by_whoever_says_so_and_only_while_sharing() {
    let roster = vec![
        member("me", &[("watching", "me")]),
        member("bob", &[("watching", "me")]),
        member("carol", &[("watching", "dave")]),
        member("dave", &[]),
        member("erin", &[("watching", "me"), ("hand", "up")]),
        member("frank", &[("watching", "ME")]),
    ];

    assert_eq!(hands::watchers("me".into(), true, roster.clone()), ["bob", "erin"]);
    assert!(hands::watchers("me".into(), false, roster).is_empty());
}
