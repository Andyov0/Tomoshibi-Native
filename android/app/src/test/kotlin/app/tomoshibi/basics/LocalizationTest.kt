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

    /**
     * The deployment's phrases (Remote) come before the app's own, in the
     * language showing, placeholders and all; a language it wrote nothing in
     * keeps the app's own, and so does a phrase it did not reword.
     */
    @Test
    fun phrasesTheDeploymentLaysOverComeFirstInTheLanguageShowing() {
        val words = Localization.forTesting(
            mapOf(
                Language.Ja to mapOf("Leave" to "退出", "Show {name} larger" to "{name}を大きく表示"),
                Language.ZhHans to mapOf("Leave" to "离开"),
            ),
            Language.Ja,
        )
        words.laidOver = mapOf(Language.ZhHans to mapOf("Leave" to "退出会议", "Show {name} larger" to "放大{name}"))
        assertEquals("the Japanese reader keeps the app's own", "退出", words.t("Leave"))
        words.choose(Language.ZhHans)
        assertEquals("退出会议", words.t("Leave"))
        assertEquals("放大Ann", words.t("Show {name} larger", mapOf("name" to "Ann")))
        assertEquals("退出会议", words.say("Leave"))
        assertEquals("a phrase not reworded is the app's own", "Stay", words.t("Stay"))
        words.choose(Language.Ja)
        assertEquals("退出", words.t("Leave"))
        // Taken back by the deployment: the app's own again.
        words.choose(Language.ZhHans)
        words.laidOver = emptyMap()
        assertEquals("离开", words.t("Leave"))
    }
}
