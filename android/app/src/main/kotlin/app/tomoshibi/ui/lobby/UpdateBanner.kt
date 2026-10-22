package app.tomoshibi.ui.lobby

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.ui.draw.dropShadow
import androidx.compose.ui.graphics.shadow.Shadow
import androidx.compose.ui.unit.DpOffset
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Localization
import app.tomoshibi.basics.Pace
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.readingLocale
import app.tomoshibi.basics.t
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.Updates
import app.tomoshibi.model.Updates.Step
import app.tomoshibi.ui.kit.ContinuousShape
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.MediaSpinner
import app.tomoshibi.ui.kit.PillButton
import app.tomoshibi.ui.kit.PillRole
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.glassFade
import app.tomoshibi.ui.kit.glassLayer

/**
 * A newer build, said at the top of the lobby without stopping anything
 * (UpdateBanner.swift): what it is and what changed, "Update" and "Later";
 * then how far the download has got, with a way to stop it; then, on
 * Android, why the app is about to ask to be allowed to install apps, before
 * the system's page is opened; then the system's own confirmation.
 *
 * Drawn under the sheets and the settings, which cover it as they cover the
 * lobby, and only while the lobby is the screen (Updates `shown`), so that
 * no call is ever interrupted by it. Where it floats it slides down from
 * above and fades, as the notices at the foot slide up; what is
 * said inside changes with a cross-fade and a change of height, never a cut.
 * `top` is what it clears below the top of where it is put: the sign-in's
 * language menu, the wide lobby's server menu. `inline` puts it in a column
 * instead, where it opens a space for itself (the phone lobby).
 */
@Composable
fun UpdateBanner(app: AppModel, modifier: Modifier = Modifier, top: Dp = 12.dp, inline: Boolean = false) {
    val updates = app.updates ?: return
    val shown = updates.shown
    // What it last said, kept while it leaves.
    var said by remember { mutableStateOf(updates.step) }
    if (shown) said = updates.step
    if (inline) {
        // In the phone lobby's own column, under its large title: it opens
        // the column up and closes it again rather than lying over the title.
        AnimatedVisibility(
            shown,
            modifier,
            enter = expandVertically(Theme.settle(), expandFrom = Alignment.Top) + fadeIn(Theme.settle()),
            exit = shrinkVertically(Theme.leave(), shrinkTowards = Alignment.Top) + fadeOut(Theme.leave()),
        ) { Card(updates, said, Modifier.fillMaxWidth(), floating = false) }
        return
    }
    val above = with(LocalDensity.current) { top.roundToPx() }
    Box(modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        AnimatedVisibility(
            shown,
            enter = slideInVertically(Theme.settle()) { -(it + above) },
            exit = slideOutVertically(Theme.leave()) { -(it + above) },
        ) {
            val fade by glassFade()
            Card(updates, said, Modifier.padding(start = 16.dp, end = 16.dp, top = top).widthIn(max = 520.dp).fillMaxWidth().glassLayer({ fade }), floating = true)
        }
    }
}

/**
 * The card, flat as the screens are drawn: the surface colour with the
 * theme's edge, and a shadow only where it floats over something, which in
 * the phone's column it does not. Not glass: it is content the person reads,
 * not a control laid over a picture.
 */
@Composable
private fun Card(updates: Updates, said: Step, modifier: Modifier, floating: Boolean) {
    val shape = ContinuousShape(20.dp)
    Box(
        modifier
            .then(if (floating) Modifier.dropShadow(shape, Shadow(radius = 18.dp, color = Color.Black, offset = DpOffset(0.dp, 8.dp), alpha = 0.35f)) else Modifier)
            .background(Theme.surface, shape)
            .border(1.dp, Theme.border, shape)
            .padding(14.dp)
            .testTag("update.banner"),
    ) {
        val pace = updates.downloadBar
        AnimatedContent(
            said,
            contentKey = { it::class },
            transitionSpec = { (fadeIn(Theme.settle()) togetherWith fadeOut(Theme.leave())).using(SizeTransform(clip = false)) },
            label = "update",
        ) { step -> Body(updates, step, pace) }
    }
}

@Composable
private fun Body(updates: Updates, step: Step, pace: Pace) {
    when (step) {
        Step.None -> {}
        is Step.Offered -> Said(
            badge = if (step.problem != null) "exclamationmark.circle.fill" else "sparkles",
            warning = step.problem != null,
            title = t("Tomoshibi {version} is available", mapOf("version" to step.offer.label)),
            lines = listOfNotNull(step.offer.notes(Localization.shared.language.code)),
            problem = step.problem,
        ) {
            PillButton(t("Later"), updates::later, Modifier.testTag("update.later"), height = 30.dp)
            PillButton(
                if (step.problem != null) t("Try again") else t("Update"),
                updates::accept,
                Modifier.testTag("update.accept"),
                role = PillRole.Prominent,
                height = 30.dp,
            )
        }
        is Step.Downloading -> Downloading(updates, step.offer.label, step.received, step.total, pace)
        is Step.Held -> Downloading(updates, step.offer.label, step.received, step.total, pace)
        is Step.Permission -> if (step.refused) {
            Said(
                badge = "exclamationmark.circle.fill",
                warning = true,
                title = t("The update was not installed."),
                lines = listOf(t("Tomoshibi is not allowed to install apps, so it could not install the update. Allow it on the next screen to try again.")),
            ) {
                PillButton(t("Later"), updates::later, Modifier.testTag("update.later"), height = 30.dp)
                PillButton(t("Allow"), updates::allow, Modifier.testTag("update.allow"), role = PillRole.Prominent, height = 30.dp)
            }
        } else {
            Said(
                badge = "lock",
                title = t("Allow Tomoshibi to install its update"),
                lines = listOf(t("Android asks you to allow this once. On the next screen, turn on “Allow from this source”, then come back to Tomoshibi.")),
            ) {
                PillButton(t("Not now"), updates::later, Modifier.testTag("update.later"), height = 30.dp)
                PillButton(t("Continue"), updates::allow, Modifier.testTag("update.allow"), role = PillRole.Prominent, height = 30.dp)
            }
        }
        is Step.Installing -> Said(
            badge = null,
            title = t("Installing Tomoshibi {version}", mapOf("version" to step.offer.label)),
            lines = listOf(t("Confirm the update in the window Android shows.")),
        ) {
            PillButton(t("Cancel"), updates::stopInstalling, Modifier.testTag("update.cancel"), height = 30.dp)
        }
    }
}

@Composable
private fun Downloading(updates: Updates, label: String, received: Long, total: Long, pace: Pace) {
    Said(
        badge = "arrow.clockwise",
        title = t("Downloading Tomoshibi {version}", mapOf("version" to label)),
        lines = listOf(t("{received} of {total}", mapOf("received" to megabytes(pacedReceived(received, total, pace)), "total" to megabytes(total)))),
        above = { ProgressTrack(pace, fraction(received, total)) },
    ) {
        PillButton(t("Cancel"), updates::cancel, Modifier.testTag("update.cancel"), height = 30.dp)
    }
}

/** The symbol in a lit circle, the title, what there is to say, and the buttons at the trailing end. */
@Composable
private fun Said(
    badge: String?,
    title: String,
    lines: List<String>,
    warning: Boolean = false,
    problem: String? = null,
    above: (@Composable () -> Unit)? = null,
    buttons: @Composable () -> Unit,
) {
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.Top) {
        val tint = if (warning) Theme.danger else Theme.tally
        Box(
            Modifier.size(30.dp).background(tint.copy(alpha = 0.18f), CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            if (badge != null) Symbol(badge, size = 13f, weight = Font.Semibold, tint = tint) else MediaSpinner(size = 14f, tint = tint)
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            KitText(title, font = Font(14f, Font.Semibold))
            for (line in lines) KitText(line, font = Font(12.5f), color = Theme.muted, lines = 6)
            if (problem != null) KitText(problem, font = Font(12.5f), color = Theme.danger)
            above?.invoke()
            Row(
                Modifier.fillMaxWidth().padding(top = 6.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End),
                verticalAlignment = Alignment.CenterVertically,
            ) { buttons() }
        }
    }
}

/**
 * How much has arrived, as a filling capsule drawn where the model's bar is
 * (Updates `downloadBar`), which already moves at one speed: an animation to
 * each value as well would ease every frame's step. TalkBack is told
 * [fraction], what was reported, which it reads out now and then rather than
 * watching fill.
 */
@Composable
private fun ProgressTrack(pace: Pace, fraction: Float) {
    Box(
        Modifier
            .padding(top = 4.dp)
            .fillMaxWidth()
            .height(5.dp)
            .clip(CircleShape)
            .background(Theme.surfaceHigh)
            .semantics { progressBarRangeInfo = ProgressBarRangeInfo(fraction, 0f..1f) }
            .testTag("update.progress"),
    ) {
        Box(Modifier.fillMaxWidth(pace.value).height(5.dp).background(Theme.tally, CircleShape).testTag("update.progress.fill"))
    }
}

/** How much of [total] has arrived, from nought to one. */
private fun fraction(received: Long, total: Long): Float = if (total > 0) (received.toFloat() / total).coerceIn(0f, 1f) else 0f

/**
 * The bytes that have arrived as the bar has them, so that the figures beside
 * it count up with it rather than run ahead; what was reported, exactly,
 * once the bar has caught up.
 */
internal fun pacedReceived(received: Long, total: Long, pace: Pace): Long =
    if (pace.caughtUp || total <= 0) received else (pace.value.toDouble() * total).toLong().coerceIn(0L, received)

/** Bytes as megabytes to a tenth, in the reader's own way of writing a number. */
internal fun megabytes(bytes: Long): String = String.format(readingLocale(), "%.1f MB", bytes / 1_000_000.0)
