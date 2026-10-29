package app.tomoshibi.model

/**
 * The identity a deployment gave this app on its last join, and the proof it
 * gave with it, kept together and sent back together (api.ts keeps
 * `meet-live.identity` and `meet-live.identity-proof` side by side in the
 * tab's storage; the Apple apps' KeptIdentity is the same).
 *
 * A guest's identity is issued by the server and is on every roster in the
 * room, so anybody could send it back as their own and be seated in the
 * guest's place; the server keeps an issued identity only for whoever shows
 * the proof it handed out with it, and issues a fresh one otherwise. Kept
 * apart, or not at all, an app guest was a new participant at every
 * reconnect, move and "Join again", lost a pin an operator had set for it, and
 * read to everybody else as somebody leaving and a stranger arriving. So the
 * proof lives exactly where the identity lives and for exactly as long, and an
 * answer that comes without one (a passphrase or an account keeps its identity
 * by itself; an older server hands none out) clears it, as api.ts clears it,
 * so a proof is never sent beside an identity it is not for.
 */
data class KeptIdentity(val identity: String? = null, val proof: String? = null) {
    companion object {
        /** Where the two are kept for the deployment at [base]: the identity under the name it always had, the proof beside it. */
        fun keys(base: String): Pair<String, String> = "tomoshibi.identity.$base" to "tomoshibi.identity-proof.$base"

        fun read(defaults: Defaults, base: String): KeptIdentity {
            val (identityKey, proofKey) = keys(base)
            val identity = defaults.string(identityKey)?.takeIf { it.isNotEmpty() }
            // A proof outliving its identity is about nobody.
            val proof = identity?.let { defaults.string(proofKey)?.takeIf { it.isNotEmpty() } }
            return KeptIdentity(identity, proof)
        }

        /** What a join answered, kept whole: the identity, and its proof or none. */
        fun keep(answer: KeptIdentity, defaults: Defaults, base: String) {
            val (identityKey, proofKey) = keys(base)
            defaults.set(identityKey, answer.identity)
            val proof = answer.proof?.takeIf { it.isNotEmpty() && answer.identity != null }
            if (proof != null) defaults.set(proofKey, proof) else defaults.remove(proofKey)
        }
    }
}
