package app.tomoshibi.ui.kit.gallery

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Accent
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.basics.Theme
import app.tomoshibi.ui.kit.Backdrop
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.GlassBackdrop
import app.tomoshibi.ui.kit.GlassShell
import app.tomoshibi.ui.kit.GlassSupport
import app.tomoshibi.ui.kit.GlassSwitch
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.PillButton
import app.tomoshibi.ui.kit.PillRole
import app.tomoshibi.ui.kit.SegmentedChoice
import app.tomoshibi.ui.kit.glassSource

/**
 * The UI kit, every piece in its states, for the owner to look at on a
 * device (debug builds only): `adb shell am start -n
 * app.tomoshibi.android/app.tomoshibi.ui.kit.gallery.KitGalleryActivity`, or
 * the "Tomoshibi Kit" icon. Choices made here (language, colour) are put back
 * as they were on leaving, and the colour is never written down.
 */
class KitGalleryActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        Localization.install(this)
        Appearance.install(this).keeps = false
        val start = intent.getStringExtra("section") ?: "glass"
        setContent { KitGallery(start) }
    }
}

@Composable
fun KitGallery(start: String = "glass") {
    var section by rememberSaveable { mutableStateOf(start) }
    val localization = Localization.shared
    DisposableEffect(Unit) {
        val followed = localization.followsSystem
        val language = localization.language
        onDispose { if (followed) localization.followSystem() else localization.choose(language) }
    }
    GlassBackdrop {
        Box(Modifier.fillMaxSize().background(Theme.background)) {
            Backdrop(Modifier.fillMaxSize().glassSource())
            Column(Modifier.fillMaxSize().statusBarsPadding()) {
                GlassShell(Modifier.padding(10.dp).fillMaxWidth(), cornerRadius = 22.dp) {
                    Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            KitText("Tomoshibi Kit", Modifier.weight(1f), font = Font(17f, Font.Bold))
                            KitText("No glass", font = Font(11.5f), color = Theme.muted)
                            GlassSwitch(GlassSupport.forcedFallback, { GlassSupport.forcedFallback = it },
                                Modifier.padding(start = 6.dp, end = 12.dp), label = "No glass")
                            KitText("All glass", font = Font(11.5f), color = Theme.muted)
                            GlassSwitch(GlassSupport.everywhere, { GlassSupport.everywhere = it },
                                Modifier.padding(start = 6.dp), label = "All glass")
                        }
                        SegmentedChoice(
                            Language.entries.map { it to it.displayName },
                            localization.language,
                            { localization.choose(it) },
                        )
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            for (accent in Accent.entries) {
                                val chosen = Appearance.shared.accent == accent
                                Box(
                                    Modifier
                                        .size(22.dp)
                                        .background(accent.color, CircleShape)
                                        .border(2.dp, if (chosen) Color.White else Color.Transparent, CircleShape)
                                        .semantics { contentDescription = accent.displayName }
                                        .clickable { Appearance.shared.choose(accent) },
                                )
                            }
                        }
                        Row(
                            Modifier.horizontalScroll(rememberScrollState()),
                            horizontalArrangement = Arrangement.spacedBy(6.dp),
                        ) {
                            for (one in gallerySections) {
                                PillButton(one.title, { section = one.key },
                                    role = if (one.key == section) PillRole.Prominent else PillRole.Normal)
                            }
                        }
                    }
                }
                AnimatedContent(
                    section,
                    Modifier.weight(1f),
                    transitionSpec = { Theme.arrive togetherWith Theme.depart },
                    label = "section",
                ) { key ->
                    val page = gallerySections.first { it.key == key }
                    if (page.key == "popovers") {
                        Box(Modifier.fillMaxSize().navigationBarsPadding()) { page.content(GalleryOptions()) }
                    } else {
                        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).navigationBarsPadding()) {
                            page.content(GalleryOptions())
                            Spacer(Modifier.size(40.dp))
                        }
                    }
                }
            }
        }
    }
}
