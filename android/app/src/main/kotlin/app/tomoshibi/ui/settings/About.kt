package app.tomoshibi.ui.settings

import androidx.compose.animation.core.animateFloatAsState
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
                Modifier.dropShadow(rounded(19f), Shadow(radius = 14.dp, color = Color.Black, offset = DpOffset(0.dp, 6.dp), alpha = 0.4f)),
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

@Composable
private fun SourceRow(source: String) {
    val uri = LocalUriHandler.current
    val press = rememberPress()
    val light by animateFloatAsState(if (press.lit) 0.06f else 0f, Motion.hover(), label = "light")
    Row(
        Modifier
            .fillMaxWidth()
            .background(Color.White.copy(alpha = light))
            .pressable(press, RectangleShape, true, Role.Button) { runCatching { uri.openUri(source) } }
            .heightIn(min = 48.dp)
            .padding(horizontal = 14.dp, vertical = 9.dp)
            .testTag("settings.source"),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(26.dp).background(Color.White.copy(alpha = 0.10f), rounded(7f)), contentAlignment = Alignment.Center) {
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
