package app.tomoshibi.model.host

import uniffi.tomoshibi_protocol.IdentitySignature
import uniffi.tomoshibi_protocol.NamedIdentity
import uniffi.tomoshibi_protocol.RosterMember

/**
 * The protocol crate's rules that the call's screens and features read: whose
 * name is proven and whose is worn by somebody else (name.ts), which scope a
 * room is held under, who is watching a share (watching.ts), and the six
 * reactions with how long one is seen (hands.ts).
 *
 * Reached through here rather than called straight, because they live in the
 * core's native library, which a picture of the room taken on a computer
 * (Robolectric) cannot load: a test hands in its own answers. The app never
 * changes them.
 */
internal object CrateRules {
    @Volatile var signature: (String) -> IdentitySignature? = { uniffi.tomoshibi_protocol.signatureOf(it) }
    @Volatile var scope: (String) -> String = { uniffi.tomoshibi_protocol.scopeOf(it) }
    @Volatile var impersonating: (NamedIdentity, List<NamedIdentity>) -> Boolean =
        { one, everybody -> uniffi.tomoshibi_protocol.impersonating(one, everybody) }
    @Volatile var watchers: (String, List<RosterMember>) -> List<String> =
        { me, roster -> uniffi.tomoshibi_protocol.watchers(me, true, roster) }
    @Volatile var reactionChoices: () -> List<String> = { uniffi.tomoshibi_protocol.reactionChoices() }
    @Volatile var reactionShownForMs: () -> Long = { uniffi.tomoshibi_protocol.reactionShownForMs().toLong() }
    @Volatile var reactionPayload: (String) -> ByteArray? = {
        try {
            uniffi.tomoshibi_protocol.reactionPayload(it)
        } catch (_: uniffi.tomoshibi_protocol.ProtocolException) {
            null
        }
    }
    @Volatile var receivedReaction: (String, String, ByteArray) -> String? =
        { topic, from, payload -> uniffi.tomoshibi_protocol.receivedReaction(topic, from, payload) }

    /** Whether somebody joined with a passphrase or an account, so that a name they wear is a claim about them. */
    fun proven(identity: String): Boolean = signature(identity)?.proven == true

    /** Whether the room is held under a scope, where every member runs it. */
    fun scoped(room: String): Boolean = scope(room).isNotEmpty()
}
