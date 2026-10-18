package app.tomoshibi.basics

import android.content.Context
import android.content.SharedPreferences
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color

/**
 * The colour the interface spends, chosen by the person using it, and the
 * two other choices about how a call looks (Appearance.swift).
 *
 * The product has one signal colour, the amber a studio camera lights when it
 * is live; everything that says "this is happening" wears it. Red for leaving
 * and errors and green for a connection light stay fixed: they are read for
 * their meaning, not noticed. Stored under the same keys as on Apple, and the
 * controls under the web's own name.
 */
class Appearance private constructor(private val preferences: SharedPreferences?) {
    var accent by mutableStateOf(
        preferences?.getString(ACCENT_KEY, null)?.let { Accent.of(it) } ?: Accent.Amber)
        private set

    var connectionBadge by mutableStateOf(
        preferences?.getString(BADGE_KEY, null)?.let { ConnectionBadge.of(it) } ?: ConnectionBadge.Signal)
        private set

    var controlsPlacement by mutableStateOf(
        preferences?.getString(CONTROLS_KEY, null)?.let { ControlsPlacement.of(it) } ?: ControlsPlacement.Always)
        private set

    /**
     * Whether choices are written down: a test driving the app shows its
     * choices while it runs and keeps none, since the defaults it would write
     * are the owner's.
     */
    var keeps = true

    fun choose(accent: Accent) {
        this.accent = accent
        keep(ACCENT_KEY, accent.key)
    }

    fun choose(badge: ConnectionBadge) {
        connectionBadge = badge
        keep(BADGE_KEY, badge.key)
    }

    /** "always" is kept as nothing, as the web keeps it. */
    fun choose(placement: ControlsPlacement) {
        controlsPlacement = placement
        keep(CONTROLS_KEY, if (placement == ControlsPlacement.Always) null else placement.key)
    }

    private fun keep(key: String, value: String?) {
        if (!keeps || preferences == null) return
        preferences.edit().apply { if (value == null) remove(key) else putString(key, value) }.apply()
    }

    companion object {
        const val ACCENT_KEY = "tomoshibi.accent"
        const val BADGE_KEY = "tomoshibi.connection-badge"
        const val CONTROLS_KEY = "meet-live.controls"

        @Volatile private var installed: Appearance? = null
        private val fallback by lazy { Appearance(null) }

        val shared: Appearance get() = installed ?: fallback

        @Synchronized
        fun install(context: Context): Appearance {
            installed?.let { return it }
            return Appearance(context.getSharedPreferences(Preferences.FILE, Context.MODE_PRIVATE)).also { installed = it }
        }
    }
}

/**
 * The colours on offer, each with the colour drawn on top of it: Apple's
 * dark-appearance system colours apart from the first, which is the
 * product's, the same on every platform so that somebody's choice looks the
 * same on their phone and their Mac.
 */
enum class Accent(val key: String, val color: Color, val onColor: Color) {
    Amber("amber", Color(1.0f, 0.635f, 0.231f), Color(0.122f, 0.071f, 0.0f)),
    Orange("orange", Color(1.0f, 0.624f, 0.039f), Color(0.122f, 0.071f, 0.0f)),
    Pink("pink", Color(1.0f, 0.216f, 0.373f), Color.White),
    Purple("purple", Color(0.749f, 0.353f, 0.949f), Color.White),
    Indigo("indigo", Color(0.369f, 0.361f, 0.902f), Color.White),
    Blue("blue", Color(0.039f, 0.518f, 1.0f), Color.White),
    Teal("teal", Color(0.251f, 0.784f, 0.878f), Color(0.0f, 0.15f, 0.18f)),
    Green("green", Color(0.188f, 0.820f, 0.345f), Color(0.0f, 0.17f, 0.055f)),
    Graphite("graphite", Color(0.596f, 0.596f, 0.616f), Color(0.07f, 0.07f, 0.08f));

    val displayName: String
        get() = when (this) {
            Amber -> t("Lamplight")
            Orange -> t("Orange")
            Pink -> t("Pink")
            Purple -> t("Purple")
            Indigo -> t("Indigo")
            Blue -> t("Blue")
            Teal -> t("Teal")
            Green -> t("Green")
            Graphite -> t("Graphite")
        }

    companion object {
        fun of(key: String) = entries.firstOrNull { it.key == key }
    }
}

/** How the connection is shown in a call: the bars alone unless asked. */
enum class ConnectionBadge(val key: String) {
    Signal("signal"), Detailed("detailed"), Hidden("hidden");

    val displayName: String
        get() = when (this) {
            Signal -> t("Signal only")
            Detailed -> t("With details")
            Hidden -> t("Hidden")
        }

    companion object {
        fun of(key: String) = entries.firstOrNull { it.key == key }
    }
}

/** The three places the web lets the controls sit (controls.ts). */
enum class ControlsPlacement(val key: String) {
    Always("always"), Idle("idle"), Side("side");

    val displayName: String
        get() = when (this) {
            Always -> t("Always shown")
            Idle -> t("Hide when nothing is happening")
            Side -> t("At the side")
        }

    companion object {
        fun of(key: String) = entries.firstOrNull { it.key == key }
    }
}
