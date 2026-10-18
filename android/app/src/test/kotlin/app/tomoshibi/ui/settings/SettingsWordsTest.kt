package app.tomoshibi.ui.settings

import org.json.JSONObject
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * Every phrase the settings say is in the catalog, as the Apple apps'
 * LocalizationCatalogTests hold their sources to: a phrase missing from it is
 * shown in English to everybody, which nothing else would notice. In the
 * catalog at all is in it in every language, since the generator refuses a
 * phrase that lacks one; a table leaves out a translation that is the English
 * itself (Japanese writes "Bluetooth" so), so presence in any of them counts.
 * "1080p" and "720p" are the same in every language and have no entry, on
 * Apple as here.
 */
class SettingsWordsTest {
    @Test
    fun everyPhraseIsInTheCatalogInEveryLanguage() {
        val catalog = JSONObject(File("src/main/assets/phrases.json").readText())
        val sources = File("src/main/kotlin/app/tomoshibi/ui/settings").listFiles { file -> file.extension == "kt" }.orEmpty()
        assertTrue("no sources found", sources.isNotEmpty())
        val said = Regex("""\bt\("((?:[^"\\]|\\.)*)"""")
        val neutral = setOf("1080p", "720p")
        val missing = sources.flatMap { file ->
            said.findAll(file.readText()).map { it.groupValues[1].replace("\\\"", "\"") }
                .filter { it !in neutral }
                .filter { phrase -> listOf("zh-Hans", "zh-Hant", "ja").none { catalog.getJSONObject(it).has(phrase) } }
                .map { "${file.name}: $it" }
                .toList()
        }
        assertTrue("not in the catalog:\n" + missing.joinToString("\n"), missing.isEmpty())
    }
}
