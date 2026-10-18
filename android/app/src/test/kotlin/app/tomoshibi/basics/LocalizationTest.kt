package app.tomoshibi.basics

import org.junit.Assert.assertEquals
import org.junit.Test

/** The web's language rules and placeholder substitution, as Apple keeps them. */
class LocalizationTest {
    @Test
    fun aScriptSubtagDecidesBeforeTheRegion() {
        // Simplified Chinese read in Hong Kong: the web's region rule alone
        // would open in traditional characters, against the script chosen.
        assertEquals(Language.ZhHans, Language.detect(listOf("zh-Hans-HK")))
        assertEquals(Language.ZhHant, Language.detect(listOf("zh-Hant-CN")))
        assertEquals(Language.ZhHant, Language.detect(listOf("zh-TW")))
        assertEquals(Language.ZhHans, Language.detect(listOf("zh-CN")))
    }

    @Test
    fun theListIsReadInOrderPastLanguagesNotOffered() {
        assertEquals(Language.Ja, Language.detect(listOf("fr-FR", "ja-JP", "en-US")))
        assertEquals(Language.En, Language.detect(listOf("de-DE")))
    }

    @Test
    fun placeholdersAreFilledAndMissingOnesBecomeNothing() {
        assertEquals("Show Ann larger", Localization.substitute("Show {name} larger", mapOf("name" to "Ann")))
        assertEquals("Show  larger", Localization.substitute("Show {name} larger", mapOf("other" to "x")))
        assertEquals("3 people", Localization.substitute("{count} people", mapOf("count" to 3.0)))
        // A brace that does not close a word is text.
        assertEquals("{not a name}", Localization.substitute("{not a name}", mapOf("not" to "x")))
    }

    @Test
    fun aMissingTranslationFallsBackToTheEnglish() {
        val words = Localization.forTesting(mapOf(Language.Ja to mapOf("Leave" to "退出")), Language.Ja)
        assertEquals("退出", words.t("Leave"))
        assertEquals("Stay", words.t("Stay"))
        assertEquals("退出", words.say("leave"))
    }
}
