package app.tomoshibi.basics

import java.util.Locale

// STAND-IN, to be replaced at merge by the real app.tomoshibi.basics
// Localization (the phrase tables in four languages), which is being written
// beside this branch. Only the signatures the models call are here, and they
// are the real file's: `t` with a map of values, `say` for text the
// deployment supplies. `readingLocale` is the one addition; at merge it
// becomes `Localization.shared.language.locale`.

/**
 * Says [phrase], its `{placeholders}` filled from [values]: the English key
 * as it is, which is what the real tables fall back to as well.
 */
fun t(phrase: String, values: Map<String, Any?>? = null): String {
    if (values == null) return phrase
    val out = StringBuilder()
    var at = 0
    while (at < phrase.length) {
        val open = phrase.indexOf('{', at)
        val close = if (open < 0) -1 else phrase.indexOf('}', open + 1)
        if (open < 0 || close < 0) {
            out.append(phrase, at, phrase.length)
            break
        }
        out.append(phrase, at, open)
        val name = phrase.substring(open + 1, close)
        out.append(rendered(values[name]))
        at = close + 1
    }
    return out.toString()
}

/** Text the deployment supplies, such as a relay's label, as it was typed. */
fun say(text: String): String = text

/** The locale dates and times are written in for the reader. */
fun readingLocale(): Locale = Locale.getDefault()

/** A value as the web's `String(value)` writes it: 50, never 50.0. */
private fun rendered(value: Any?): String = when (value) {
    null -> ""
    is Double -> if (value.isFinite() && value == Math.rint(value) && kotlin.math.abs(value) < 9.0e15) value.toLong().toString() else value.toString()
    is Float -> rendered(value.toDouble())
    else -> value.toString()
}
