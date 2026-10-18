package app.tomoshibi.feature.watch

import app.tomoshibi.model.Defaults

/**
 * How sharp a video watched together may get on this screen
 * (web/src/live/preferences.ts, `watchQuality`, `levelCap`, `capping`;
 * TomoshibiBasics/WatchCeiling.swift).
 *
 * A ceiling and not a choice of quality: what plays is still decided by the
 * line, so somebody on a slow connection who said 1080p gets whatever their
 * connection carries, and somebody who said 720p never gets more however fast
 * it is. It exists for the second person, on a metered phone or a line shared
 * with a household.
 *
 * Kept under the web's own key and in the web's own words, so the account
 * page's setting and the apps' are one habit. The settings screen writes it;
 * the players read it when a video is opened, as the web's does, so a change
 * applies from the next video.
 */
enum class WatchCeiling(val word: String, private val lines: Int?) {
    Auto("auto", null),
    P1080("1080p", 1080),
    P720("720p", 720);

    /** Writes it down; no ceiling removes the key rather than writing "auto". */
    fun remember(defaults: Defaults?) {
        if (this == Auto) defaults?.remove(STORAGE_KEY) else defaults?.set(STORAGE_KEY, word)
    }

    /** One rung of a playlist, as much of it as the rule needs: its size, if the playlist says it. */
    data class Level(val width: Double?, val height: Double)

    /**
     * The highest rung this ceiling allows, as an index into the rungs in the
     * order the player ranks them (by bitrate), or null for no ceiling at all
     * (`levelCap`, whose -1 this is).
     *
     * Measured on the shorter side, so that 1080p means what people mean by it
     * whatever the shape: a film cropped to 1920x800 is a 1080p film and a 4K
     * one cropped to 3840x1600 is not. Eight lines of slack, because encoders
     * round a picture up to whole sixteen-pixel blocks and a 1080p stream that
     * says 1088 is still the one asked for. A rung that does not say its size
     * is allowed, since refusing it could leave nothing to play; the ceiling
     * stops below the first rung that is too tall, wherever the list puts it;
     * and where even the first is too tall, that one is allowed, because
     * somebody who asked for less is better served by the least there is than
     * by a player with nothing it may play.
     */
    fun cap(levels: List<Level>): Int? {
        val lines = lines ?: return null
        val most = (lines + 8).toDouble()
        val tooTall = levels.indexOfFirst { level ->
            val side = level.width?.let { if (it > 0) minOf(it, level.height) else level.height } ?: level.height
            side > most
        }
        if (tooTall < 0) return null
        return maxOf(0, tooTall - 1)
    }

    /**
     * What a player's own ceiling should become (`capping`): lowered to the
     * viewer's, never raised. The player may already hold the picture below it
     * for its own reasons -- sized to the tile -- and that ceiling is the
     * deployment's bandwidth, which this choice may only save more of.
     */
    fun capping(current: Int?, levels: List<Level>): Int? {
        val most = cap(levels) ?: return current
        if (current == null) return most
        return minOf(current, most)
    }

    companion object {
        const val STORAGE_KEY = "meet-live.watch-quality"

        /**
         * The ceiling chosen, or none: anything that is not one of the stored
         * words (a hand edit, "1080P", "4k") reads as none, as the web's does.
         */
        fun stored(defaults: Defaults?): WatchCeiling {
            val word = defaults?.string(STORAGE_KEY) ?: return Auto
            return entries.firstOrNull { it.word == word } ?: Auto
        }
    }
}
