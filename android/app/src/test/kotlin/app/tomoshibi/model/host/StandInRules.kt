package app.tomoshibi.model.host

import uniffi.tomoshibi_protocol.IdentitySignature

/*
 * The protocol crate's rules stood in for, where the core's library cannot
 * be loaded (a computer running the tests), by what each one means.
 */

/** The protocol crate's rules, by their plain meanings. */
internal fun standInRules() {
    // A proven identity starts "p-" and a guest's "g-", as signed and
    // issued ones are told apart by their prefix in the crate.
    CrateRules.signature = { id ->
        when {
            id.startsWith("p-") -> IdentitySignature(id.drop(2).padEnd(10, 'x').take(10), proven = true, account = false)
            id.startsWith("g-") -> IdentitySignature("guest00000", proven = false, account = false)
            else -> null
        }
    }
    CrateRules.scope = { "" }
    // name.ts: wearing a name somebody else in the room proved, without proving it.
    CrateRules.impersonating = { one, everybody ->
        !one.identity.startsWith("p-") && everybody.any { it.identity != one.identity && it.identity.startsWith("p-") && it.name == one.name }
    }
    CrateRules.watchers = { me, roster -> roster.filter { it.attributes["watching"] == me }.map { it.identity } }
    CrateRules.reactionChoices = { REACTIONS }
    CrateRules.reactionShownForMs = { 4000L }
    CrateRules.reactionPayload = { it.toByteArray() }
    CrateRules.receivedReaction = { topic, _, payload -> if (topic == "reaction") String(payload) else null }
}

/** hands.rs `REACTIONS`, by their code points. */
internal val REACTIONS = listOf(
    String(Character.toChars(0x1F44D)), String(Character.toChars(0x1F44F)), String(Character.toChars(0x1F602)),
    String(Character.toChars(0x1F389)), String(Character.toChars(0x2764)) + "️", String(Character.toChars(0x1F62E)),
)
