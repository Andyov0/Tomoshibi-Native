package app.tomoshibi.ui.settings

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.dropShadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.shadow.Shadow
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.Updates
import app.tomoshibi.model.Updates.Said
import app.tomoshibi.model.Updates.Step
import app.tomoshibi.ui.kit.Hairline
import app.tomoshibi.ui.kit.PillButton
import app.tomoshibi.ui.kit.PillRole
import app.tomoshibi.ui.lobby.megabytes
import app.tomoshibi.ui.lobby.pacedReceived
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.Motion
import app.tomoshibi.ui.kit.SettingsRow
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.pressable
import app.tomoshibi.ui.kit.rememberPress
import app.tomoshibi.ui.kit.rounded

/** The About page (Settings.swift `AboutSettings`): the app, its core, the server's build, and where its code can be read. */
@Composable
internal fun AboutSettings(app: AppModel) {
    val platform = platform()
    val version = platform.appVersion
    val core = remember(platform) { platform.coreVersion() }
    val info = app.info
    Column(verticalArrangement = Arrangement.spacedBy(22.dp)) {
        Row(
            Modifier.padding(vertical = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(18.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Mark(
                Modifier.dropShadow(rounded(19f), Shadow(radius = 14.dp, color = Color.Black, offset = DpOffset(0.dp, 6.dp), alpha = Theme.shade(0.4f))),
                size = 76.dp,
            )
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                KitText("Tomoshibi", font = Font(26f, Font.Bold), lines = 1)
                KitText(version, font = Font(13f, mono = true), color = Theme.muted, lines = 1)
            }
        }
        Section(t("About")) {
            SettingsRow(t("Version"), symbol = "app.badge") {
                KitText(version, font = Font(12.5f, mono = true), color = Theme.muted, lines = 1)
            }
            // Only where the app looks for updates at all: not in a debug
            // build started by hand, which would ask the real deployment,
            // and not while the deployment has them switched off, unless one
            // is already on its way (Updates `speaks`).
            app.updates?.let { updates ->
                AnimatedVisibility(updates.speaks, enter = Theme.arrive + expandVertically(Theme.settle()), exit = Theme.depart + shrinkVertically(Theme.leave())) {
                    UpdateRow(updates)
                }
            }
            SettingsRow(t("Media core"), symbol = "cpu", last = info == null) {
                KitText(core, font = Font(12.5f, mono = true), color = Theme.muted, lines = 1)
            }
            if (info != null) {
                // The server's build and not its address: which server this
                // is, is not something the app says anywhere.
                SettingsRow(t("Server"), symbol = "server.rack", last = info.source.isEmpty()) {
                    KitText(info.build.ifEmpty { "—" }.take(12), font = Font(12.5f, mono = true), color = Theme.muted, lines = 1)
                }
                // Where the code running there can be read: an obligation
                // under the AGPL's thirteenth section, which only a page
                // somebody uses can discharge, and from the server because a
                // deployment running a changed copy owes that copy. Not
                // translated, as on the web: it is the licence's name.
                if (info.source.isNotEmpty()) SourceRow(info.source)
            }
        }
    }
}

/**
 * Where the app's updates stand, and the check somebody can ask for
 * (Settings.swift `UpdateRow`): the version on offer with "Update", a
 * download's progress with "Cancel", and the answer to a check, which says
 * so even when there is nothing newer. What it says changes with a fade.
 *
 * A download's figures count up with the model's bar (Updates
 * `downloadBar`), the one the lobby's banner under this page draws, so that
 * the two never say different things.
 */
@Composable
private fun UpdateRow(updates: Updates) {
    val step = updates.step
    val said = updates.said
    val line: String? = when {
        step is Step.Downloading -> t("{received} of {total}", mapOf("received" to megabytes(pacedReceived(step.received, step.total, updates.downloadBar)), "total" to megabytes(step.total)))
        step is Step.Held -> t("{received} of {total}", mapOf("received" to megabytes(pacedReceived(step.received, step.total, updates.downloadBar)), "total" to megabytes(step.total)))
        step is Step.Installing -> t("Installing Tomoshibi {version}", mapOf("version" to step.offer.label))
        step is Step.Permission -> t("Allow Tomoshibi to install its update")
        step is Step.Offered -> step.problem ?: t("Version {version} is available.", mapOf("version" to step.offer.label))
        said == Said.Checking -> t("Checking for updates…")
        said == Said.Current -> t("You have the latest version.")
        said == Said.Failed -> t("Could not check for updates. Try again later.")
        else -> null
    }
    // A download's figures are one line whose numbers change, faded in once.
    // Keyed by its words, each report, and each frame of the pace, would be a
    // new line faded in over the one before, ten to a hundred times a second.
    val about: Any? = if (step is Step.Downloading || step is Step.Held) Step.Downloading::class else line
    Column(Modifier.fillMaxWidth().testTag("settings.updates")) {
        Row(
            Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(horizontal = 14.dp, vertical = 9.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.size(26.dp).background(Theme.wash.copy(alpha = 0.10f), rounded(7f)), contentAlignment = Alignment.Center) {
                Symbol("arrow.clockwise", size = 12.5f, weight = Font.Semibold)
            }
            Column(Modifier.weight(1f).animateContentSize(Theme.settle()), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                KitText(t("Updates"), font = Font(13.5f))
                AnimatedContent(
                    line to about,
                    contentKey = { it.second },
                    transitionSpec = { fadeIn(Theme.settle()) togetherWith fadeOut(Theme.leave()) },
                    label = "said",
                ) { (now, _) ->
                    if (now != null) {
                        KitText(now, font = Font(11.5f), color = if (step is Step.Offered && step.problem != null) Theme.danger else Theme.muted)
                    }
                }
            }
            Spacer(Modifier.width(10.dp))
            AnimatedContent(
                step::class,
                transitionSpec = { fadeIn(Theme.settle()) togetherWith fadeOut(Theme.leave()) },
                label = "action",
            ) { kind ->
                when (kind) {
                    Step.Offered::class -> PillButton(t("Update"), updates::accept, Modifier.testTag("settings.update"), role = PillRole.Prominent)
                    Step.Downloading::class -> PillButton(t("Cancel"), updates::cancel, Modifier.testTag("settings.cancelUpdate"))
                    else -> PillButton(
                        t("Check for updates"),
                        updates::checkNow,
                        Modifier.testTag("settings.checkUpdates"),
                        busy = said == Said.Checking,
                        enabled = step is Step.None && said != Said.Checking,
                    )
                }
            }
        }
        Hairline(52.dp)
    }
}

@Composable
private fun SourceRow(source: String) {
    val uri = LocalUriHandler.current
    val press = rememberPress()
    val light by animateFloatAsState(if (press.lit) 0.06f else 0f, Motion.hover(), label = "light")
    Row(
        Modifier
            .fillMaxWidth()
            .background(Theme.wash.copy(alpha = light))
            .pressable(press, RectangleShape, true, Role.Button) { runCatching { uri.openUri(source) } }
            .heightIn(min = 48.dp)
            .padding(horizontal = 14.dp, vertical = 9.dp)
            .testTag("settings.source"),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(26.dp).background(Theme.wash.copy(alpha = 0.10f), rounded(7f)), contentAlignment = Alignment.Center) {
            Symbol("chevron.left.forwardslash.chevron.right", size = 12.5f, weight = Font.Semibold)
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            KitText("AGPL-3.0 · source", font = Font(13.5f), lines = 1)
            KitText(source, font = Font(11.5f), color = Theme.muted, lines = 1)
        }
        Spacer(Modifier.width(10.dp))
        Symbol("arrow.up.right", size = 11.5f, weight = Font.Bold, tint = Theme.muted)
    }
}
