package app.tomoshibi.ui.room

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.expandHorizontally
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkHorizontally
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.media.ShareRates
import app.tomoshibi.media.Sharing
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.CallModel
import app.tomoshibi.model.CallToast
import app.tomoshibi.ui.call.Half
import app.tomoshibi.ui.call.HandControl
import app.tomoshibi.ui.call.TRAY
import app.tomoshibi.ui.kit.BarAxis
import app.tomoshibi.ui.kit.BarButton
import app.tomoshibi.ui.kit.BarPopover
import app.tomoshibi.ui.kit.BarPopoverPlace
import app.tomoshibi.ui.kit.Capsule
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.GlassBacking
import app.tomoshibi.ui.kit.KitEnabled
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.MenuCard
import app.tomoshibi.ui.kit.MenuHeading
import app.tomoshibi.ui.kit.MenuItem
import app.tomoshibi.ui.kit.MenuRole
import app.tomoshibi.ui.kit.MenuRule
import app.tomoshibi.ui.kit.MenuSwitch
import app.tomoshibi.ui.kit.Motion
import app.tomoshibi.ui.kit.Popovers
import app.tomoshibi.ui.kit.SegmentedChoice
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.barPress
import app.tomoshibi.ui.kit.holdsPresses
import app.tomoshibi.ui.kit.pressable
import app.tomoshibi.ui.kit.rememberPress
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.tomoshibi_protocol.ShareQuality

/** The panels a call opens beside it (RoomView.swift `Panel`). */
enum class RoomPanel { Messages, Sound, Watch, Music, Host, Devices }

/**
 * What the features layered over the call say about their buttons, read in
 * the room view so the bar does not hold the features themselves
 * (ControlBar.swift `Offers`).
 */
data class Offers(
    /**
     * The screen in the share's menu: unless the deployment has switched
     * screen sharing off (Remote), and then still while a share is running,
     * which is not stopped by it and needs its Stop.
     */
    val screenShare: Boolean = true,
    /** Room.tsx ~295: to whoever can start a show, to everybody once there is one. */
    val watch: Boolean = false,
    val watchPlaying: Boolean = false,
    val music: Boolean = false,
    val musicPlaying: Boolean = false,
    /** HostPanel.tsx ~387: only for whoever runs the room. */
    val host: Boolean = false,
    /** People at the door, counted on the button. */
    val knocking: Int = 0,
)

/**
 * What the watching-together and song-desk features say about their buttons
 * on the bar, kept in the call's parts under [WATCH] and [MUSIC] by whichever
 * of them is running (WatchFeature.swift `offered`/`playing`, MusicFeature
 * `available`/`playing`). Read through the parts rather than called, so that
 * the bar needs neither feature to be built to be drawn. Until a feature puts
 * one there, the room offers what it can tell by itself: the watch button
 * while a show is on the stage, the music button while somebody sends music.
 */
interface BarOffer {
    val offered: Boolean
    val playing: Boolean

    companion object {
        const val WATCH = "offer.watch"
        const val MUSIC = "offer.music"
    }
}

/** The bar's popovers, by the names the Swift bar gives them. */
internal const val SHARE = "share.screen"
internal const val MORE = "more"
internal const val LEAVE = "leave"

/** Something pressed on the bar that did not take: said briefly, and gone (ControlBar.swift `act`). */
internal fun CallModel.pressFailed() {
    announce(CallToast(t("That could not be done. Try again."), kind = CallToast.Kind.Failure, lasts = 4.0))
}

/**
 * The island of controls at the foot of the call
 * (components/room/ControlBar.tsx; ControlBar.swift): microphone, camera,
 * the share, hand and reactions, messages, the panels, leave. Forty-four
 * point buttons on a raised surface, the active ones in the one signal
 * colour, leaving in red and nothing else in red.
 *
 * The whole bar where it fits in [room], the length it is given along
 * [axis], and where it does not (a phone held upright) the same bar with the
 * panels behind one button: on a phone the whole bar was wider than the
 * screen and both ends hung off it, hanging up with them. Which one is worked
 * out from the buttons' own sizes before either is drawn, because drawing
 * both to measure them (SwiftUI's ViewThatFits) would open each popover twice.
 */
@Composable
internal fun ControlBar(
    app: AppModel,
    call: CallModel,
    panel: RoomPanel?,
    onPanel: (RoomPanel?) -> Unit,
    offers: Offers,
    popovers: Popovers,
    axis: BarAxis,
    room: Dp,
    modifier: Modifier = Modifier,
) {
    val compact = BarLength.of(compact = false, axis, offers, panel, call.sealed) > room.value - 20
    // The narrow bar's buttons as far apart as the bar has room for, up to
    // eight points, which puts a fingertip's width between the middles of
    // two forty-point buttons: four apart, a press between two went to
    // whichever was nearer, and each was pressed over 44 points. Where even
    // that leaves no room for the reactions' narrow half (the Fold's cover
    // screen), the half goes behind More and the hand's long press.
    val horizontal = axis == BarAxis.Horizontal
    val beside = !compact || !horizontal || BarLength.of(true, axis, offers, panel, call.sealed, spacing = 8f) <= room.value - 20
    val gap = if (!compact) 6f else {
        val bare = BarLength.of(true, axis, offers, panel, call.sealed, spacing = 0f, reactionsBeside = beside)
        ((room.value - 20 - bare) / (BarLength.count(true, axis, offers, panel, call.sealed) - 1)).coerceIn(4f, 8f)
    }
    // A microphone or camera press still being carried out (ControlBar.tsx
    // `guard`): another toggle press meanwhile is ignored. Only ignored: the
    // bar used to be disabled for it, Leave included, so somebody could not
    // leave while the system's permission prompt was open.
    var busy by remember { mutableStateOf(false) }
    // The hand's own press, so that a pending microphone does not hold up a
    // raised hand, nor the hand a microphone.
    var raising by remember { mutableStateOf(false) }
    fun device(work: suspend () -> Unit) {
        if (busy) return
        busy = true
        // On the call's own scope: a device asked for just before the room
        // is drawn again must still be answered.
        call.scope.launch {
            try {
                work()
            } catch (fault: CancellationException) {
                throw fault
            } catch (_: Exception) {
                // A device that would not start says so itself (CallModel
                // `deviceFailed`); the bar only waited for it.
            } finally {
                busy = false
            }
        }
    }
    fun act(work: suspend () -> Unit) {
        if (raising) return
        raising = true
        call.scope.launch {
            try {
                work()
            } catch (fault: CancellationException) {
                throw fault
            } catch (_: Exception) {
                call.pressFailed()
            } finally {
                raising = false
            }
        }
    }
    val size = if (compact) 40.dp else 44.dp
    val spacing = Arrangement.spacedBy(gap.dp)
    val items: @Composable () -> Unit = {
        BarButton(
            if (call.microphoneOn) "mic.fill" else "mic.slash.fill",
            if (call.microphoneOn) t("Mute microphone") else t("Unmute microphone"),
            { device { call.setMicrophone(!call.microphoneOn) } },
            off = !call.microphoneOn, size = size,
        )
        BarButton(
            if (call.cameraOn) "video.fill" else "video.slash.fill",
            if (call.cameraOn) t("Turn camera off") else t("Turn camera on"),
            { device { call.setCamera(!call.cameraOn) } },
            off = !call.cameraOn, size = size,
        )
        ShareControl(call, offers.screenShare, popovers, axis, size)
        // The hand and, behind the narrow half beside it, the reactions
        // (HandButton.tsx).
        HandControl(call, popovers, axis, beside = beside, size = size) { work -> act(work) }
        // The one dot that says something was said while the messages were
        // closed, in the signal colour (ControlBar.tsx ~383). Open, the
        // button is held down rather than lit, as every panel's is (BarButton).
        BarButton(
            "bubble.left.fill",
            if (panel == RoomPanel.Messages) t("Hide messages") else t("Show messages"),
            { onPanel(if (panel == RoomPanel.Messages) null else RoomPanel.Messages) },
            pressed = panel == RoomPanel.Messages, dot = call.unread && panel != RoomPanel.Messages, size = size,
        )
        if (compact) {
            MoreControl(call, panel, onPanel, offers, popovers, axis, size, reactions = !beside)
        } else {
            Panels(call, panel, onPanel, offers, axis, size)
            Box(
                Modifier
                    .then(if (horizontal) Modifier.padding(horizontal = 2.dp) else Modifier.padding(vertical = 2.dp))
                    .size(if (horizontal) 1.dp else 26.dp, if (horizontal) 26.dp else 1.dp)
                    .background(Theme.border),
            )
        }
        LeaveControl(app, call, offers.host, popovers, axis, compact, size)
    }
    val inset = if (compact) 5.dp else 7.dp
    // Behind the buttons, not around them: the popovers hang outside the
    // capsule (Glass.swift `glassBacking`).
    // Holding every press on the capsule, so that one between two buttons
    // goes to the nearer of them rather than to the picture under the bar.
    GlassBacking(Capsule, modifier.holdsPresses()) {
        if (horizontal) {
            Row(Modifier.padding(inset), horizontalArrangement = spacing, verticalAlignment = Alignment.CenterVertically) { items() }
        } else {
            Column(Modifier.padding(inset), verticalArrangement = spacing, horizontalAlignment = Alignment.CenterHorizontally) { items() }
        }
    }
}

/**
 * How long the bar is along its axis, from its buttons' own sizes: what
 * SwiftUI's ViewThatFits measures by drawing it. The buttons the features
 * offer are counted only while they are offered, as they are drawn.
 */
internal object BarLength {
    fun of(
        compact: Boolean,
        axis: BarAxis,
        offers: Offers,
        panel: RoomPanel?,
        sealed: Boolean,
        spacing: Float = if (compact) 4f else 6f,
        reactionsBeside: Boolean = true,
    ): Float {
        val along = lengths(compact, axis, offers, panel, sealed, reactionsBeside)
        val inset = if (compact) 5f else 7f
        return 2 * inset + along.sum() + spacing * (along.size - 1)
    }

    /** How many things the bar holds along its length. */
    fun count(compact: Boolean, axis: BarAxis, offers: Offers, panel: RoomPanel?, sealed: Boolean): Int =
        lengths(compact, axis, offers, panel, sealed, true).size

    private fun lengths(compact: Boolean, axis: BarAxis, offers: Offers, panel: RoomPanel?, sealed: Boolean, reactionsBeside: Boolean): List<Float> {
        val size = if (compact) 40f else 44f
        val horizontal = axis == BarAxis.Horizontal
        val along = ArrayList<Float>()
        along += size // microphone
        along += size // camera
        along += size // share
        // The hand, and its reactions' half beside it along a row and under
        // it down a column (HandControl): the bar's button and thirty-two.
        along += if (reactionsBeside || !horizontal) size + Half.value else size
        along += size // messages
        if (compact) {
            along += size // more
        } else {
            along += size // sound
            if (!sealed && (offers.watch || panel == RoomPanel.Watch)) along += size
            if (offers.music || panel == RoomPanel.Music) along += size
            if (offers.host || panel == RoomPanel.Host) along += size
            along += size // devices
            along += 1f + 4f // the rule before leaving
        }
        along += if (horizontal) (if (compact) 52f else 56f) else (if (compact) 48f else 52f)
        return along
    }
}

/** A button the features offer, arriving and leaving along the bar as the bar grows and shrinks round it. */
@Composable
private fun Offered(visible: Boolean, axis: BarAxis, content: @Composable () -> Unit) {
    val enter: EnterTransition = Theme.arrive + if (axis == BarAxis.Horizontal) expandHorizontally(Theme.settle()) else expandVertically(Theme.settle())
    val exit: ExitTransition = Theme.depart + if (axis == BarAxis.Horizontal) shrinkHorizontally(Theme.leave()) else shrinkVertically(Theme.leave())
    AnimatedVisibility(visible, enter = enter, exit = exit) { content() }
}

/** The panels' own buttons, on a bar wide enough for them (ControlBar.swift `panels`). */
@Composable
private fun Panels(call: CallModel, panel: RoomPanel?, onPanel: (RoomPanel?) -> Unit, offers: Offers, axis: BarAxis, size: Dp) {
    fun flip(target: RoomPanel) = onPanel(if (panel == target) null else target)
    BarButton(
        "speaker.wave.2.fill",
        if (panel == RoomPanel.Sound) t("Hide sound") else t("Show sound"),
        { flip(RoomPanel.Sound) },
        pressed = panel == RoomPanel.Sound, size = size,
    )
    Offered(!call.sealed && (offers.watch || panel == RoomPanel.Watch), axis) {
        // The show is playing while its panel is closed (ControlBar.tsx ~430).
        BarButton(
            "play.rectangle.fill",
            if (panel == RoomPanel.Watch) t("Hide watch together") else t("Show watch together"),
            { flip(RoomPanel.Watch) },
            pressed = panel == RoomPanel.Watch, dot = offers.watchPlaying && panel != RoomPanel.Watch, size = size,
        )
    }
    Offered(offers.music || panel == RoomPanel.Music, axis) {
        BarButton(
            "music.note",
            if (panel == RoomPanel.Music) t("Hide music") else t("Show music"),
            { flip(RoomPanel.Music) },
            pressed = panel == RoomPanel.Music, dot = offers.musicPlaying && panel != RoomPanel.Music, size = size,
        )
    }
    Offered(offers.host || panel == RoomPanel.Host, axis) {
        Box {
            BarButton("crown.fill", t("Running this room"), { flip(RoomPanel.Host) }, pressed = panel == RoomPanel.Host, size = size)
            // Counted as well as coloured (HostPanel.tsx ~400): somebody
            // waiting outside is the one thing in the room that is lost if
            // nobody notices.
            AnimatedVisibility(
                offers.knocking > 0 && panel != RoomPanel.Host,
                Modifier.align(Alignment.TopEnd).offset(x = 2.dp, y = (-2).dp),
                enter = Theme.arrive, exit = Theme.depart,
            ) { Count(offers.knocking) }
        }
    }
    BarButton("gearshape.fill", t("Devices"), { flip(RoomPanel.Devices) }, pressed = panel == RoomPanel.Devices, size = size)
}

/** How many are at the door, in the signal colour ringed in the bar's grey. */
@Composable
private fun Count(count: Int) {
    KitText(
        count.toString(),
        Modifier
            .background(Theme.tally, Capsule)
            .border(2.dp, Theme.surfaceHigh, Capsule)
            .defaultMinSize(16.dp, 16.dp)
            .padding(horizontal = 4.dp, vertical = 1.dp),
        font = Font(10f, Font.Bold, digits = true),
        color = Theme.tallyForeground,
        lines = 1,
        align = TextAlign.Center,
    )
}

// Sharing

/**
 * The share's button and its popover (ShareButton.tsx; ControlBar.swift
 * `ScreenShareControl` and `SoundShareControl`, as one): the size and the
 * frame rate first, because they are what somebody came to change, then the
 * press that starts the share; while sharing, the same settings, applied to
 * the running share at once, and stopping. Below it, sound shared on its
 * own, which the web keeps in the same menu.
 *
 * One button rather than the Mac's two, as the iPhone has one: on a phone's
 * bar a second would push the bar into its narrow form on every phone. And a
 * popover where the iPhone opens the system's broadcast sheet, because
 * Android's consent to share a screen is a yes or no with none of the web's
 * choices on it.
 */
@Composable
private fun ShareControl(call: CallModel, screen: Boolean, popovers: Popovers, axis: BarAxis, size: Dp) {
    val sharing = call.sharing
    // With the screen switched off by the deployment the button is the
    // sound's alone, and says so, rather than offering a screen it no longer
    // has (the Mac keeps its sound's own button in the same case).
    val symbol = when {
        sharing.active -> "rectangle.inset.filled.on.rectangle"
        sharing.listening -> "waveform.circle.fill"
        !screen -> "waveform"
        else -> "rectangle.on.rectangle"
    }
    val label = when {
        sharing.active -> t("Screen sharing settings")
        sharing.listening -> t("Sharing sound")
        !screen -> t("Share only sound")
        else -> t("Share your screen")
    }
    BarPopover(SHARE, popovers, axis, width = 376.dp, panel = { SharePanel(call, screen, popovers) }) {
        BarButton(symbol, label, { popovers.toggle(SHARE) }, lit = sharing.active || sharing.listening,
            pressed = popovers.open == SHARE, size = size)
    }
}

@Composable
private fun SharePanel(call: CallModel, screen: Boolean, popovers: Popovers) {
    val sharing = call.sharing
    // The menu's choices are plain fields of Sharing, not state a screen can
    // watch: read again whenever a change has gone through.
    var changed by remember { mutableIntStateOf(0) }
    changed
    val shown = sharing.shown
    var lossless by remember { mutableStateOf(sharing.lossless) }
    // A setting changed mid-share that did not take: the share goes on as it
    // was, and says so. A share that would not start says the same; a
    // cancelled consent is an answer, not a failure, and says nothing
    // (ControlBar.tsx: it was once announced as a camera nobody could use).
    fun change(work: suspend () -> Unit) {
        call.scope.launch {
            try {
                work()
            } catch (fault: CancellationException) {
                throw fault
            } catch (_: Exception) {
                call.pressFailed()
            } finally {
                changed++
            }
        }
    }
    MenuCard {
        // The screen's half: gone while the deployment has screen sharing
        // switched off (Offers `screenShare`), and back when it is on again.
        AnimatedVisibility(screen, enter = Theme.arrive + expandVertically(Theme.settle()), exit = Theme.depart + shrinkVertically(Theme.leave())) {
            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                MenuHeading(t("Picture"))
                SegmentedChoice(
                    listOf(ShareQuality.AUTO to t("Automatic"), ShareQuality.P1080 to t("1080p"), ShareQuality.P1440 to t("1440p"), ShareQuality.P4K to t("4K")),
                    shown.quality,
                    { quality -> change { sharing.choose(quality) } },
                    Modifier.padding(horizontal = 4.dp),
                )
                AnimatedContent(
                    describes(shown.quality),
                    transitionSpec = { fadeIn(Theme.settle()) togetherWith fadeOut(Theme.leave()) },
                    label = "quality",
                ) { words ->
                    KitText(words, Modifier.padding(start = 10.dp, end = 10.dp, top = 4.dp), font = Font(11.5f), color = Theme.muted)
                }
                // Absent for automatic, which chooses the rate as well: a control
                // that did nothing would be a control somebody set and believed.
                AnimatedVisibility(shown.quality != ShareQuality.AUTO, enter = Theme.arrive + expandVertically(Theme.settle()),
                    exit = Theme.depart + shrinkVertically(Theme.leave())) {
                    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        MenuHeading(t("Frames a second"))
                        SegmentedChoice(
                            ShareRates.allowed(Sharing.shorterSide(shown.quality)).map { it to it.toString() },
                            shown.fps,
                            { rate -> change { sharing.choose(shown.quality, rate) } },
                            Modifier.padding(horizontal = 4.dp),
                        )
                    }
                }
                MenuRule()
                // Not disabled while busy: a change waits its turn and a stop always
                // goes through (Sharing).
                AnimatedContent(sharing.active, transitionSpec = { Theme.arrive togetherWith Theme.depart }, label = "screen") { active ->
                    if (active) {
                        MenuItem(t("Stop sharing"), {
                            popovers.close()
                            change { sharing.stopScreen() }
                        }, symbol = "stop.circle", busy = sharing.screenBusy)
                    } else {
                        MenuItem(t("Share your screen"), {
                            popovers.close()
                            change { sharing.start(call.call) }
                        }, symbol = "rectangle.on.rectangle",
                            detail = if (sharing.listening) t("Without its sound, while sound is shared on its own") else null,
                            role = MenuRole.Prominent, busy = sharing.screenBusy)
                    }
                }
                MenuRule()
            }
        }
        AnimatedContent(sharing.listening, transitionSpec = { Theme.arrive togetherWith Theme.depart }, label = "sound") { listening ->
            if (listening) {
                // Which of the two is going out, because the switch that
                // chose it is not shown while it is: it takes effect when the
                // sharing starts, and a setting shown mid-share would look as
                // if changing it changed something (ShareButton.tsx).
                MenuItem(t("Stop sharing sound"), {
                    popovers.close()
                    change { sharing.stopSound() }
                }, symbol = "stop.circle", detail = if (sharing.sendingLossless) t("Lossless") else t("Real time"), busy = sharing.soundBusy)
            } else {
                Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    MenuItem(t("Share only sound"), {
                        popovers.close()
                        change { sharing.startSoundOnly(call.call, call.sealed) }
                    }, symbol = "waveform",
                        detail = if (sharing.active && sharing.withSound) t("Your shared screen goes quiet while this plays")
                        else t("Pick the app that is playing"),
                        role = MenuRole.Prominent, busy = sharing.soundBusy)
                    // Beneath what it qualifies and decided before the
                    // consent is asked. Off where it cannot work, with the
                    // reason in place of the description, so nobody turns it
                    // on and gets real time without being told. Lossless is
                    // data, which the SDK does not seal and which would cross
                    // the relay as plain samples (sound.ts `losslessUnavailable`).
                    MenuSwitch(
                        t("Lossless"),
                        isOn = !call.sealed && lossless,
                        onChange = {
                            lossless = it
                            sharing.lossless = it
                        },
                        Modifier.alpha(if (call.sealed) 0.7f else 1f),
                        symbol = "hifispeaker",
                        detail = if (call.sealed) t("Not available in an encrypted call") else t("Every sample as it is played, about a second behind"),
                        enabled = !call.sealed,
                    )
                }
            }
        }
    }
}

/**
 * What somebody is choosing between rather than pixels (ShareButton.tsx
 * `QUALITY_LABELS`): automatic is the only setting that will quietly send
 * less, and every other one is a promise to send what was asked.
 */
private fun describes(quality: ShareQuality): String = when (quality) {
    ShareQuality.AUTO -> t("Adjusts to the connection")
    ShareQuality.P1080 -> t("Up to 240 frames a second")
    ShareQuality.P1440 -> t("Up to 120 frames a second")
    ShareQuality.P4K -> t("Up to 60 frames a second")
}

// Leaving

/**
 * Leaving, and for whoever runs the room ending it (Leaving.tsx;
 * ControlBar.swift `LeaveControl`): one popover above the red button rather
 * than a dialog in the middle of the screen, so the answer is where the
 * question was asked. Asked with the consequence rather than with "are you
 * sure", and closed by a press anywhere else or Back, because a question that
 * can only be dismissed by choosing something is one somebody chooses
 * something in by accident.
 */
@Composable
private fun LeaveControl(app: AppModel, call: CallModel, host: Boolean, popovers: Popovers, axis: BarAxis, compact: Boolean, size: Dp) {
    val open = popovers.open == LEAVE
    var ending by remember { mutableStateOf(false) }
    val press = rememberPress()
    val fill by animateFloatAsState(if (open || press.lit) 0.86f else 1f, Motion.hover(), label = "leave")
    val rim by animateColorAsState(Color.White.copy(alpha = if (open) 0.4f else 0.25f), Theme.settle(), label = "rim")
    val horizontal = axis == BarAxis.Horizontal
    fun end() {
        ending = true
        RoomWork.launch {
            try {
                endForEverybody(app, call)
                popovers.close()
            } catch (fault: CancellationException) {
                throw fault
            } catch (_: Exception) {
                ending = false
                call.pressFailed()
            }
        }
    }
    BarPopover(
        LEAVE, popovers, axis, place = BarPopoverPlace.End, width = 272.dp,
        panel = {
            KitEnabled(!ending) {
                MenuCard {
                    MenuItem(t("Leave the call"), {
                        popovers.close()
                        RoomWork.launch { app.leaveCall(call) }
                    }, symbol = "rectangle.portrait.and.arrow.right", detail = t("The meeting carries on without you."))
                    AnimatedVisibility(host, enter = Theme.arrive + expandVertically(Theme.settle()), exit = Theme.depart + shrinkVertically(Theme.leave())) {
                        MenuItem(t("End for everybody"), { end() }, symbol = "door.left.hand.closed",
                            detail = t("Everybody is disconnected and links stop working."),
                            role = MenuRole.Destructive, busy = ending)
                    }
                }
            }
        },
    ) {
        Box(
            Modifier
                .barPress(press)
                .size(
                    if (horizontal) (if (compact) 52.dp else 56.dp) else size,
                    if (horizontal) size else (if (compact) 48.dp else 52.dp),
                )
                .background(Theme.danger.copy(alpha = fill), Capsule)
                .border(1.dp, rim, Capsule)
                .pressable(press, Capsule, true, Role.Button, t("Leave"), selected = if (open) true else null) { popovers.toggle(LEAVE) },
            contentAlignment = Alignment.Center,
        ) { Symbol("phone.down.fill", size = 16f, weight = Font.Semibold, tint = Theme.dangerForeground) }
    }
}

/**
 * The same request the host panel's "Close the room" makes
 * (HostFeature.dissolve), made here so that a failure can be told apart from
 * a success: the panel's own call says a failure in its notices and returns
 * either way.
 */
internal suspend fun endForEverybody(app: AppModel, call: CallModel) {
    call.deployment.dissolve(call.room, call.call.token())
    // The media server disconnects everybody, this app among them, and that
    // is what should carry the host out, to the screen everybody else lands
    // on. Waited for rather than relied on (Leaving.tsx): if the disconnect
    // does not arrive, whoever ended the meeting is left sitting in a room
    // that no longer exists. Only while this is still the call showing, so
    // that the ending the disconnect gave ("the room was closed") is not
    // written over with "you left".
    delay(1200)
    app.leaveCall(call)
}

/** Leaves [call] the way the Leave menu does, if it is still the call the app is in. */
internal suspend fun AppModel.leaveCall(call: CallModel) {
    if (this.call === call) leave() else call.leave()
}

// More

/**
 * The panels, behind one button on a narrow bar (ControlBar.swift
 * `MoreControl`). It carries the signal dot whenever one of the buttons it
 * stands for would: a show or the music playing, somebody at the door.
 */
@Composable
private fun MoreControl(call: CallModel, panel: RoomPanel?, onPanel: (RoomPanel?) -> Unit, offers: Offers, popovers: Popovers, axis: BarAxis, size: Dp, reactions: Boolean = false) {
    val behind = listOf(RoomPanel.Sound, RoomPanel.Watch, RoomPanel.Music, RoomPanel.Host, RoomPanel.Devices)
    val open = panel in behind
    val busy = (offers.watchPlaying && panel != RoomPanel.Watch) || (offers.musicPlaying && panel != RoomPanel.Music) ||
        (offers.knocking > 0 && panel != RoomPanel.Host)
    @Composable
    fun item(title: String, symbol: String, target: RoomPanel, dot: Boolean = false) {
        Box {
            MenuItem(title, {
                popovers.close()
                onPanel(if (panel == target) null else target)
            }, symbol = symbol, checked = panel == target)
            AnimatedVisibility(
                dot && panel != target,
                Modifier.align(Alignment.CenterStart).offset(x = 24.dp, y = (-7).dp),
                enter = Theme.arrive, exit = Theme.depart,
            ) { Box(Modifier.size(6.dp).background(Theme.tally, CircleShape)) }
        }
    }
    BarPopover(
        MORE, popovers, axis, place = BarPopoverPlace.End, width = 240.dp,
        panel = {
            MenuCard {
                item(t("Sound"), "speaker.wave.2.fill", RoomPanel.Sound)
                if (!call.sealed && (offers.watch || panel == RoomPanel.Watch)) {
                    item(t("Watch together"), "play.rectangle.fill", RoomPanel.Watch, dot = offers.watchPlaying)
                }
                if (offers.music || panel == RoomPanel.Music) item(t("Music"), "music.note", RoomPanel.Music, dot = offers.musicPlaying)
                if (offers.host || panel == RoomPanel.Host) {
                    item(
                        if (offers.knocking > 0) "${t("Running this room")} (${offers.knocking})" else t("Running this room"),
                        "crown.fill", RoomPanel.Host, dot = offers.knocking > 0,
                    )
                }
                MenuRule()
                item(t("Devices"), "gearshape.fill", RoomPanel.Devices)
                // The reactions, where the bar had no room for their half
                // beside the hand: opened over the hand, as from the half.
                if (reactions) {
                    MenuRule()
                    MenuItem(t("Reactions"), { popovers.show(TRAY) }, symbol = "sparkles")
                }
            }
        },
    ) {
        // Held down while its menu or a panel behind it is open: neither is
        // something happening in the call, so neither is amber (BarButton).
        BarButton("ellipsis", t("More"), { popovers.toggle(MORE) }, pressed = open || popovers.open == MORE, dot = busy, size = size)
    }
}

/** The popovers that close when the bar steps aside or the room goes. */
internal val BarPopoverIds = setOf(SHARE, MORE, LEAVE, TRAY)
