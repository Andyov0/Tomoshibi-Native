package app.tomoshibi.basics

import android.content.Context
import android.content.SharedPreferences
import android.os.LocaleList
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import org.json.JSONObject
import java.util.Locale

/**
 * A language the interface is written in: the web client's four
 * (web/src/live/i18n.ts `LOCALES`), with the codes as stored values, as on
 * Apple (TomoshibiBasics/Localization/Language.swift).
 */
enum class Language(val code: String) {
    En("en"),
    ZhHans("zh-Hans"),
    ZhHant("zh-Hant"),
    Ja("ja");

    /**
     * What the language calls itself. Never translated: a language picker is
     * read by somebody who cannot yet read the language on screen.
     */
    val displayName: String
        get() = when (this) {
            En -> "English"
            ZhHans -> "简体中文"
            ZhHant -> "繁體中文"
            Ja -> "日本語"
        }

    val locale: Locale get() = Locale.forLanguageTag(code)

    companion object {
        fun of(code: String?): Language? = entries.firstOrNull { it.code == code }

        /**
         * Which language to open in, given the system's preference list: the
         * web's `detect()` rule for rule, read in order, with an explicit
         * script subtag deciding before a region (Language.swift says why:
         * simplified Chinese with the region set to Hong Kong arrives as
         * `zh-Hans-HK`, which the web's region rule would open in traditional).
         */
        fun detect(preferred: List<String>): Language {
            for (tag in preferred) {
                val lower = tag.lowercase(Locale.ROOT)
                if (lower.startsWith("ja")) return Ja
                if (lower.startsWith("zh")) {
                    val subtags = lower.split('-', '_')
                    if ("hant" in subtags) return ZhHant
                    if ("hans" in subtags) return ZhHans
                    val traditional = listOf("hant", "tw", "hk", "mo").any { lower.contains(it) }
                    return if (traditional) ZhHant else ZhHans
                }
                if (lower.startsWith("en")) return En
            }
            return En
        }

        fun systemPreferences(): List<String> {
            val list = LocaleList.getDefault()
            return (0 until list.size()).map { list[it].toLanguageTag() }
        }
    }
}

/**
 * What the interface says, in the language of whoever is reading it: the
 * web's i18n.ts, as on Apple (Phrases.swift), rather than Android's string
 * resources, which follow the system's language alone while the web lets a
 * person choose one in the call itself, and expect `%s` where every
 * translation is written with `{name}`.
 *
 * Every phrase is keyed by its English, from the same catalog the Apple apps
 * build from (android/i18n.mjs writes assets/phrases.json). A phrase with no
 * translation falls back to its key, a whole readable sentence.
 */
class Localization private constructor(
    private val tables: Map<Language, Map<String, String>>,
    private val preferences: SharedPreferences?,
    systemLanguage: Language,
    chosen: Language?,
) {
    private val folded: Map<Language, Map<String, String>> = tables.mapValues { (_, table) ->
        val out = HashMap<String, String>()
        for (key in table.keys.sorted()) out.putIfAbsent(key.lowercase(Locale.ROOT), table.getValue(key))
        out
    }

    /** Whether the person chose a language rather than following the system's. */
    var chosen by mutableStateOf(chosen != null)
        private set

    /**
     * The language on screen. Snapshot state, so that everything that said
     * something redraws in the new language at once when it changes.
     */
    var language by mutableStateOf(chosen ?: systemLanguage)
        private set

    val followsSystem: Boolean get() = !chosen

    /**
     * Whether a choice is written down: a test driving the app shows its
     * choice while it runs and keeps none, since what it would write over
     * is the owner's.
     */
    var keeps = true

    fun choose(language: Language) {
        if (keeps) preferences?.edit()?.putString(STORAGE_KEY, language.code)?.apply()
        chosen = true
        this.language = language
    }

    fun followSystem() {
        if (keeps) preferences?.edit()?.remove(STORAGE_KEY)?.apply()
        chosen = false
        language = Language.detect(Language.systemPreferences())
    }

    /** The system's languages changed; followed unless one was chosen. */
    fun systemChanged() {
        if (!chosen) language = Language.detect(Language.systemPreferences())
    }

    fun template(phrase: String): String = tables[language]?.get(phrase) ?: phrase

    fun t(phrase: String, values: Map<String, Any?>? = null): String = substitute(template(phrase), values)

    /**
     * Text the deployment supplies rather than the interface, such as a
     * relay's label: translated if it matches a phrase, exactly or ignoring
     * case, and shown as typed otherwise.
     */
    fun say(text: String): String {
        val language = language
        tables[language]?.get(text)?.let { return it }
        folded[language]?.get(text.lowercase(Locale.ROOT))?.let { return it }
        return text
    }

    fun segments(phrase: String): List<PhraseSegment> = PhraseSegment.split(template(phrase))

    companion object {
        /** The web's own storage name for the choice. */
        const val STORAGE_KEY = "meet-live.locale"

        @Volatile private var installed: Localization? = null

        /** The app's, once [install] has run; English keys before that. */
        val shared: Localization
            get() = installed ?: fallback

        private val fallback by lazy { Localization(emptyMap(), null, Language.En, null) }

        /** Once, at start, before anything is said. */
        @Synchronized
        fun install(context: Context): Localization {
            installed?.let { return it }
            val preferences = context.getSharedPreferences(Preferences.FILE, Context.MODE_PRIVATE)
            val raw = context.assets.open("phrases.json").use { it.readBytes().decodeToString() }
            val made = Localization(parse(raw), preferences, Language.detect(Language.systemPreferences()),
                Language.of(preferences.getString(STORAGE_KEY, null)))
            installed = made
            return made
        }

        /** For tests: the tables as given, nothing kept. */
        fun forTesting(tables: Map<Language, Map<String, String>>, language: Language) =
            Localization(tables, null, language, language)

        fun parse(raw: String): Map<Language, Map<String, String>> {
            val json = JSONObject(raw)
            return Language.entries.associateWith { language ->
                val table = json.optJSONObject(language.code) ?: return@associateWith emptyMap()
                buildMap { for (key in table.keys()) put(key, table.getString(key)) }
            }
        }

        /**
         * `{name}` replaced by its value; a placeholder with no value becomes
         * nothing, as on the web. Whole doubles are written without a point,
         * as Swift's `rendered` does, so a count reads "3" rather than "3.0".
         */
        fun substitute(template: String, values: Map<String, Any?>?): String {
            if (values == null) return template
            return PhraseSegment.split(template).joinToString("") { part ->
                val name = part.value ?: return@joinToString part.text
                rendered(values[name])
            }
        }

        private fun rendered(value: Any?): String = when (value) {
            null -> ""
            is Double -> if (value == Math.rint(value) && Math.abs(value) < 9.007199254740992E15) value.toLong().toString() else value.toString()
            is Float -> rendered(value.toDouble())
            else -> value.toString()
        }
    }
}

/** A run of a template: literal text, or a `{placeholder}` and its name. */
data class PhraseSegment(val text: String, val value: String? = null) {
    companion object {
        fun split(template: String): List<PhraseSegment> {
            val parts = ArrayList<PhraseSegment>()
            var literal = 0
            var cursor = 0
            while (cursor < template.length) {
                if (template[cursor] != '{') {
                    cursor++
                    continue
                }
                val nameStart = cursor + 1
                var nameEnd = nameStart
                while (nameEnd < template.length && template[nameEnd].isWordChar()) nameEnd++
                if (nameEnd == nameStart || nameEnd >= template.length || template[nameEnd] != '}') {
                    cursor = nameStart
                    continue
                }
                if (literal < cursor) parts.add(PhraseSegment(template.substring(literal, cursor)))
                parts.add(PhraseSegment(template.substring(cursor, nameEnd + 1), template.substring(nameStart, nameEnd)))
                literal = nameEnd + 1
                cursor = literal
            }
            if (literal < template.length) parts.add(PhraseSegment(template.substring(literal)))
            return parts
        }

        private fun Char.isWordChar() = this in 'a'..'z' || this in 'A'..'Z' || this in '0'..'9' || this == '_'
    }
}

/** Everything the interface says goes through this, keyed by its English. */
fun t(phrase: String, values: Map<String, Any?>? = null): String = Localization.shared.t(phrase, values)

/** Text from the deployment: translated where it matches a phrase. */
fun say(text: String): String = Localization.shared.say(text)

fun phraseSegments(phrase: String): List<PhraseSegment> = Localization.shared.segments(phrase)

/**
 * The locale of the language on screen, for formatting dates and numbers in
 * it: the web sets the document's `lang` as the language changes, and a
 * time written in English inside a Japanese sentence reads wrongly.
 */
fun readingLocale(): Locale = Localization.shared.language.locale
