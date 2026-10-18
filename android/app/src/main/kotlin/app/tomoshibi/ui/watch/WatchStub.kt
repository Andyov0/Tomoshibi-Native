package app.tomoshibi.ui.watch

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
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
 * Stand-ins for the watching-together feature, built in another worktree, so
 * that the room compiles and can be looked at on its own. This file is
 * deleted when the two are merged; the signatures are the ones agreed.
 */

@Composable
fun WatchPanel(app: AppModel, call: CallModel, close: () -> Unit, modifier: Modifier = Modifier) {
    GlassShell(modifier.fillMaxWidth(), cornerRadius = 22.dp) {
        Column {
            PanelHeading(t("Watch together"), t("Close"), close, symbol = "play.rectangle.fill")
            PanelRule()
            KitText(t("Watch together"), Modifier.padding(16.dp), font = Font(12f), color = Theme.muted)
        }
    }
}

@Composable
fun WatchStage(app: AppModel, call: CallModel, modifier: Modifier = Modifier) {
    Box(modifier.fillMaxSize().background(Color.Black))
}

/** Whether a shared video is on the stage, where it takes the stage as a shared screen does. */
fun watchShowing(call: CallModel): Boolean = false
