package app.tomoshibi.ui.room

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.CallModel
import app.tomoshibi.ui.LocalAppScope
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.GlassButton
import app.tomoshibi.ui.kit.KitText
import kotlinx.coroutines.launch

/**
 * A stand-in for the call, so that the navigation builds and can be walked
 * through on its own; replaced at merge by the real RoomView, which fills the
 * screen from edge to edge and keeps its own insets, with this signature.
 * This file is deleted at merge.
 */
@Composable
fun RoomView(app: AppModel, call: CallModel, modifier: Modifier = Modifier) {
    val scope = LocalAppScope.current
    Box(modifier.background(Theme.background), contentAlignment = Alignment.Center) {
        Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            KitText("RoomView", font = Font(20f, Font.Bold))
            KitText(call.room, font = Font(15f, mono = true), color = Theme.muted)
            KitText("${call.people.size + 1}", font = Font(13f, digits = true), color = Theme.muted)
            GlassButton({ scope.launch { app.leave() } }, destructive = true) { KitText(t("Leave")) }
        }
    }
}
