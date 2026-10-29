import Foundation

/// The identity a deployment gave this app on its last join, and the proof it
/// gave with it, kept together and sent back together (api.ts keeps
/// `meet-live.identity` and `meet-live.identity-proof` side by side in the
/// tab's storage).
///
/// A guest's identity is issued by the server and is on every roster in the
/// room, so anybody could send it back as their own and be seated in the
/// guest's place; the server keeps an issued identity only for whoever shows
/// the proof it handed out with it, and issues a fresh one otherwise. Kept
/// apart, or not at all, an app guest was a new participant at every
/// reconnect, move and "Join again", lost a pin an operator had set for it,
/// and read to everybody else as somebody leaving and a stranger arriving. So
/// the proof lives exactly where the identity lives and for exactly as long,
/// and an answer that comes without one (a passphrase or an account keeps its
/// identity by itself; an older server hands none out) clears it, as api.ts
/// clears it, so a proof can never be sent beside an identity it is not for.
public struct KeptIdentity: Equatable, Sendable {
    public var identity: String?
    public var proof: String?

    public init(identity: String? = nil, proof: String? = nil) {
        self.identity = identity
        self.proof = proof
    }

    /// Where the two are kept for the deployment at `base`: the identity under
    /// the name it always had, the proof beside it.
    public static func keys(base: String) -> (identity: String, proof: String) {
        ("tomoshibi.identity." + base, "tomoshibi.identity-proof." + base)
    }

    public static func read(_ defaults: UserDefaults, base: String) -> KeptIdentity {
        let keys = keys(base: base)
        let identity = defaults.string(forKey: keys.identity).flatMap { $0.isEmpty ? nil : $0 }
        // A proof outliving its identity is about nobody.
        let proof = identity == nil ? nil : defaults.string(forKey: keys.proof).flatMap { $0.isEmpty ? nil : $0 }
        return KeptIdentity(identity: identity, proof: proof)
    }

    /// What a join answered, kept whole: the identity, and its proof or none.
    public static func keep(_ answer: KeptIdentity, in defaults: UserDefaults, base: String) {
        let keys = keys(base: base)
        defaults.set(answer.identity, forKey: keys.identity)
        if let proof = answer.proof, !proof.isEmpty, answer.identity != nil {
            defaults.set(proof, forKey: keys.proof)
        } else {
            defaults.removeObject(forKey: keys.proof)
        }
    }
}
