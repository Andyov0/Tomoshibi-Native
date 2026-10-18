package app.tomoshibi.ui.settings

import android.content.Context
import android.content.SharedPreferences
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onPlaced
import androidx.compose.ui.layout.positionInParent
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Localization
import app.tomoshibi.basics.Preferences
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.CallPreferences
import app.tomoshibi.ui.kit.AccountAvatar
import app.tomoshibi.ui.kit.Backdrop
import app.tomoshibi.ui.kit.ContentLayer
import app.tomoshibi.ui.kit.FloatingPopoverLayer
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.GlassBackdrop
import app.tomoshibi.ui.kit.GlassButton
import app.tomoshibi.ui.kit.GlassButtonShape
import app.tomoshibi.ui.kit.Hairline
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.Motion
import app.tomoshibi.ui.kit.SectionTile
import app.tomoshibi.ui.kit.SettingsPageHeading
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.glassPane
import app.tomoshibi.ui.kit.glassSource
import app.tomoshibi.ui.kit.pressable
import app.tomoshibi.ui.kit.rememberPopovers
import app.tomoshibi.ui.kit.rememberPress
import app.tomoshibi.ui.kit.rounded
import kotlinx.coroutines.launch

// The settings, drawn as the app's own rather than as a preferences screen
// (Settings.swift): the owner found the grouped form too plain and did not
// want the system's stock pieces (2026-10-17). On a phone the sections are a
// list that drills in, as on the iPhone; from 600 points across (a tablet, a
// foldable opened out) a glass sidebar of sections beside the page, as the
// Mac's Settings window has them. What they hold is the web's: the account
// page (account/Account.tsx) and the choices it writes where a call reads
// them (live/preferences.ts, live/controls.ts, live/duck.ts, live/sound.ts),
// under the web's storage names, so that a choice made here is the one a
// call makes.

/** The sections (Settings.swift `SettingsTopic`). */
enum class SettingsTopic(val symbol: String) {
    General("slider.horizontal.3"),
    Media("video.fill"),
    Watching("play.rectangle.fill"),
    Music("music.note"),
    Account("person.crop.circle.fill"),
    About("info.circle.fill");

    val title: String
        get() = when (this) {
            General -> t("General")
            Media -> t("Audio & video")
            Watching -> t("Watch together")
            Music -> t("Music")
            Account -> t("Account")
            About -> t("About")
        }

    val detail: String
        get() = when (this) {
            General -> t("Language, colour, and how a call is laid out.")
            Media -> t("What starts on when you join, and which devices.")
            Watching -> t("How sharp a video watched together may get here.")
            Music -> t("How music sounds in a call.")
            Account -> t("Who you are here, and what you can use.")
            About -> t("The app, and where its code can be read.")
        }
}

/** Where the settings stop being a phone's list and become a sidebar beside a page. */
private val WIDE = 600.dp

/**
 * The settings: on a phone a full page with its own close, a list of the
 * sections that drills into each; on a wide screen a sidebar of sections
 * beside the chosen one. Back goes from a section to the list, and from the
 * list closes.
 */
@Composable
fun SettingsView(app: AppModel, close: () -> Unit, modifier: Modifier = Modifier) {
    SettingsView(app, close, modifier, opening = null)
}

/** The settings opened at [opening], for a test to look at one section without a finger. */
@Composable
internal fun SettingsView(app: AppModel, close: () -> Unit, modifier: Modifier, opening: SettingsTopic?) {
    val preferences = app.preferences
    KeepFresh(preferences)
    ProvideSettingsPlatform {
        val popovers = rememberPopovers()
        FloatingPopoverLayer(popovers) {
            GlassBackdrop {
                BoxWithConstraints(modifier.fillMaxSize()) {
                    Box(Modifier.matchParentSize().background(Theme.background))
                    Backdrop(Modifier.matchParentSize().glassSource())
                    if (maxWidth >= WIDE) {
                        WideSettings(app, preferences, close, opening) { popovers.close() }
                    } else {
                        PhoneSettings(app, preferences, close, opening) { popovers.close() }
                    }
                }
            }
        }
    }
}

/**
 * Reads the call's choices again whenever the app's store changes while the
 * settings show, as the Apple apps do on UserDefaults' notice: a copy read
 * once when the page opened would go on saying, and writing back, what had
 * been true then.
 */
@Composable
private fun KeepFresh(preferences: CallPreferences) {
    val context = LocalContext.current
    DisposableEffect(preferences, context) {
        preferences.refresh()
        val store = context.getSharedPreferences(Preferences.FILE, Context.MODE_PRIVATE)
        // Held here: the store keeps only a weak reference to its listeners.
        val listener = SharedPreferences.OnSharedPreferenceChangeListener { _, _ -> preferences.refresh() }
        store.registerOnSharedPreferenceChangeListener(listener)
        onDispose { store.unregisterOnSharedPreferenceChangeListener(listener) }
    }
}

// MARK: - The phone's list

@Composable
private fun PhoneSettings(
    app: AppModel,
    preferences: CallPreferences,
    close: () -> Unit,
    opening: SettingsTopic?,
    closePopovers: () -> Unit,
) {
    var open by rememberSaveable { mutableStateOf(opening) }
    LaunchedEffect(open) { closePopovers() }
    BackHandler { if (open != null) open = null else close() }
    val shift = with(LocalDensity.current) { 60.dp.roundToPx() }
    AnimatedContent(
        open,
        Modifier.fillMaxSize(),
        transitionSpec = {
            if (targetState != null) {
                // The section slides over the list from the trailing edge,
                // and the list gives way a little behind it.
                (slideInHorizontally(Theme.settle()) { it } + fadeIn(Theme.settle()))
                    .togetherWith(slideOutHorizontally(Theme.leave()) { -shift } + fadeOut(Theme.leave()))
                    .apply { targetContentZIndex = 1f }
            } else {
                (slideInHorizontally(Theme.settle()) { -shift } + fadeIn(Theme.settle()))
                    .togetherWith(slideOutHorizontally(Theme.leave()) { it } + fadeOut(Theme.leave()))
                    .apply { targetContentZIndex = 0f }
            }
        },
        label = "drill",
    ) { topic ->
        if (topic == null) {
            Home(app, preferences, close) { open = it }
        } else {
            Drilled(topic, app, preferences) { open = null }
        }
    }
}

private val PhoneInsets: WindowInsets
    @Composable get() = WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal + WindowInsetsSides.Top)

@Composable
private fun Home(app: AppModel, preferences: CallPreferences, close: () -> Unit, open: (SettingsTopic) -> Unit) {
    ContentLayer {
        Column(
            Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .windowInsetsPadding(PhoneInsets)
                .padding(horizontal = 16.dp)
                .padding(bottom = 30.dp)
                .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Bottom)),
            verticalArrangement = Arrangement.spacedBy(18.dp),
        ) {
            Row(Modifier.fillMaxWidth().padding(top = 22.dp).rise(), verticalAlignment = Alignment.CenterVertically) {
                KitText(t("Settings"), Modifier.weight(1f), font = Font(32f, Font.Bold))
                GlassButton(close, Modifier.testTag("settings.close"), shape = GlassButtonShape.Circle(36.dp), label = t("Close")) {
                    Symbol("xmark", size = 13f, weight = Font.Bold)
                }
            }
            AccountCard(app, Modifier.rise(50)) { open(SettingsTopic.Account) }
            val rows = listOf(SettingsTopic.General, SettingsTopic.Media, SettingsTopic.Watching, SettingsTopic.Music, SettingsTopic.About)
            Column(Modifier.fillMaxWidth().rise(100).glassPane(26.dp)) {
                for (one in rows) {
                    TopicRow(one, summary(one, app, preferences), last = one == rows.last()) { open(one) }
                }
            }
        }
    }
}

/** The account at the head of the list, the way into its page. */
@Composable
private fun AccountCard(app: AppModel, modifier: Modifier, onClick: () -> Unit) {
    val press = rememberPress()
    val light by animateFloatAsState(if (press.lit) 0.06f else 0f, Motion.hover(), label = "light")
    val shape = rounded(26f)
    val account = app.account
    Row(
        modifier
            .fillMaxWidth()
            .glassPane(26.dp)
            .background(Color.White.copy(alpha = light), shape)
            .pressable(press, shape, true, Role.Button, onClick = onClick)
            .padding(16.dp)
            .testTag("settings.account"),
        horizontalArrangement = Arrangement.spacedBy(14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (account != null) {
            AccountAvatar(account.name, app.avatarAddress, size = 54.dp)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                KitText(account.name, font = Font(19f, Font.Semibold), lines = 1)
                KitText(account.trip, font = Font(13f, mono = true), color = Theme.muted, lines = 1)
            }
        } else {
            Mark(size = 54.dp)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                KitText(t("Account"), font = Font(19f, Font.Semibold))
                KitText(t("Sign in to change your passphrase or picture."), font = Font(13f), color = Theme.muted)
            }
        }
        // The Swift row's spacer, at least six points, with the row's
        // spacing on either side of it.
        Spacer(Modifier.width(6.dp))
        Chevron()
    }
}

@Composable
private fun TopicRow(one: SettingsTopic, summary: String, last: Boolean, onClick: () -> Unit) {
    val press = rememberPress()
    val light by animateFloatAsState(if (press.lit) 0.06f else 0f, Motion.hover(), label = "light")
    Column(Modifier.fillMaxWidth()) {
        Row(
            Modifier
                .fillMaxWidth()
                .background(Color.White.copy(alpha = light))
                .pressable(press, androidx.compose.ui.graphics.RectangleShape, true, Role.Button, onClick = onClick)
                .padding(horizontal = 14.dp, vertical = 11.dp)
                .testTag("settings.topic.${one.name.lowercase()}"),
            horizontalArrangement = Arrangement.spacedBy(13.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            SectionTile(one.symbol, lit = true, size = 32f)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                KitText(one.title, font = Font(16f, Font.Medium), color = Theme.foreground, lines = 1)
                KitText(summary, font = Font(12.5f), color = Theme.muted, lines = 1)
            }
            Spacer(Modifier.width(6.dp))
            Chevron()
        }
        if (!last) Hairline(59.dp)
    }
}

/** What a section is set to now, under its name in the list. */
private fun summary(one: SettingsTopic, app: AppModel, preferences: CallPreferences): String = when (one) {
    SettingsTopic.General -> {
        val localization = Localization.shared
        if (localization.followsSystem) t("Same as the system") else localization.language.displayName
    }
    SettingsTopic.Media -> listOfNotNull(
        if (app.startWithMicrophone) t("Microphone") else null,
        if (app.startWithCamera) t("Camera") else null,
    ).joinToString(" · ").ifEmpty { one.detail }
    SettingsTopic.Watching -> watchLabel(preferences.watchQuality)
    SettingsTopic.Music -> if (preferences.musicQuality == "best") t("Highest") else t("Lossless")
    else -> one.detail
}

@Composable
private fun Drilled(topic: SettingsTopic, app: AppModel, preferences: CallPreferences, back: () -> Unit) {
    val density = LocalDensity.current
    Column(
        Modifier
            .fillMaxSize()
            // Back with a swipe from the left edge, as every drill-in on the
            // iPhone goes back; where the system's own back gesture has the
            // edge, Back reaches the same place.
            .pointerInput(Unit) {
                val edge = with(density) { 36.dp.toPx() }
                val far = with(density) { 80.dp.toPx() }
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
                    if (down.position.x >= edge) return@awaitEachGesture
                    var travelled = 0f
                    while (true) {
                        val event = awaitPointerEvent(PointerEventPass.Initial)
                        val change = event.changes.firstOrNull { it.id == down.id } ?: break
                        travelled = change.position.x - down.position.x
                        if (!change.pressed) break
                    }
                    if (travelled > far) back()
                }
            }
            .verticalScroll(rememberScrollState())
            .windowInsetsPadding(PhoneInsets)
            .padding(horizontal = 16.dp)
            .padding(bottom = 34.dp)
            .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Bottom)),
    ) {
        GlassButton(
            back,
            Modifier.padding(top = 22.dp, bottom = 18.dp).testTag("settings.back"),
            shape = GlassButtonShape.Capsule(36.dp),
        ) {
            Symbol("chevron.left", size = 13f, weight = Font.Bold)
            KitText(t("Settings"), font = Font(14f, Font.Semibold), lines = 1)
        }
        ContentLayer { Page(topic, app, preferences) }
    }
}

// MARK: - The wide screen's sidebar

@Composable
private fun WideSettings(
    app: AppModel,
    preferences: CallPreferences,
    close: () -> Unit,
    opening: SettingsTopic?,
    closePopovers: () -> Unit,
) {
    var section by rememberSaveable { mutableStateOf(opening ?: SettingsTopic.General) }
    LaunchedEffect(section) { closePopovers() }
    BackHandler { close() }
    val rise = with(LocalDensity.current) { 10.dp.roundToPx() }
    Row(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
        Sidebar(app, section, close, Modifier.width(256.dp).fillMaxHeight().padding(10.dp)) { section = it }
        AnimatedContent(
            section,
            Modifier.weight(1f).fillMaxHeight(),
            transitionSpec = {
                (fadeIn(Theme.settle()) + slideInVertically(Theme.settle()) { rise }).togetherWith(fadeOut(Theme.leave()))
            },
            label = "section",
        ) { topic ->
            Column(
                Modifier.fillMaxSize().verticalScroll(rememberScrollState()),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                ContentLayer {
                    Page(
                        topic,
                        app,
                        preferences,
                        Modifier
                            .padding(horizontal = 28.dp)
                            .padding(top = 44.dp, bottom = 32.dp)
                            .widthIn(max = 620.dp)
                            .fillMaxWidth(),
                    )
                }
            }
        }
    }
}

@Composable
private fun Sidebar(app: AppModel, section: SettingsTopic, close: () -> Unit, modifier: Modifier, choose: (SettingsTopic) -> Unit) {
    val account = app.account
    Column(modifier.glassPane(22.dp).padding(10.dp), verticalArrangement = Arrangement.spacedBy(3.dp)) {
        // Where the Mac's window has its buttons: the way out, beside the
        // name of the window.
        Row(
            Modifier.fillMaxWidth().padding(start = 10.dp, end = 4.dp, top = 6.dp, bottom = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            KitText(t("Settings"), Modifier.weight(1f), font = Font(21f, Font.Bold), lines = 1)
            CloseButton(t("Close"), close, Modifier.testTag("settings.close"))
        }
        SidebarRows(section, choose)
        Spacer(Modifier.weight(1f).heightIn(min = 12.dp))
        AnimatedVisibility(account != null, enter = Theme.arrive, exit = Theme.depart) {
            val shown = remember { mutableStateOf(account) }
            if (account != null) shown.value = account
            val who = shown.value ?: return@AnimatedVisibility
            Row(
                Modifier
                    .fillMaxWidth()
                    .background(Color.White.copy(alpha = 0.05f), rounded(14f))
                    .padding(10.dp),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                AccountAvatar(who.name, app.avatarAddress, size = 32.dp)
                Column(verticalArrangement = Arrangement.spacedBy(1.dp)) {
                    KitText(who.name, font = Font(12.5f, Font.Semibold), lines = 1)
                    KitText(who.trip, font = Font(11f, mono = true), color = Theme.muted, lines = 1)
                }
            }
        }
    }
}

/**
 * The sections, the chosen one under a plate that slides to it, as the Mac's
 * highlight moves with `matchedGeometryEffect`. Taller than the Mac's rows,
 * which are pointed at: these are pressed with a finger.
 */
@Composable
private fun SidebarRows(section: SettingsTopic, choose: (SettingsTopic) -> Unit) {
    val tops = remember { mutableStateMapOf<SettingsTopic, Pair<Int, Int>>() }
    val top = remember { Animatable(0f) }
    val height = remember { Animatable(0f) }
    val scope = rememberCoroutineScope()
    var placed by remember { mutableStateOf(false) }
    val target = tops[section]
    LaunchedEffect(target) {
        val (y, h) = target ?: return@LaunchedEffect
        if (!placed) {
            top.snapTo(y.toFloat())
            height.snapTo(h.toFloat())
            placed = true
        } else {
            scope.launch { top.animateTo(y.toFloat(), Theme.settle()) }
            height.animateTo(h.toFloat(), Theme.settle())
        }
    }
    Box(Modifier.fillMaxWidth()) {
        if (placed) {
            val density = LocalDensity.current
            Box(
                Modifier
                    .offset { IntOffset(0, top.value.toInt()) }
                    .fillMaxWidth()
                    .height(with(density) { height.value.toDp() })
                    .background(Color.White.copy(alpha = 0.11f), rounded(12f))
                    .border(1.dp, Color.White.copy(alpha = 0.07f), rounded(12f)),
            )
        }
        Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
            for (one in SettingsTopic.entries) {
                SidebarRow(one, chosen = one == section, Modifier.onPlaced { tops[one] = it.positionInParent().y.toInt() to it.size.height }) {
                    choose(one)
                }
            }
        }
    }
}

@Composable
private fun SidebarRow(one: SettingsTopic, chosen: Boolean, modifier: Modifier, onClick: () -> Unit) {
    val press = rememberPress()
    val ink by animateColorAsState(
        if (chosen) Theme.foreground else Theme.foreground.copy(alpha = 0.82f),
        Theme.settle(),
        label = "ink",
    )
    val light by animateFloatAsState(if (press.lit && !chosen) 0.05f else 0f, Motion.hover(), label = "light")
    val shape = rounded(12f)
    Row(
        modifier
            .fillMaxWidth()
            .background(Color.White.copy(alpha = light), shape)
            .pressable(press, shape, true, Role.Tab, selected = chosen, onClick = onClick)
            .padding(horizontal = 8.dp, vertical = 9.dp)
            .testTag("settings.section.${one.name.lowercase()}"),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        SectionTile(one.symbol, lit = chosen, size = 26f)
        KitText(one.title, Modifier.weight(1f), font = Font(13.5f, if (chosen) Font.Semibold else Font.Regular), color = ink, lines = 1)
    }
}

// MARK: - One page

@Composable
private fun Page(topic: SettingsTopic, app: AppModel, preferences: CallPreferences, modifier: Modifier = Modifier) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(24.dp)) {
        SettingsPageHeading(topic.title, topic.symbol, Modifier.rise(), detail = topic.detail)
        Box(Modifier.rise(50)) {
            when (topic) {
                SettingsTopic.General -> GeneralSettings()
                SettingsTopic.Media -> MediaSettings(app, preferences)
                SettingsTopic.Watching -> WatchingSettings(preferences)
                SettingsTopic.Music -> MusicSettings(app, preferences)
                SettingsTopic.Account -> AccountSettings(app)
                SettingsTopic.About -> AboutSettings(app)
            }
        }
    }
}
