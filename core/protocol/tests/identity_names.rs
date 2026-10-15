//! Names, signatures and room names: web/src/live/names.test.ts case for case,
//! and the server's own cases for the rules only it holds
//! (internal/room/room_test.go).

use std::collections::HashSet;

use tomoshibi_protocol::identity::{
    display_name, generate_room_name, impersonating, looks_generated, normalise_room_name,
    parse_name, passphrase_of, scope_of, signature_of, valid_identity, valid_room_name,
    valid_scope, IdentitySignature, NamedIdentity, MAX_DISPLAY_NAME, MAX_ROOM_NAME, MAX_SCOPE,
};

fn s(value: &str) -> String {
    value.to_owned()
}

// describe("generateRoomName")

#[test]
fn a_generated_name_is_one_the_server_will_accept() {
    for _ in 0..200 {
        let name = generate_room_name();
        assert!(valid_room_name(name.clone()), "{name}");
        assert!(looks_generated(name.clone()), "{name}");
    }
}

// The name is the credential, so repetition would be somebody walking into a
// stranger's meeting rather than a cosmetic flaw.
#[test]
fn a_generated_name_does_not_repeat_itself_over_a_large_sample() {
    let seen: HashSet<String> = (0..500).map(|_| generate_room_name()).collect();
    assert_eq!(seen.len(), 500);
}

// describe("normaliseRoomName")

#[test]
fn normalising_reduces_what_somebody_typed_to_something_acceptable() {
    assert_eq!(normalise_room_name(s("Weekly Standup")), "weekly-standup");
    assert_eq!(normalise_room_name(s("  Team!! Sync  ")), "team-sync");
    assert_eq!(normalise_room_name(s("--edges--")), "edges");
    assert_eq!(normalise_room_name(s("已经开始")), "");
}

#[test]
fn normalising_keeps_the_result_within_the_length_the_server_allows() {
    assert!(normalise_room_name("a".repeat(200)).len() <= 64);
    // Cut first and trimmed after, as names.ts does, so a dash the cut lands
    // beside does not survive at the end.
    assert_eq!(
        normalise_room_name(format!("{}-b", "a".repeat(63))),
        "a".repeat(63)
    );
}

#[test]
fn normalising_leaves_an_acceptable_name_alone() {
    assert_eq!(normalise_room_name(s("weekly-standup")), "weekly-standup");
}

// describe("scoped names")

#[test]
fn a_room_and_a_scope_are_accepted() {
    for name in ["standup@acme", "a@b", "weekly-standup@acme-cn"] {
        assert!(valid_room_name(s(name)), "{name}");
    }

    assert!(valid_room_name(format!(
        "{}@{}",
        "a".repeat(MAX_ROOM_NAME),
        "b".repeat(MAX_SCOPE)
    )));
}

#[test]
fn anything_that_splits_two_ways_or_has_an_empty_half_is_refused() {
    for name in ["@acme", "standup@", "a@b@c", "standup@-acme"] {
        assert!(!valid_room_name(s(name)), "{name}");
    }
    assert!(!valid_room_name(format!(
        "standup@{}",
        "b".repeat(MAX_SCOPE + 1)
    )));

    assert!(!valid_scope(s("")));
    assert!(!valid_scope(s("a@b")));
}

#[test]
fn each_half_is_normalised_as_a_plain_name_would_be() {
    assert_eq!(
        normalise_room_name(s("Weekly Standup@ACME Corp")),
        "weekly-standup@acme-corp"
    );
    assert_eq!(normalise_room_name(s("a@b@c")), "a@b-c");
}

// Normalised as somebody types, so the separator has to survive the moment it
// is pressed or nobody could type a scope at all.
#[test]
fn the_at_sign_is_kept_while_the_scope_is_still_being_typed() {
    assert_eq!(normalise_room_name(s("standup@")), "standup@");
    assert!(!valid_room_name(normalise_room_name(s("standup@"))));
}

#[test]
fn the_scope_is_read_out_of_a_name() {
    assert_eq!(scope_of(s("standup@acme")), "acme");
    assert_eq!(scope_of(s("standup")), "");
}

// room_test.go TestValidName and TestScopedNames: the server's own cases, which
// the client's rule has to agree with or a page loads a room it cannot join.
#[test]
fn agrees_with_the_server_about_names() {
    for name in ["demo", "weekly-standup", "room-2", "a"] {
        assert!(valid_room_name(s(name)), "{name}");
    }
    for name in ["", "Demo", "with space", "-lead", "trail-", "under_score"] {
        assert!(!valid_room_name(s(name)), "{name}");
    }
    assert!(!valid_room_name("a".repeat(MAX_ROOM_NAME + 1)));

    for name in [
        "standup@acme@else",
        "standup@Acme",
        "standup@acme-",
        "stand up@acme",
        "standup@ac_me",
    ] {
        assert!(!valid_room_name(s(name)), "{name}");
    }
    assert!(!valid_room_name(format!(
        "{}@acme",
        "a".repeat(MAX_ROOM_NAME + 1)
    )));
    assert!(!valid_scope(s("-acme")));
    assert!(!valid_scope("b".repeat(MAX_SCOPE + 1)));
}

// describe("looksGenerated"): decides whether to warn that a room can be
// guessed, so a chosen name must never be mistaken for one from the generator.
#[test]
fn a_chosen_name_does_not_look_generated() {
    for name in [
        "standup",
        "team-sync",
        "amber-otter-glide",
        "amber-otter-glide-42",
    ] {
        assert!(!looks_generated(s(name)), "{name}");
    }
    // And one a browser generated is recognised here: the lists are the web's.
    assert!(looks_generated(s("amber-abbey-abide-0042")));
    assert!(looks_generated(s("tidy-toucan-whirl-9999")));
}

// describe("parseName")

#[test]
fn a_name_is_split_from_its_passphrase() {
    let split = |typed: &str| {
        let parts = parse_name(s(typed));
        (parts.name, parts.passphrase)
    };

    assert_eq!(split("Alice#hunter2"), (s("Alice"), s("hunter2")));
    assert_eq!(split("Alice"), (s("Alice"), s("")));
    assert_eq!(split("  Alice  "), (s("Alice"), s("")));
}

// A harder passphrase is a better one, so nothing in it is rejected.
#[test]
fn everything_after_the_first_separator_is_kept() {
    assert_eq!(parse_name(s("Alice#a#b#c")).passphrase, "a#b#c");
}

#[test]
fn a_passphrase_with_no_name_is_allowed() {
    let parts = parse_name(s("#secret"));
    assert_eq!(
        (parts.name.as_str(), parts.passphrase.as_str()),
        ("", "secret")
    );
}

#[test]
fn the_split_never_prints_the_passphrase() {
    let said = format!("{:?}", parse_name(s("Alice#hunter2")));
    assert!(
        said.contains("Alice") && !said.contains("hunter2"),
        "{said}"
    );
}

// describe("signatureOf"). A signature is exactly ten base32 characters, and an
// identity carries thirty-two hex characters of randomness after it.
const TRIP: &str = "k7m2q6x4rt";
const HEX: &str = "0123456789abcdef0123456789abcdef";

fn mark(trip: &str, proven: bool, account: bool) -> Option<IdentitySignature> {
    Some(IdentitySignature {
        trip: s(trip),
        proven,
        account,
    })
}

#[test]
fn a_mark_that_was_earned_is_read() {
    assert_eq!(
        signature_of(format!("t{TRIP}-{HEX}")),
        mark(TRIP, true, false)
    );
}

// The third kind: the same mark and the same person as a passphrase gives, and
// the one thing that turns on how they arrived is whether the account's
// picture is shown.
#[test]
fn a_mark_from_an_account_is_read_and_still_called_proven() {
    assert_eq!(
        signature_of(format!("a{TRIP}-{HEX}")),
        mark(TRIP, true, true)
    );
}

#[test]
fn an_issued_mark_is_read_and_not_called_proven() {
    assert_eq!(
        signature_of(format!("g{TRIP}-{HEX}")),
        mark(TRIP, false, false)
    );
}

#[test]
fn anything_not_shaped_like_a_mark_is_refused() {
    assert_eq!(signature_of(s("t-short")), None);
    assert_eq!(signature_of(format!("tABCDEFGHIJ-{HEX}")), None);
    assert_eq!(signature_of(format!("t{TRIP}x{HEX}")), None);
    assert_eq!(signature_of(format!("x{TRIP}-{HEX}")), None);
    assert_eq!(signature_of(s("")), None);
    // A multi-byte character where the mark should be is not one, and must not
    // be cut through.
    assert_eq!(signature_of(s("tééééé-abc")), None);
}

// describe("impersonating")

fn person(name: &str, identity: String) -> NamedIdentity {
    NamedIdentity {
        name: s(name),
        identity,
    }
}

#[test]
fn an_unsigned_participant_wearing_a_signed_name_is_marked() {
    let signed = person("Alice", format!("t{TRIP}-{HEX}"));
    let unsigned = person("Alice", format!("gaaaaaaaaaa-{HEX}"));
    let other = person("Bob", format!("gbbbbbbbbbb-{HEX}"));

    assert!(impersonating(
        unsigned.clone(),
        vec![signed.clone(), unsigned.clone(), other.clone()]
    ));
    assert!(!impersonating(
        signed.clone(),
        vec![signed, unsigned, other]
    ));
}

// Two people genuinely called Alex is ordinary and worth no comment.
#[test]
fn a_collision_between_two_unsigned_names_says_nothing() {
    let unsigned = person("Alice", format!("gaaaaaaaaaa-{HEX}"));
    let twin = person("Alice", format!("gcccccccccc-{HEX}"));
    let other = person("Bob", format!("gbbbbbbbbbb-{HEX}"));

    assert!(!impersonating(
        unsigned.clone(),
        vec![unsigned.clone(), twin]
    ));
    assert!(!impersonating(unsigned.clone(), vec![unsigned, other]));
}

// describe("passphraseOf"): one shape, two fields. The pre-join reads
// `Alice#secret`, and a field that takes a passphrase alone has to reach the
// same passphrase from the same keystrokes.
#[test]
fn passphrase_of_keeps_the_half_parse_name_throws_away() {
    assert_eq!(
        passphrase_of(s("Alice#secret")),
        parse_name(s("Alice#secret")).passphrase
    );
}

#[test]
fn passphrase_of_takes_a_bare_passphrase_whole() {
    assert_eq!(passphrase_of(s("djoff-bv2j6")), "djoff-bv2j6");
    assert_eq!(parse_name(s("djoff-bv2j6")).passphrase, "");
}

#[test]
fn passphrase_of_cuts_at_the_first_separator_only() {
    assert_eq!(passphrase_of(s("Alice#one#two")), "one#two");
}

#[test]
fn passphrase_of_agrees_with_the_pre_join_on_everything_it_is_given() {
    for typed in ["Alice#secret", "  Bo #x y z", "Cy#a#b", "#leading", "Dee#"] {
        assert_eq!(
            passphrase_of(s(typed)),
            parse_name(s(typed)).passphrase,
            "{typed}"
        );
    }
}

// Trimming belongs to the server, which does it for both fields.
#[test]
fn passphrase_of_leaves_the_passphrase_exactly_as_it_was_typed() {
    assert_eq!(passphrase_of(s("Alice# padded ")), " padded ");
}

// room_test.go TestIdentityWeDidNotMintIsRejected, and the shape it keeps.
#[test]
fn only_an_identity_the_server_could_have_minted_is_valid() {
    assert!(valid_identity(format!("g{TRIP}-{HEX}")));
    assert!(valid_identity(format!("a{TRIP}-{HEX}")));
    for identity in [
        s(""),
        s("alice"),
        s("g-short"),
        format!("g{}-{}", "z".repeat(10), "Z".repeat(32)),
        format!("x{}-{}", "z".repeat(10), "a".repeat(32)),
        format!("g{TRIP}-{HEX}0"),
        format!("g{TRIP}-{}", &HEX[1..]),
    ] {
        assert!(!valid_identity(identity.clone()), "{identity}");
    }
}

// room_test.go TestDisplayNameIsSignedIn, TestAnEmptyNameFallsBackToTheIdentity
// and TestALongNameIsTrimmedRatherThanRefused.
#[test]
fn a_display_name_is_what_the_server_will_call_somebody() {
    let identity = format!("g{TRIP}-{HEX}");

    assert_eq!(display_name(s("  Alice  "), identity.clone()), "Alice");
    assert_eq!(display_name(s("   "), identity.clone()), identity);

    let long = display_name("a".repeat(MAX_DISPLAY_NAME + 20), identity.clone());
    assert_eq!(long.chars().count(), MAX_DISPLAY_NAME);

    // Characters, not bytes, and trimmed again after the cut.
    let wide = display_name("已".repeat(MAX_DISPLAY_NAME + 1), identity.clone());
    assert_eq!(wide.chars().count(), MAX_DISPLAY_NAME);
    let spaced = display_name(format!("{} b", "a".repeat(MAX_DISPLAY_NAME - 1)), identity);
    assert_eq!(spaced, "a".repeat(MAX_DISPLAY_NAME - 1));
}
