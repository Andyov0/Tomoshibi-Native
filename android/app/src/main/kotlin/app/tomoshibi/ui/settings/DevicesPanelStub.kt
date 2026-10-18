package app.tomoshibi.ui.settings

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.CallModel
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.GlassShell
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.PanelHeading
import app.tomoshibi.ui.kit.PanelRule

/*
 * A stand-in for the settings package's DevicesPanel, built in another
 * worktree, so that the room compiles and can be looked at on its own. This
 * file is deleted when the two are merged; the signature is the one agreed.
 */

@Composable
fun DevicesPanel(app: AppModel, call: CallModel, close: () -> Unit, modifier: Modifier = Modifier) {
    GlassShell(modifier.fillMaxWidth(), cornerRadius = 22.dp) {
        Column {
            PanelHeading(t("Devices"), t("Close"), close, symbol = "gearshape.fill")
            PanelRule()
            KitText(t("Devices"), Modifier.padding(16.dp), font = Font(12f), color = Theme.muted)
        }
    }
}
