package app.tomoshibi.ui.call

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.model.Person
import app.tomoshibi.model.Sound
import app.tomoshibi.model.host.HostFeature
import app.tomoshibi.ui.kit.Capsule
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.GlassSlider
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.PanelHeading
import app.tomoshibi.ui.kit.PanelRule
import app.tomoshibi.ui.kit.Monogram
import app.tomoshibi.ui.kit.SwappingSymbol
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.glassPane
import app.tomoshibi.ui.kit.keepDeparting
import app.tomoshibi.ui.kit.panelCard
import app.tomoshibi.ui.kit.pressable
import app.tomoshibi.ui.kit.rememberPress
import app.tomoshibi.ui.kit.fingertip
import uniffi.tomoshibi_protocol.MediaSource
import kotlin.math.roundToInt

/**
 * How loud everybody else is (components/room/SoundPanel.tsx;
 * SoundPanel.swift `SoundPanelView`).
 *
 * A list of what can be heard rather than of what can be seen: the person who
 * is too loud may be on no tile at all. One row per thing there is to hear,
 * because a shared screen's sound and the voice describing it arrive on
 * separate tracks and are turned down separately; nobody hears their own
 * microphone, so there is no row for it. Each person's rows share a card, so
 * that whose sound a slider is never has to be read twice.
 */
@Composable
fun SoundPanel(feature: HostFeature, close: () -> Unit, modifier: Modifier = Modifier) {
    val others = feature.call.people
    Column(modifier.fillMaxWidth().glassPane(22.dp)) {
        PanelHeading(t("Sound"), t("Close sound"), close, symbol = "speaker.wave.2.fill")
        PanelRule()
        Column(Modifier.fillMaxWidth().padding(10.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            AnimatedVisibility(others.isEmpty(), enter = Theme.arrive + expandVertically(Theme.settle()), exit = Theme.depart + shrinkVertically(Theme.leave())) {
                Column(
                    Modifier.fillMaxWidth().padding(vertical = 26.dp),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Symbol("speaker.wave.2", size = 22f, tint = Theme.foreground.copy(alpha = 0.4f))
                    KitText(t("Nobody else is here."), font = Font(12f), color = Theme.muted)
                }
            }
            for (entry in keepDeparting(others) { it.id }) {
                AnimatedVisibility(entry.visible, enter = Theme.arrive + expandVertically(Theme.settle()), exit = Theme.depart + shrinkVertically(Theme.leave())) {
                    PersonSounds(feature, entry.item)
                }
            }
        }
    }
}

/** Everything one person can be heard through. */
@Composable
private fun PersonSounds(feature: HostFeature, person: Person) {
    val name = person.name.ifEmpty { person.id }
    // `isMicrophoneEnabled`: there and not muted. Said rather than drawn as a
    // state of the slider, which is about what the listener chose.
    val microphoneOn = person.track(MediaSource.MICROPHONE)?.let { !it.muted } ?: false
    Column(
        Modifier
            .fillMaxWidth()
            .panelCard()
            .padding(start = 12.dp, end = 12.dp, top = 10.dp, bottom = 8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(9.dp), verticalAlignment = Alignment.CenterVertically) {
            Monogram(name, person.id, size = 26.dp)
            KitText(name, Modifier.weight(1f, fill = false), font = Font(13f, Font.Semibold), lines = 1)
            AnimatedVisibility(
                !microphoneOn,
                enter = fadeIn(Theme.settle()) + scaleIn(Theme.settle(), initialScale = 0.9f),
                exit = fadeOut(Theme.leave()) + scaleOut(Theme.leave(), targetScale = 0.9f),
            ) {
                KitText(
                    t("microphone off"),
                    Modifier
                        .background(Theme.wash.copy(alpha = 0.07f), Capsule)
                        .padding(horizontal = 7.dp, vertical = 2.dp),
                    font = Font(10.5f, Font.Medium), color = Theme.muted, lines = 1,
                )
            }
            Spacer(Modifier.weight(1f))
        }
        SoundRow(feature, person, Sound.Voice, name)
        // A screen's row only while it is shared with sound: a share whose
        // sound was never granted has nothing to turn down.
        AnimatedVisibility(person.track(MediaSource.SCREEN_SHARE_AUDIO) != null,
            enter = Theme.arrive + expandVertically(Theme.settle()), exit = Theme.depart + shrinkVertically(Theme.leave())) {
            SoundRow(feature, person, Sound.Screen, t("{name} (screen)", mapOf("name" to name)), symbol = "rectangle.on.rectangle")
        }
        // And the song desk's music, a track of its own beside a screen's
        // sound, turned down on its own.
        AnimatedVisibility(person.musicTrack != null,
            enter = Theme.arrive + expandVertically(Theme.settle()), exit = Theme.depart + shrinkVertically(Theme.leave())) {
            SoundRow(feature, person, Sound.Music, t("{name} (music)", mapOf("name" to name)), symbol = "music.note")
        }
    }
}

/**
 * One sound's mute and volume (SoundPanel.tsx `SoundRow`). Drawn with its
 * name above the slider when it is not the voice, which the card's own
 * heading already names.
 */
@Composable
private fun SoundRow(feature: HostFeature, person: Person, sound: Sound, name: String, symbol: String? = null) {
    val setting = feature.setting(person, sound)
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        if (symbol != null) {
            val ink = if (setting.silenced) Theme.muted.copy(alpha = Theme.muted.alpha * 0.7f) else Theme.muted
            Row(Modifier.padding(start = 2.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                Symbol(symbol, size = 10f, weight = Font.Semibold, tint = ink)
                KitText(name, font = Font(11.5f, Font.Medium), color = ink, lines = 1)
            }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
            // Named for what pressing it does, not for the state it is in:
            // the two read as opposites, and only one is being decided.
            val press = rememberPress()
            val label = if (setting.blocked) t("Unmute {name}", mapOf("name" to name)) else t("Mute {name}", mapOf("name" to name))
            val fill by animateColorAsState(Theme.wash.copy(alpha = if (setting.blocked) 0.16f else 0.06f), Theme.settle(), label = "fill")
            // Pressed over a fingertip on a phone and the cover screen, beside
            // a slider that would otherwise take the presses meant for it.
            Box(
                Modifier
                    .size(fingertip(28.dp))
                    .pressable(press, CircleShape, true, Role.Button, label, selected = if (setting.blocked) true else null) {
                        feature.setBlocked(person, sound, !setting.blocked)
                    },
                contentAlignment = Alignment.Center,
            ) {
                Box(
                    Modifier
                        .size(28.dp)
                        .background(fill, CircleShape)
                        .border(1.dp, fill, CircleShape),
                    contentAlignment = Alignment.Center,
                ) {
                    SwappingSymbol(
                        if (setting.blocked) "speaker.slash.fill" else "speaker.wave.2.fill",
                        size = 11.5f, weight = Font.Semibold,
                        tint = if (setting.blocked) Theme.foreground else Theme.muted,
                    )
                }
            }
            // Disabled rather than hidden while blocked: nothing is arriving
            // to be made louder, and a slider that moves while the sound
            // stays off is a control that lies.
            GlassSlider(
                setting.volume,
                { feature.setVolume(person, sound, it) },
                t("{name}'s volume", mapOf("name" to name)),
                Modifier.weight(1f),
                enabled = !setting.blocked,
            )
            // Read at a glance, because a slider near one end tells nobody
            // whether it is at nought.
            AnimatedContent(
                if (setting.blocked) t("Muted") else "${(setting.volume * 100).roundToInt()}%",
                Modifier.widthIn(min = 42.dp),
                transitionSpec = { fadeIn(Theme.settle()) togetherWith fadeOut(Theme.leave()) },
                contentAlignment = Alignment.CenterEnd,
                label = "volume",
            ) { said ->
                KitText(said, font = Font(11f, Font.Medium, digits = true), color = Theme.muted, lines = 1, align = TextAlign.End)
            }
        }
    }
}
