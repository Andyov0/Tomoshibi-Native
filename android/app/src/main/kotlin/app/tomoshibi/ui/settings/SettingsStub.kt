package app.tomoshibi.ui.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.model.AppModel
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.GlassButton
import app.tomoshibi.ui.kit.KitText

/**
 * A stand-in for the settings, so that the screens that open them build on
 * their own; replaced at merge by the real SettingsView (a full page on a
 * phone, a sidebar and its pages on a wide screen), with this signature.
 * This file is deleted at merge.
 */
@Composable
fun SettingsView(app: AppModel, close: () -> Unit, modifier: Modifier = Modifier) {
    Box(modifier.fillMaxSize().background(Theme.background), contentAlignment = Alignment.Center) {
        Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            KitText("SettingsView", font = Font(20f, Font.Bold))
            KitText(app.account?.name ?: "", font = Font(13f), color = Theme.muted)
            GlassButton(close) { KitText(t("Close")) }
        }
    }
}
