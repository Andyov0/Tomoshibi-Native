import Foundation
import Security

/// A room name nobody chose, for where the core is not: the Messages
/// extension, which starts a meeting for now and has to name it.
///
/// The core's `generate_room_name` (core/protocol/src/identity.rs), which is
/// the web's names.ts `generateRoomName`: three words from lists of 256 and
/// four digits, drawn from the system's generator. The lists are the core's,
/// in its order, because a name made here is read by everything else. The
/// web and the core tell a generated name from a chosen one by finding its
/// words in these lists (`looksGenerated`), and a name chosen by somebody is
/// warned about as guessable where a generated one is not; a word missing
/// here would make a name the core calls chosen. RoomNamesTests compares the
/// lists with the core's source, so the two cannot drift apart unseen.
///
/// How hard the name is to guess is a security property, not a matter of
/// taste: a room here is its name. So the bytes come from SecRandomCopyBytes,
/// as the web's come from crypto.getRandomValues, and each list being 256 long
/// lets one byte pick a word without the bias a modulus would add.
public enum RoomNames {
    public static func generated() -> String {
        var bytes = [UInt8](repeating: 0, count: 5)
        if SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes) != errSecSuccess {
            // Swift's own generator is arc4random on Apple's systems, a
            // cryptographic one too; it is only the second choice because
            // the Security framework's is the one documented as such.
            var any = SystemRandomNumberGenerator()
            bytes = bytes.map { _ in UInt8.random(in: .min ... .max, using: &any) }
        }
        return name(from: bytes)
    }

    /// The name five bytes make, as the core makes it from the same five.
    static func name(from bytes: [UInt8]) -> String {
        let number = (Int(bytes[3]) << 8 | Int(bytes[4])) % 10_000
        return "\(adjectives[Int(bytes[0])])-\(nouns[Int(bytes[1])])-\(verbs[Int(bytes[2])])-" + String(format: "%04d", number)
    }

    // The core's ADJECTIVES, NOUNS and VERBS, in their order.

    static let adjectives: [String] = [
        "amber", "ancient", "arctic", "autumn", "azure", "balmy", "blazing", "bold", "brave", "breezy",
        "bright", "brisk", "bronze", "calm", "candid", "cheerful", "chilly", "civic", "clear", "clever",
        "cobalt", "cosmic", "cozy", "crimson", "crisp", "curious", "daring", "dawn", "deep", "deft",
        "dizzy", "dusky", "eager", "early", "earnest", "easy", "elder", "electric", "elegant", "ember",
        "empty", "endless", "fair", "faithful", "famous", "fancy", "fearless", "feisty", "fertile",
        "fiery", "final", "fine", "firm", "first", "flat", "fleet", "floral", "fluent", "fond", "formal",
        "forward", "fragrant", "free", "fresh", "friendly", "frosty", "frugal", "full", "gallant",
        "gentle", "giant", "gifted", "glad", "glassy", "gleaming", "glossy", "golden", "gracious", "grand",
        "grassy", "gray", "great", "green", "happy", "hardy", "harsh", "hasty", "hazel", "healthy",
        "hearty", "heavy", "helpful", "hidden", "high", "hollow", "honest", "hopeful", "humble", "hungry",
        "ideal", "idle", "indigo", "inner", "ivory", "jade", "jolly", "joyful", "keen", "kind", "lanky",
        "large", "late", "lavish", "lean", "level", "light", "lilac", "limber", "lively", "lofty",
        "logical", "lone", "loud", "lower", "loyal", "lucid", "lucky", "lunar", "lush", "magenta", "major",
        "mellow", "merry", "mighty", "mild", "milky", "mindful", "minor", "misty", "modern", "modest",
        "moral", "mossy", "mystic", "narrow", "native", "neat", "neutral", "nimble", "noble", "noisy",
        "normal", "northern", "novel", "oaken", "olive", "open", "orange", "ornate", "outer", "patient",
        "peaceful", "pearly", "perfect", "placid", "plain", "playful", "pleasant", "plum", "polar",
        "polite", "prime", "prompt", "proper", "proud", "pure", "purple", "quaint", "quick", "quiet",
        "radiant", "rapid", "rare", "ready", "regal", "restful", "rich", "right", "ripe", "rising",
        "robust", "rosy", "round", "royal", "ruby", "rugged", "rural", "rustic", "sable", "safe", "sandy",
        "scarlet", "secret", "serene", "sharp", "sheer", "shiny", "short", "silent", "silken", "silver",
        "simple", "sincere", "skilled", "slender", "slight", "small", "smooth", "snowy", "soft", "solar",
        "solemn", "solid", "sombre", "sonic", "sound", "southern", "spare", "spirited", "splendid", "spry",
        "stable", "steady", "stellar", "stern", "still", "stormy", "stout", "straight", "strong", "sturdy",
        "subtle", "sudden", "summer", "sunny", "supple", "sure", "swift", "tall", "tame", "tender",
        "thankful", "thorough", "thrifty", "tidal", "tidy"
    ]

    static let nouns: [String] = [
        "abbey", "acorn", "alcove", "anchor", "anthem", "arbour", "arch", "archer", "arrow", "aspen",
        "atlas", "aurora", "badger", "banner", "basalt", "basin", "bay", "beacon", "beam", "bear",
        "beaver", "beech", "bell", "birch", "bison", "blossom", "bluff", "bough", "boulder", "branch",
        "breaker", "breeze", "bridge", "brook", "buffalo", "bugle", "burrow", "cabin", "canal", "canopy",
        "canyon", "cape", "cardinal", "cascade", "castle", "cavern", "cedar", "channel", "chapel", "chart",
        "chasm", "cliff", "cloud", "clover", "comet", "compass", "copper", "coral", "cottage", "cove",
        "crane", "crater", "creek", "crest", "crow", "crown", "crystal", "current", "cypress", "dahlia",
        "dawn", "dell", "delta", "dew", "dolphin", "dove", "drift", "dune", "dusk", "eagle", "echo", "elm",
        "ember", "estuary", "falcon", "fawn", "fell", "fern", "field", "finch", "fissure", "fjord",
        "flame", "flint", "forest", "fountain", "fox", "gale", "garden", "gate", "geyser", "glacier",
        "glade", "glen", "gorge", "granite", "grove", "gull", "hamlet", "harbour", "harvest", "haven",
        "hawk", "hazel", "heath", "heather", "hedge", "heron", "hickory", "hill", "hollow", "horizon",
        "hummock", "ibis", "inlet", "iris", "island", "ivy", "jasmine", "jetty", "juniper", "kestrel",
        "key", "kite", "knoll", "lagoon", "lake", "lantern", "larch", "lark", "laurel", "ledge", "lemur",
        "lighthouse", "lily", "linden", "lion", "lotus", "lynx", "magnolia", "maple", "marsh", "meadow",
        "meander", "mesa", "meteor", "mill", "mist", "moor", "moss", "moth", "mountain", "mulberry",
        "narrows", "nebula", "nest", "oak", "oasis", "ocean", "orchard", "orchid", "osprey", "otter",
        "owl", "palm", "panther", "pasture", "path", "peak", "pearl", "pebble", "pelican", "petal", "pier",
        "pigeon", "pillar", "pine", "plateau", "plum", "pond", "poplar", "prairie", "quail", "quarry",
        "quartz", "quay", "rabbit", "rain", "rapids", "raven", "reef", "ridge", "rift", "ripple", "river",
        "robin", "rock", "rook", "root", "rose", "sable", "sage", "sail", "sand", "sapling", "savanna",
        "sequoia", "shale", "shell", "shoal", "shore", "shrub", "sierra", "silo", "sky", "slope", "snow",
        "sparrow", "spire", "spring", "spruce", "spur", "stag", "star", "steppe", "stone", "stork",
        "strand", "stream", "summit", "sun", "swallow", "swan", "sycamore", "tarn", "teal", "temple",
        "terrace", "thermal", "thicket", "thistle", "thorn", "thrush", "tide", "timber", "toucan"
    ]

    static let verbs: [String] = [
        "abide", "adorn", "amble", "anchor", "arch", "arise", "ascend", "balance", "bank", "bask", "beam",
        "beckon", "bend", "bide", "blaze", "bloom", "blossom", "bound", "brace", "branch", "breeze",
        "brighten", "bristle", "broaden", "buoy", "burrow", "carry", "carve", "cascade", "cast", "chart",
        "chase", "cheer", "chime", "circle", "claim", "clasp", "clear", "cleave", "climb", "cling",
        "coast", "coax", "coil", "collect", "comb", "crest", "cross", "crown", "cruise", "cull", "curl",
        "curve", "dance", "dart", "dash", "dawn", "deepen", "descend", "dip", "dive", "draw", "drift",
        "dwell", "ease", "ebb", "echo", "edge", "elevate", "ember", "emerge", "enfold", "enter", "escort",
        "fall", "fan", "fasten", "ferry", "fetch", "fill", "filter", "finish", "flag", "flank", "flare",
        "flee", "flick", "fling", "flip", "float", "flourish", "flow", "flutter", "fold", "follow",
        "forage", "forge", "gather", "glide", "glimmer", "glisten", "glow", "grace", "graze", "grow",
        "guard", "guide", "gust", "harbour", "harvest", "haul", "haunt", "heave", "herd", "hold", "hover",
        "hurry", "ignite", "journey", "keep", "kindle", "laze", "leap", "lift", "linger", "listen", "loop",
        "lull", "mantle", "mark", "meander", "mend", "mingle", "mirror", "mount", "muster", "nestle",
        "open", "orbit", "paddle", "pass", "pause", "perch", "pivot", "plait", "play", "plume", "ponder",
        "pool", "press", "prowl", "pull", "quiver", "race", "radiate", "ramble", "range", "reach",
        "rebound", "recall", "reflect", "rest", "return", "ride", "rise", "roam", "rock", "roll", "rove",
        "rush", "sail", "scale", "scatter", "search", "settle", "shape", "shelter", "shift", "shimmer",
        "shine", "sift", "signal", "sing", "skim", "skip", "slide", "slip", "soar", "sound", "spark",
        "speak", "spin", "spiral", "split", "spread", "spring", "sprout", "stack", "stand", "steady",
        "steer", "stir", "stitch", "stray", "stream", "stretch", "stride", "strike", "stroll", "surge",
        "sway", "sweep", "swell", "swim", "swing", "tack", "tail", "take", "tally", "tap", "temper",
        "tend", "thread", "thrive", "tilt", "tip", "toll", "top", "toss", "trace", "track", "trail",
        "travel", "traverse", "trek", "trim", "trot", "tumble", "turn", "twine", "twirl", "unfold",
        "unfurl", "usher", "vault", "veer", "venture", "voyage", "wade", "wake", "wander", "watch", "wave",
        "weave", "wheel", "whirl"
    ]
}
