package app.tomoshibi.model

import org.junit.Assert.assertEquals
import org.junit.Test

/*
 * The guest identity and its proof, kept together. What these guard: a proof
 * sent beside an identity it was not issued with (the server would issue a
 * fresh identity, and a guest would lose its place and any pin), a proof left
 * behind when a passphrase join replaced the identity, and the identity no
 * longer being found where earlier versions of the app kept it.
 */
class KeptIdentityTest {
    private val base = "http://10.0.2.2:8080/"

    @Test
    fun aGuestsIdentityComesBackWithItsProof() {
        val defaults = MemoryDefaults()
        assertEquals(KeptIdentity(), KeptIdentity.read(defaults, base))
        KeptIdentity.keep(KeptIdentity("g1", "p1"), defaults, base)
        assertEquals(KeptIdentity("g1", "p1"), KeptIdentity.read(defaults, base))
    }

    @Test
    fun anAnswerWithoutAProofForgetsTheOldOne() {
        val defaults = MemoryDefaults()
        KeptIdentity.keep(KeptIdentity("g1", "p1"), defaults, base)
        KeptIdentity.keep(KeptIdentity("s1", null), defaults, base)
        assertEquals(KeptIdentity("s1", null), KeptIdentity.read(defaults, base))
        KeptIdentity.keep(KeptIdentity("g2", ""), defaults, base)
        assertEquals(null, KeptIdentity.read(defaults, base).proof)
    }

    @Test
    fun theIdentityIsWhereEarlierVersionsKeptItAndEachDeploymentHasItsOwn() {
        val defaults = MemoryDefaults()
        defaults.set("tomoshibi.identity.$base", "old")
        assertEquals(KeptIdentity("old", null), KeptIdentity.read(defaults, base))
        KeptIdentity.keep(KeptIdentity("g1", "p1"), defaults, "https://meet.example.com/")
        assertEquals(KeptIdentity("old", null), KeptIdentity.read(defaults, base))
    }

    @Test
    fun aProofWithoutItsIdentityIsNeverRead() {
        val defaults = MemoryDefaults()
        defaults.set(KeptIdentity.keys(base).second, "stray")
        assertEquals(KeptIdentity(), KeptIdentity.read(defaults, base))
    }
}
