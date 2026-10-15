//! Names: what people call themselves, the signature that makes a name hard to
//! wear, and the names of rooms.
//!
//! Follows web/src/live/name.ts (splitting `Name#passphrase`, reading the mark
//! an identity carries, telling an impostor from a twin) and names.ts (room
//! names: generating, normalising as somebody types, validating, and spotting
//! a generated one). The server's own rules, which these are kept in step with
//! and which still decide, are in internal/room/room.go and trip.go; where a
//! rule exists only on the server (what a display name becomes, what identity
//! it mints) it is ported from there and says so.

use rand::RngCore;

use crate::api::text::js_trim;

/// How long a signature is, matching what the server derives.
pub const TRIP_LENGTH: usize = 10;

/// How many hex characters of randomness follow the signature in an identity.
const RANDOM: usize = 32;

/// The longest display name the server keeps, in characters; it trims rather
/// than refuses (room.go `MaxDisplayName`).
pub const MAX_DISPLAY_NAME: usize = 40;

/// The longest room name the server accepts, before any scope.
pub const MAX_ROOM_NAME: usize = 64;

/// The longest scope the server accepts after the `@`.
pub const MAX_SCOPE: usize = 32;

/// The longest a whole name can be: a room, its `@`, and its scope.
pub const MAX_SCOPED_NAME: usize = MAX_ROOM_NAME + 1 + MAX_SCOPE;

/// How many digits follow a generated name's words.
const DIGITS: usize = 4;

/// The limits above, for the apps' text fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct NameLimits {
    pub trip_length: u32,
    pub max_display_name: u32,
    pub max_room_name: u32,
    pub max_scope: u32,
    pub max_scoped_name: u32,
}

#[uniffi::export]
pub fn name_limits() -> NameLimits {
    NameLimits {
        trip_length: TRIP_LENGTH as u32,
        max_display_name: MAX_DISPLAY_NAME as u32,
        max_room_name: MAX_ROOM_NAME as u32,
        max_scope: MAX_SCOPE as u32,
        max_scoped_name: MAX_SCOPED_NAME as u32,
    }
}

/// A name as typed, split into what is shown and what proves it.
#[derive(Clone, PartialEq, Eq, uniffi::Record)]
pub struct NameParts {
    /// Shown to everybody.
    pub name: String,
    /// Never shown, and kept nowhere but the Keychain.
    pub passphrase: String,
}

// Written by hand so that logging the split cannot print the secret, which is
// what the server's `room.Passphrase` does for its side.
impl std::fmt::Debug for NameParts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let passphrase = if self.passphrase.is_empty() {
            ""
        } else {
            "[redacted]"
        };
        f.debug_struct("NameParts")
            .field("name", &self.name)
            .field("passphrase", &passphrase)
            .finish()
    }
}

/// Splits `Alice#secret` into a name and a passphrase (name.ts `parseName`).
///
/// Only the first `#` counts, so a passphrase may contain them: the harder a
/// passphrase is, the better, and refusing a character would be advice in the
/// wrong direction. The name is trimmed as JavaScript trims; the passphrase is
/// left exactly as typed, because trimming it is the server's to do.
#[uniffi::export]
pub fn parse_name(typed: String) -> NameParts {
    match typed.split_once('#') {
        None => NameParts {
            name: js_trim(&typed).to_owned(),
            passphrase: String::new(),
        },
        Some((name, passphrase)) => NameParts {
            name: js_trim(name).to_owned(),
            passphrase: passphrase.to_owned(),
        },
    }
}

/// The passphrase out of whatever somebody typed into a field that takes a
/// passphrase alone (name.ts `passphraseOf`).
///
/// Not `parse_name`: with no `#` that calls the whole string a name, which is
/// right where a name is wanted and exactly wrong here. `Alice#secret` and
/// `secret` both give `secret`, so the form everybody learnt at the join works
/// in the other field too.
#[uniffi::export]
pub fn passphrase_of(typed: String) -> String {
    match typed.split_once('#') {
        None => typed,
        Some((_, passphrase)) => passphrase.to_owned(),
    }
}

/// The mark an identity carries, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct IdentitySignature {
    /// Ten base32 characters, the same for one passphrase on every visit.
    pub trip: String,
    /// Derived from a passphrase rather than drawn from nothing: "this is the
    /// same person as last time", as against only "these two are not the same
    /// person". Only a proven mark may be drawn as a claim about anybody.
    pub proven: bool,
    /// They were signed in to an account when they joined. Implies proven, and
    /// is what decides whether the account's picture is theirs to wear.
    pub account: bool,
}

/// The mark an identity carries, read from the identity itself, which is the
/// one thing about a participant nobody can change after the token was signed
/// (name.ts `signatureOf`). `t` is proven, `a` is proven and signed in, `g` is
/// issued; anything else is not an identity this deployment minted.
#[uniffi::export]
pub fn signature_of(identity: String) -> Option<IdentitySignature> {
    let bytes = identity.as_bytes();
    let kind = *bytes.first()?;
    if !matches!(kind, b't' | b'g' | b'a') {
        return None;
    }

    let trip = bytes.get(1..1 + TRIP_LENGTH)?;
    if bytes.get(1 + TRIP_LENGTH) != Some(&b'-') {
        return None;
    }
    if !trip.iter().all(|b| matches!(b, b'a'..=b'z' | b'2'..=b'7')) {
        return None;
    }

    Some(IdentitySignature {
        trip: String::from_utf8_lossy(trip).into_owned(),
        proven: kind == b't' || kind == b'a',
        account: kind == b'a',
    })
}

/// Whether `identity` is one the server could have minted, and so one it will
/// keep when it is sent back (room.go `ValidIdentity`): a mark and thirty-two
/// lower-case hex characters. One that is not is replaced at the next join.
#[uniffi::export]
pub fn valid_identity(identity: String) -> bool {
    signature_of(identity.clone()).is_some()
        && identity.len() == 1 + TRIP_LENGTH + 1 + RANDOM
        && identity.as_bytes()[1 + TRIP_LENGTH + 1..]
            .iter()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A participant as the impostor check sees them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NamedIdentity {
    pub name: String,
    pub identity: String,
}

/// Whether somebody is wearing a name another participant has proven
/// (name.ts `impersonating`).
///
/// Only ever said of the one who cannot prove it, and only when the collision
/// is with somebody who can. Two people genuinely called Alex is ordinary;
/// somebody unproven under a proven name is the one shape impersonation takes.
#[uniffi::export]
pub fn impersonating(participant: NamedIdentity, everybody: Vec<NamedIdentity>) -> bool {
    if signature_of(participant.identity.clone()).is_some_and(|mark| mark.proven) {
        return false;
    }
    if participant.name.is_empty() {
        return false;
    }

    everybody.iter().any(|other| {
        other.identity != participant.identity
            && other.name == participant.name
            && signature_of(other.identity.clone()).is_some_and(|mark| mark.proven)
    })
}

/// What the server will call somebody who asked to be called `wanted`
/// (room.go `display`): trimmed, cut to forty characters and trimmed again,
/// and the identity itself where nothing is left, so no tile is ever blank.
///
/// Trimmed the way Go trims, which is not quite the way JavaScript does (it
/// takes U+0085 and leaves U+FEFF), because this is the server's rule.
#[uniffi::export]
pub fn display_name(wanted: String, identity: String) -> String {
    let trimmed = wanted.trim();
    let cut = match trimmed.char_indices().nth(MAX_DISPLAY_NAME) {
        Some((end, _)) => trimmed[..end].trim(),
        None => trimmed,
    };

    if cut.is_empty() {
        identity
    } else {
        cut.to_owned()
    }
}

// Each list is 256 words, so one random byte picks a word without bias: a
// modulus over a list that is not a power of two makes its first entries
// fractionally likelier, which is entropy given away for nothing. Copied from
// names.ts in its order, because `looks_generated` has to recognise a name a
// browser generated, and a browser has to recognise one generated here.
const ADJECTIVES: [&str; 256] = [
    "amber", "ancient", "arctic", "autumn", "azure", "balmy", "blazing", "bold", "brave", "breezy",
    "bright", "brisk", "bronze", "calm", "candid", "cheerful", "chilly", "civic", "clear",
    "clever", "cobalt", "cosmic", "cozy", "crimson", "crisp", "curious", "daring", "dawn", "deep",
    "deft", "dizzy", "dusky", "eager", "early", "earnest", "easy", "elder", "electric", "elegant",
    "ember", "empty", "endless", "fair", "faithful", "famous", "fancy", "fearless", "feisty",
    "fertile", "fiery", "final", "fine", "firm", "first", "flat", "fleet", "floral", "fluent",
    "fond", "formal", "forward", "fragrant", "free", "fresh", "friendly", "frosty", "frugal",
    "full", "gallant", "gentle", "giant", "gifted", "glad", "glassy", "gleaming", "glossy",
    "golden", "gracious", "grand", "grassy", "gray", "great", "green", "happy", "hardy", "harsh",
    "hasty", "hazel", "healthy", "hearty", "heavy", "helpful", "hidden", "high", "hollow",
    "honest", "hopeful", "humble", "hungry", "ideal", "idle", "indigo", "inner", "ivory", "jade",
    "jolly", "joyful", "keen", "kind", "lanky", "large", "late", "lavish", "lean", "level",
    "light", "lilac", "limber", "lively", "lofty", "logical", "lone", "loud", "lower", "loyal",
    "lucid", "lucky", "lunar", "lush", "magenta", "major", "mellow", "merry", "mighty", "mild",
    "milky", "mindful", "minor", "misty", "modern", "modest", "moral", "mossy", "mystic", "narrow",
    "native", "neat", "neutral", "nimble", "noble", "noisy", "normal", "northern", "novel",
    "oaken", "olive", "open", "orange", "ornate", "outer", "patient", "peaceful", "pearly",
    "perfect", "placid", "plain", "playful", "pleasant", "plum", "polar", "polite", "prime",
    "prompt", "proper", "proud", "pure", "purple", "quaint", "quick", "quiet", "radiant", "rapid",
    "rare", "ready", "regal", "restful", "rich", "right", "ripe", "rising", "robust", "rosy",
    "round", "royal", "ruby", "rugged", "rural", "rustic", "sable", "safe", "sandy", "scarlet",
    "secret", "serene", "sharp", "sheer", "shiny", "short", "silent", "silken", "silver", "simple",
    "sincere", "skilled", "slender", "slight", "small", "smooth", "snowy", "soft", "solar",
    "solemn", "solid", "sombre", "sonic", "sound", "southern", "spare", "spirited", "splendid",
    "spry", "stable", "steady", "stellar", "stern", "still", "stormy", "stout", "straight",
    "strong", "sturdy", "subtle", "sudden", "summer", "sunny", "supple", "sure", "swift", "tall",
    "tame", "tender", "thankful", "thorough", "thrifty", "tidal", "tidy",
];

const NOUNS: [&str; 256] = [
    "abbey",
    "acorn",
    "alcove",
    "anchor",
    "anthem",
    "arbour",
    "arch",
    "archer",
    "arrow",
    "aspen",
    "atlas",
    "aurora",
    "badger",
    "banner",
    "basalt",
    "basin",
    "bay",
    "beacon",
    "beam",
    "bear",
    "beaver",
    "beech",
    "bell",
    "birch",
    "bison",
    "blossom",
    "bluff",
    "bough",
    "boulder",
    "branch",
    "breaker",
    "breeze",
    "bridge",
    "brook",
    "buffalo",
    "bugle",
    "burrow",
    "cabin",
    "canal",
    "canopy",
    "canyon",
    "cape",
    "cardinal",
    "cascade",
    "castle",
    "cavern",
    "cedar",
    "channel",
    "chapel",
    "chart",
    "chasm",
    "cliff",
    "cloud",
    "clover",
    "comet",
    "compass",
    "copper",
    "coral",
    "cottage",
    "cove",
    "crane",
    "crater",
    "creek",
    "crest",
    "crow",
    "crown",
    "crystal",
    "current",
    "cypress",
    "dahlia",
    "dawn",
    "dell",
    "delta",
    "dew",
    "dolphin",
    "dove",
    "drift",
    "dune",
    "dusk",
    "eagle",
    "echo",
    "elm",
    "ember",
    "estuary",
    "falcon",
    "fawn",
    "fell",
    "fern",
    "field",
    "finch",
    "fissure",
    "fjord",
    "flame",
    "flint",
    "forest",
    "fountain",
    "fox",
    "gale",
    "garden",
    "gate",
    "geyser",
    "glacier",
    "glade",
    "glen",
    "gorge",
    "granite",
    "grove",
    "gull",
    "hamlet",
    "harbour",
    "harvest",
    "haven",
    "hawk",
    "hazel",
    "heath",
    "heather",
    "hedge",
    "heron",
    "hickory",
    "hill",
    "hollow",
    "horizon",
    "hummock",
    "ibis",
    "inlet",
    "iris",
    "island",
    "ivy",
    "jasmine",
    "jetty",
    "juniper",
    "kestrel",
    "key",
    "kite",
    "knoll",
    "lagoon",
    "lake",
    "lantern",
    "larch",
    "lark",
    "laurel",
    "ledge",
    "lemur",
    "lighthouse",
    "lily",
    "linden",
    "lion",
    "lotus",
    "lynx",
    "magnolia",
    "maple",
    "marsh",
    "meadow",
    "meander",
    "mesa",
    "meteor",
    "mill",
    "mist",
    "moor",
    "moss",
    "moth",
    "mountain",
    "mulberry",
    "narrows",
    "nebula",
    "nest",
    "oak",
    "oasis",
    "ocean",
    "orchard",
    "orchid",
    "osprey",
    "otter",
    "owl",
    "palm",
    "panther",
    "pasture",
    "path",
    "peak",
    "pearl",
    "pebble",
    "pelican",
    "petal",
    "pier",
    "pigeon",
    "pillar",
    "pine",
    "plateau",
    "plum",
    "pond",
    "poplar",
    "prairie",
    "quail",
    "quarry",
    "quartz",
    "quay",
    "rabbit",
    "rain",
    "rapids",
    "raven",
    "reef",
    "ridge",
    "rift",
    "ripple",
    "river",
    "robin",
    "rock",
    "rook",
    "root",
    "rose",
    "sable",
    "sage",
    "sail",
    "sand",
    "sapling",
    "savanna",
    "sequoia",
    "shale",
    "shell",
    "shoal",
    "shore",
    "shrub",
    "sierra",
    "silo",
    "sky",
    "slope",
    "snow",
    "sparrow",
    "spire",
    "spring",
    "spruce",
    "spur",
    "stag",
    "star",
    "steppe",
    "stone",
    "stork",
    "strand",
    "stream",
    "summit",
    "sun",
    "swallow",
    "swan",
    "sycamore",
    "tarn",
    "teal",
    "temple",
    "terrace",
    "thermal",
    "thicket",
    "thistle",
    "thorn",
    "thrush",
    "tide",
    "timber",
    "toucan",
];

const VERBS: [&str; 256] = [
    "abide", "adorn", "amble", "anchor", "arch", "arise", "ascend", "balance", "bank", "bask",
    "beam", "beckon", "bend", "bide", "blaze", "bloom", "blossom", "bound", "brace", "branch",
    "breeze", "brighten", "bristle", "broaden", "buoy", "burrow", "carry", "carve", "cascade",
    "cast", "chart", "chase", "cheer", "chime", "circle", "claim", "clasp", "clear", "cleave",
    "climb", "cling", "coast", "coax", "coil", "collect", "comb", "crest", "cross", "crown",
    "cruise", "cull", "curl", "curve", "dance", "dart", "dash", "dawn", "deepen", "descend", "dip",
    "dive", "draw", "drift", "dwell", "ease", "ebb", "echo", "edge", "elevate", "ember", "emerge",
    "enfold", "enter", "escort", "fall", "fan", "fasten", "ferry", "fetch", "fill", "filter",
    "finish", "flag", "flank", "flare", "flee", "flick", "fling", "flip", "float", "flourish",
    "flow", "flutter", "fold", "follow", "forage", "forge", "gather", "glide", "glimmer",
    "glisten", "glow", "grace", "graze", "grow", "guard", "guide", "gust", "harbour", "harvest",
    "haul", "haunt", "heave", "herd", "hold", "hover", "hurry", "ignite", "journey", "keep",
    "kindle", "laze", "leap", "lift", "linger", "listen", "loop", "lull", "mantle", "mark",
    "meander", "mend", "mingle", "mirror", "mount", "muster", "nestle", "open", "orbit", "paddle",
    "pass", "pause", "perch", "pivot", "plait", "play", "plume", "ponder", "pool", "press",
    "prowl", "pull", "quiver", "race", "radiate", "ramble", "range", "reach", "rebound", "recall",
    "reflect", "rest", "return", "ride", "rise", "roam", "rock", "roll", "rove", "rush", "sail",
    "scale", "scatter", "search", "settle", "shape", "shelter", "shift", "shimmer", "shine",
    "sift", "signal", "sing", "skim", "skip", "slide", "slip", "soar", "sound", "spark", "speak",
    "spin", "spiral", "split", "spread", "spring", "sprout", "stack", "stand", "steady", "steer",
    "stir", "stitch", "stray", "stream", "stretch", "stride", "strike", "stroll", "surge", "sway",
    "sweep", "swell", "swim", "swing", "tack", "tail", "take", "tally", "tap", "temper", "tend",
    "thread", "thrive", "tilt", "tip", "toll", "top", "toss", "trace", "track", "trail", "travel",
    "traverse", "trek", "trim", "trot", "tumble", "turn", "twine", "twirl", "unfold", "unfurl",
    "usher", "vault", "veer", "venture", "voyage", "wade", "wake", "wander", "watch", "wave",
    "weave", "wheel", "whirl",
];

/// A room name nobody chose (names.ts `generateRoomName`): three words and
/// four digits, which is 37 bits.
///
/// A room here is a name and nothing else, so how hard it is to guess is a
/// security property rather than a matter of taste; drawn from the operating
/// system's generator for that reason, as the web draws from
/// `crypto.getRandomValues`.
#[uniffi::export]
pub fn generate_room_name() -> String {
    let mut bytes = [0u8; 5];
    rand::rng().fill_bytes(&mut bytes);

    let [word, thing, action, high, low] = bytes;
    let digits = ((u32::from(high) << 8) | u32::from(low)) % 10u32.pow(DIGITS as u32);

    format!(
        "{}-{}-{}-{digits:0width$}",
        ADJECTIVES[usize::from(word)],
        NOUNS[usize::from(thing)],
        VERBS[usize::from(action)],
        width = DIGITS
    )
}

/// The rule a room name has always been held to, for either half of one
/// (names.ts `plain`): runs of anything else become one dash, leading dashes
/// go, the length is held, and trailing dashes go after the cut.
fn plain(typed: &str, longest: usize) -> String {
    let mut out = String::with_capacity(typed.len());
    let mut dashed = false;

    for c in typed.chars() {
        let c = if c.is_ascii_lowercase() || c.is_ascii_digit() {
            c
        } else {
            '-'
        };
        if c == '-' {
            // One dash for any run of dashes or of refused characters, and
            // none at the start.
            if !dashed && !out.is_empty() {
                out.push('-');
            }
            dashed = true;
        } else {
            out.push(c);
            dashed = false;
        }
    }

    out.truncate(longest);
    out.trim_end_matches('-').to_owned()
}

/// Reduces what somebody typed to a name the server will accept, as they type
/// (names.ts `normaliseRoomName`).
///
/// One `@`, and each side held to its own rule; a second `@` is folded into
/// the scope as a dash, because a name that splits two ways is the one thing a
/// scope must never be. The `@` itself is kept while nothing follows it, or a
/// scope could only ever be pasted.
#[uniffi::export]
pub fn normalise_room_name(typed: String) -> String {
    let lowered = typed.to_lowercase();

    match lowered.split_once('@') {
        None => plain(&lowered, MAX_ROOM_NAME),
        Some((local, scope)) => format!(
            "{}@{}",
            plain(local, MAX_ROOM_NAME),
            plain(scope, MAX_SCOPE)
        ),
    }
}

/// `^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$`.
fn plain_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    let edge = |b: &u8| b.is_ascii_lowercase() || b.is_ascii_digit();

    match (bytes.first(), bytes.last()) {
        (Some(first), Some(last)) => {
            edge(first) && edge(last) && bytes.iter().all(|b| edge(b) || *b == b'-')
        }
        _ => false,
    }
}

/// Whether a name is one the server will authorise: a room, or a room and the
/// scope it is held under after a single `@` (names.ts `validRoomName`).
#[uniffi::export]
pub fn valid_room_name(name: String) -> bool {
    let mut halves = name.split('@');
    let local = halves.next().unwrap_or_default();
    let scope = halves.next();

    if halves.next().is_some() || local.len() > MAX_ROOM_NAME || !plain_name(local) {
        return false;
    }

    scope.is_none_or(|scope| valid_scope(scope.to_owned()))
}

/// Whether a scope is one an account may carry and a room may name.
#[uniffi::export]
pub fn valid_scope(scope: String) -> bool {
    scope.len() <= MAX_SCOPE && plain_name(&scope)
}

/// The scope a room is held under, or empty for a plain room.
#[uniffi::export]
pub fn scope_of(name: String) -> String {
    name.split_once('@')
        .map(|(_, scope)| scope.to_owned())
        .unwrap_or_default()
}

/// Whether a name looks generated rather than chosen, which decides whether to
/// warn that a room can be guessed (names.ts `looksGenerated`).
#[uniffi::export]
pub fn looks_generated(name: String) -> bool {
    let parts: Vec<&str> = name.split('-').collect();
    let [word, thing, action, digits] = parts.as_slice() else {
        return false;
    };

    digits.len() == DIGITS
        && digits.bytes().all(|b| b.is_ascii_digit())
        && ADJECTIVES.contains(word)
        && NOUNS.contains(thing)
        && VERBS.contains(action)
}
