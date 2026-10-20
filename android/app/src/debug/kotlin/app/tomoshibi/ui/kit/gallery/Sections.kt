package app.tomoshibi.ui.kit.gallery

import androidx.compose.foundation.Canvas
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import app.tomoshibi.ui.kit.glassSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.model.CallToast
import app.tomoshibi.ui.kit.AccountAvatar
import app.tomoshibi.ui.kit.BarAxis
import app.tomoshibi.ui.kit.BarButton
import app.tomoshibi.ui.kit.BarPopover
import app.tomoshibi.ui.kit.BarPopoverPlace
import app.tomoshibi.ui.kit.Capsule
import app.tomoshibi.ui.kit.Chip
import app.tomoshibi.ui.kit.ChoiceRow
import app.tomoshibi.ui.kit.ClockChoice
import app.tomoshibi.ui.kit.ContentLayer
import app.tomoshibi.ui.kit.CopyLinkButton
import app.tomoshibi.ui.kit.FitWidth
import app.tomoshibi.ui.kit.FloatingPopover
import app.tomoshibi.ui.kit.FloatingPopoverLayer
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.GlassButton
import app.tomoshibi.ui.kit.GlassButtonShape
import app.tomoshibi.ui.kit.GlassChoice
import app.tomoshibi.ui.kit.GlassChoiceOption
import app.tomoshibi.ui.kit.GlassField
import app.tomoshibi.ui.kit.GlassPopover
import app.tomoshibi.ui.kit.GlassShell
import app.tomoshibi.ui.kit.GlassSlider
import app.tomoshibi.ui.kit.GlassSwitch
import app.tomoshibi.ui.kit.HearingRow
import app.tomoshibi.ui.kit.Initial
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.LevelMeter
import app.tomoshibi.ui.kit.MediaArtwork
import app.tomoshibi.ui.kit.MediaBadge
import app.tomoshibi.ui.kit.MediaButton
import app.tomoshibi.ui.kit.MediaEmpty
import app.tomoshibi.ui.kit.MediaField
import app.tomoshibi.ui.kit.MediaIconButton
import app.tomoshibi.ui.kit.MediaPanelGlass
import app.tomoshibi.ui.kit.MediaPanelHeader
import app.tomoshibi.ui.kit.MediaProblem
import app.tomoshibi.ui.kit.MediaProgress
import app.tomoshibi.ui.kit.MediaRow
import app.tomoshibi.ui.kit.MediaRule
import app.tomoshibi.ui.kit.MediaSlider
import app.tomoshibi.ui.kit.MediaSpinner
import app.tomoshibi.ui.kit.MenuCard
import app.tomoshibi.ui.kit.MenuHeading
import app.tomoshibi.ui.kit.MenuItem
import app.tomoshibi.ui.kit.MenuRole
import app.tomoshibi.ui.kit.MenuRule
import app.tomoshibi.ui.kit.MenuSwitch
import app.tomoshibi.ui.kit.MomentChoice
import app.tomoshibi.ui.kit.Monogram
import app.tomoshibi.ui.kit.MonthCalendar
import app.tomoshibi.ui.kit.PageArrow
import app.tomoshibi.ui.kit.PageIndicator
import app.tomoshibi.ui.kit.PanelHeading
import app.tomoshibi.ui.kit.PanelRule
import app.tomoshibi.ui.kit.PictureButton
import app.tomoshibi.ui.kit.PillButton
import app.tomoshibi.ui.kit.PillRole
import app.tomoshibi.ui.kit.PopoverAlignment
import app.tomoshibi.ui.kit.PopoverEdge
import app.tomoshibi.ui.kit.Popovers
import app.tomoshibi.ui.kit.RoundButton
import app.tomoshibi.ui.kit.RoundRole
import app.tomoshibi.ui.kit.SaidColumn
import app.tomoshibi.ui.kit.SaidLine
import app.tomoshibi.ui.kit.SealControl
import app.tomoshibi.ui.kit.SectionTile
import app.tomoshibi.ui.kit.SegmentedChoice
import app.tomoshibi.ui.kit.SettingsPageHeading
import app.tomoshibi.ui.kit.SettingsRow
import app.tomoshibi.ui.kit.SettingsSection
import app.tomoshibi.ui.kit.SignalBadge
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.ToastStack
import app.tomoshibi.ui.kit.WrappingRow
import app.tomoshibi.ui.kit.glassPane
import app.tomoshibi.ui.kit.glassSurface
import app.tomoshibi.ui.kit.panelCard
import app.tomoshibi.ui.kit.rememberPopovers
import app.tomoshibi.ui.kit.rounded
import app.tomoshibi.ui.kit.symbolNames
import uniffi.tomoshibi_protocol.ConnectionGrade
import uniffi.tomoshibi_protocol.ConnectionReading
import uniffi.tomoshibi_protocol.ShareLimitation
import uniffi.tomoshibi_protocol.ShareReading
import java.time.LocalDate
import java.time.LocalDateTime

// Every piece of the kit in its states, one section to a page, for the
// gallery Activity and for the screenshot tests to draw the same thing.

/** One page of the gallery: what it is called, and whether it opens a popover. */
class GallerySection(val key: String, val title: String, val content: @Composable (GalleryOptions) -> Unit)

/** What a section is drawn with: a fixed day for the calendar, a popover to show open. */
class GalleryOptions(
    val today: LocalDate = LocalDate.of(2026, 10, 18),
    val openPopover: String? = null,
    val spreadToasts: Boolean = false,
)

val gallerySections = listOf(
    GallerySection("glass", "Glass") { GlassPage() },
    GallerySection("menus", "Menus") { MenusPage() },
    GallerySection("choosing", "Choosing") { ChoosingPage(it) },
    GallerySection("settings", "Settings") { SettingsPage() },
    GallerySection("bar", "Bar") { BarPage(it) },
    GallerySection("panels", "Panels") { PanelsPage() },
    GallerySection("fields", "Fields") { FieldsPage() },
    GallerySection("people", "People") { PeoplePage() },
    GallerySection("calendar", "Calendar") { CalendarPage(it) },
    GallerySection("stage", "Stage") { StagePage() },
    GallerySection("media", "Media") { MediaPage() },
    GallerySection("toasts", "Toasts") { ToastsPage(it) },
    GallerySection("signal", "Signal") { SignalPage() },
    GallerySection("said", "Said") { SaidPage() },
    GallerySection("popovers", "Popovers") { PopoversPage(it) },
    GallerySection("symbols", "Symbols") { SymbolsPage() },
)

@Composable
private fun Demo(caption: String, content: @Composable ColumnScope.() -> Unit) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        KitText(caption.uppercase(), font = Font(10.5f, Font.Semibold, tracking = 0.6f), color = Theme.muted)
        content()
    }
}

@Composable
private fun Page(content: @Composable ColumnScope.() -> Unit) {
    Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(22.dp), content = content)
}

@Composable
private fun GlassPage() = Page {
    Demo("Glass buttons over the backdrop") {
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
            GlassButton({}, shape = GlassButtonShape.Circle(44.dp), label = t("Mute microphone")) { Symbol("mic.fill", size = 16f) }
            GlassButton({}, shape = GlassButtonShape.Circle(44.dp), prominent = true, label = t("Turn camera off")) { Symbol("video.fill", size = 16f) }
            GlassButton({}, shape = GlassButtonShape.Circle(44.dp), destructive = true, label = t("Leave")) { Symbol("phone.down.fill", size = 16f) }
            GlassButton({}, shape = GlassButtonShape.Circle(44.dp), enabled = false, label = t("Share your screen")) { Symbol("rectangle.on.rectangle", size = 16f) }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            GlassButton({}, prominent = true) { KitText(t("Join")) }
            GlassButton({}, destructive = true) { KitText(t("Leave the call")) }
            GlassButton({}) {
                Symbol("link", size = 12f)
                KitText(t("Copy link"))
            }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            GlassButton({}, enabled = false) { KitText(t("Join")) }
            GlassButton({}, shape = GlassButtonShape.Capsule(30.dp)) { KitText(t("Settings")) }
        }
    }
    Demo("Glass over a busy picture, which it blurs") {
        Box(Modifier.fillMaxWidth().height(150.dp)) {
            Picture(Modifier.matchParentSize().glassSource(1f))
            Row(
                Modifier.align(Alignment.Center),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                GlassButton({}, shape = GlassButtonShape.Circle(52.dp), label = t("Mute microphone")) { Symbol("mic.fill", size = 19f) }
                Box(Modifier.size(150.dp, 96.dp).glassPane(18.dp))
                GlassButton({}, prominent = true) { KitText(t("Join")) }
            }
        }
    }
    Demo("Glass surfaces: a capsule, a pane") {
        Row(
            Modifier.height(36.dp).glassSurface(Capsule).padding(horizontal = 14.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) { KitText(t("Running this room"), font = Font(13f, Font.Medium)) }
        Box(Modifier.fillMaxWidth().glassPane().padding(16.dp)) {
            KitText(t("Everybody who types the same word can hear each other, and the server carrying the call cannot. Tell people the word some other way."), font = Font(13f))
        }
    }
    Demo("A shell: glass, with content inside") {
        GlassShell(Modifier.fillMaxWidth(), cornerRadius = 22.dp) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                KitText(t("Settings"), font = Font(14f, Font.Semibold))
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    GlassButton({}, prominent = true) { KitText(t("Join")) }
                    GlassButton({}) { KitText(t("Copy link")) }
                    GlassButton({}, shape = GlassButtonShape.Circle(36.dp), label = t("Mute microphone")) { Symbol("mic.fill", size = 14f) }
                }
                Row(
                    Modifier.height(30.dp).glassSurface(Capsule).padding(horizontal = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) { KitText(t("Signal only"), font = Font(12.5f)) }
            }
        }
    }
    Demo("The content layer: plain surfaces, no blur") {
        ContentLayer {
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
                GlassButton({}, shape = GlassButtonShape.Circle(44.dp), label = t("Mute microphone")) { Symbol("mic.fill", size = 16f) }
                GlassButton({}, prominent = true) { KitText(t("Join")) }
                GlassButton({}) { KitText(t("Copy link")) }
                Row(
                    Modifier.height(36.dp).glassSurface(Capsule).padding(horizontal = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) { KitText(t("Hidden"), font = Font(12.5f)) }
            }
        }
    }
}

@Composable
private fun MenusPage() = Page {
    var lossless by remember { mutableStateOf(true) }
    Demo("A popover menu (as drawn in its shell)") {
        GlassShell(Modifier.width(280.dp), cornerRadius = 18.dp) {
            MenuCard {
                MenuHeading(t("Share your screen"))
                MenuItem(t("Share your screen"), {}, symbol = "rectangle.on.rectangle")
                MenuItem(t("Share only sound"), {}, symbol = "waveform", checked = true)
                MenuItem(t("Copy link"), {}, symbol = "link", detail = t("Leave this call to open that link."))
                MenuSwitch(t("Lossless"), lossless, { lossless = it }, symbol = "hifispeaker")
                MenuRule()
                MenuItem(t("Watch together"), {}, symbol = "play.rectangle.fill", role = MenuRole.Prominent)
                MenuItem(t("Copy signature"), {}, symbol = "signature", busy = true)
                MenuItem(t("Make host"), {}, symbol = "crown", enabled = false)
                MenuItem(t("Leave the call"), {}, symbol = "phone.down.fill", role = MenuRole.Destructive)
            }
        }
    }
}

@Composable
private fun ChoosingPage(options: GalleryOptions) = Page {
    var on by remember { mutableStateOf(true) }
    var off by remember { mutableStateOf(false) }
    var segment by remember { mutableStateOf("signal") }
    var choice by remember { mutableStateOf("amber") }
    var device by remember { mutableStateOf(1) }
    Demo("Switches: on, off, disabled") {
        Row(horizontalArrangement = Arrangement.spacedBy(14.dp)) {
            GlassSwitch(on, { on = it }, label = t("Lossless"))
            GlassSwitch(off, { off = it }, label = t("Lossless"))
            GlassSwitch(true, {}, enabled = false, label = t("Lossless"))
            GlassSwitch(false, {}, enabled = false, label = t("Lossless"))
        }
    }
    Demo("A segmented choice") {
        SegmentedChoice(
            listOf("signal" to t("Signal only"), "detailed" to t("With details"), "hidden" to t("Hidden")),
            segment, { segment = it },
        )
    }
    Demo("A choice that opens a popover") {
        val popovers = rememberPopovers()
        LaunchedEffect(options.openPopover) { if (options.openPopover == "choice") popovers.show("choice") }
        FloatingPopoverLayer(popovers) {
            GlassChoice(
                "choice",
                listOf(
                    GlassChoiceOption("amber", t("Lamplight"), symbol = "sparkles"),
                    GlassChoiceOption("system", t("Same as the system"), detail = t("Language")),
                    GlassChoiceOption("off", t("Hidden"), disabled = true),
                ),
                choice, { choice = it }, symbol = "globe", heading = t("Language"),
            )
        }
    }
    Demo("Rows to pick one of a list from") {
        Column(Modifier.fillMaxWidth().panelCard()) {
            ChoiceRow(t("Microphone") + " (USB)", { device = 0 }, symbol = "mic", checked = device == 0)
            ChoiceRow(t("Microphone"), { device = 1 }, symbol = "mic", checked = device == 1)
            ChoiceRow(t("Same as the system"), { device = 2 }, checked = device == 2)
            ChoiceRow(t("Camera"), {}, symbol = "video", enabled = false, last = true)
        }
    }
}

@Composable
private fun SettingsPage() = Page {
    var blur by remember { mutableStateOf(true) }
    var level by remember { mutableStateOf(0.55f) }
    SettingsPageHeading(t("Appearance"), "sparkles", detail = t("Hide when nothing is happening"))
    ContentLayer {
        SettingsSection(t("General"), footer = t("Leave this call to open that link.")) {
            SettingsRow(t("Blur my background"), symbol = "person.and.background.dotted") { GlassSwitch(blur, { blur = it }) }
            SettingsRow(t("Picture quality"), detail = t("With details"), symbol = "sparkles.tv") {
                GlassButton({}, shape = GlassButtonShape.Capsule(30.dp)) { KitText(t("Signal only"), font = Font(12.5f, Font.Medium)) }
            }
            SettingsRow(t("Version"), symbol = "app.badge") { KitText("0.1.0", font = Font(13.5f), color = Theme.muted) }
            SettingsRow(t("Language"), last = true) { Symbol("chevron.right", size = 12f, tint = Theme.muted) }
        }
    }
    Demo("Section tiles: lit, plain") {
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
            SectionTile("gearshape.fill", lit = true)
            SectionTile("music.note")
            SectionTile("server.rack", lit = true, size = 36f)
            SectionTile("person.crop.circle.fill", size = 36f)
        }
    }
    Demo("The level meter") {
        LevelMeter(level)
        LevelMeter(0f)
        LevelMeter(1f)
        GlassSlider(level.toDouble(), { level = it.toFloat() }, t("Microphone"))
    }
}

@Composable
private fun BarPage(options: GalleryOptions) = Page {
    Demo("The call bar: plates inside its glass") {
        Row(
            Modifier.glassSurface(Capsule).padding(7.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            BarButton("mic.slash.fill", t("Unmute microphone"), {}, quiet = true)
            BarButton("video.fill", t("Turn camera off"), {}, lit = true)
            BarButton("bubble.left.fill", t("Show messages"), {}, dot = true)
            BarButton("ellipsis", t("More"), {}, pressed = true)
            BarButton("crown.fill", t("Running this room"), {}, size = 38.dp)
            BarButton("phone.down.fill", t("Leave"), {}, enabled = false, size = 38.dp)
        }
    }
}

@Composable
private fun PanelsPage() = Page {
    Demo("A panel: heading, rule, cards, round and pill buttons") {
        GlassShell(Modifier.fillMaxWidth()) {
            Column {
                PanelHeading(t("Running this room"), t("Close"), {}, symbol = "crown.fill", symbolTint = Theme.tally) {
                    RoundButton("arrow.clockwise", t("Try again"), {})
                }
                PanelRule()
                Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Row(
                        Modifier.fillMaxWidth().panelCard(Theme.tally).padding(12.dp),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Monogram("Mei Lin", "participant-1", size = 28.dp)
                        KitText(t("{name} joined", mapOf("name" to "Mei Lin")), Modifier.weight(1f), font = Font(13f))
                        PillButton(t("Let in"), {}, role = PillRole.Prominent)
                    }
                    Row(
                        Modifier.fillMaxWidth().panelCard().padding(12.dp),
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        RoundButton("crown", t("Make host"), {})
                        RoundButton("crown", t("Make host"), {}, faint = true)
                        RoundButton("person.badge.minus", t("Remove"), {}, role = RoundRole.Destructive)
                        RoundButton("xmark", t("Close"), {}, enabled = false)
                        Spacer(Modifier.weight(1f))
                        PillButton(t("Mute"), {})
                        PillButton(t("Remove"), {}, role = PillRole.Destructive, busy = true)
                    }
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Box(Modifier.weight(1f)) { PillButton(t("Join"), {}, role = PillRole.Prominent, wide = true, height = 30.dp) }
                        Box(Modifier.weight(1f)) { PillButton(t("Leave"), {}, wide = true, height = 30.dp, enabled = false) }
                    }
                }
            }
        }
    }
    Demo("Monograms: the same person, the same colours everywhere") {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Monogram("Andy Wang", "andy")
            Monogram("Mei Lin", "participant-1")
            Monogram("张 伟", "participant-2")
            Monogram("田中 さん", "participant-3")
            Monogram("guest", "guest-7f3a9c", size = 32.dp)
            Monogram("Bartholomew Wolfe", "sig:Zq3n0Vf8", size = 40.dp)
        }
    }
    Demo("FitWidth: no wider than its words, wrapping at the most") {
        FitWidth(minimum = 120.dp, maximum = 240.dp) {
            Box(Modifier.glassPane(14.dp).padding(10.dp)) { KitText(t("Leave"), font = Font(13f)) }
        }
        FitWidth(minimum = 120.dp, maximum = 240.dp) {
            Box(Modifier.glassPane(14.dp).padding(10.dp)) { KitText(t("The media server did not answer. Try again."), font = Font(13f)) }
        }
    }
}

@Composable
private fun FieldsPage() = Page {
    var empty by remember { mutableStateOf("") }
    var name by remember { mutableStateOf("Andy") }
    var secret by remember { mutableStateOf("lamplight") }
    var room by remember { mutableStateOf("standup-0930") }
    var sealOff by remember { mutableStateOf(false) }
    var sealOn by remember { mutableStateOf(true) }
    var word by remember { mutableStateOf("") }
    var wordTyped by remember { mutableStateOf("ember") }
    var search by remember { mutableStateOf("") }
    var link by remember { mutableStateOf("https://music.example/list/42") }
    ContentLayer {
        Demo("Fields: empty, filled, a secret that lights, mono, read only") {
            GlassField(t("Your name"), empty, { empty = it }, symbol = "person")
            GlassField(t("Your name"), name, { name = it }, symbol = "person")
            GlassField(t("Passphrase"), secret, { secret = it }, symbol = "key", lights = true, secret = true)
            GlassField(t("Room name"), room, { room = it }, symbol = "number", mono = true, height = 40.dp)
            GlassField(t("Media core"), "0.1.0 (6)", {}, symbol = "cpu", readOnly = true, height = 40.dp)
        }
        Demo("Sealing a call: off, on with no word, sealed") {
            SealControl(sealOff, { sealOff = it }, "", {})
            SealControl(sealOn, { sealOn = it }, word, { word = it })
            SealControl(true, {}, wordTyped, { wordTyped = it })
        }
        Demo("Copying a link") { CopyLinkButton("https://meet.example/#/standup") }
        Demo("Media fields") {
            MediaField(t("Search songs"), search, { search = it }, {}, symbol = "magnifyingglass")
            MediaField(t("Paste a playlist or song link"), link, { link = it }, {}, symbol = "link", url = true)
        }
    }
}

@Composable
private fun PeoplePage() = Page {
    Demo("The person's picture, and the initial where there is none") {
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
            AccountAvatar("Andy", null)
            AccountAvatar("mei", null, size = 44.dp)
            Initial("张伟", size = 44.dp)
            Initial("田中", size = 28.dp)
        }
    }
    Demo("Chips: given, lit, withheld, mono") {
        WrappingRow {
            Chip(t("Camera"), symbol = "video.fill")
            Chip(t("Microphone"), symbol = "mic.fill", lit = true)
            Chip(t("Share your screen"), symbol = "rectangle.on.rectangle", withheld = true)
            Chip("sig:Zq3n0Vf8", mono = true)
            Chip(t("Watch together"), symbol = "play.rectangle.fill")
            Chip(t("Music"), symbol = "music.note", withheld = true)
            Chip(t("Running this room"), symbol = "crown.fill", lit = true)
        }
    }
}

@Composable
private fun CalendarPage(options: GalleryOptions) = Page {
    var day by remember { mutableStateOf(LocalDateTime.of(2026, 10, 22, 9, 30)) }
    var hour by remember { mutableStateOf(9) }
    var minute by remember { mutableStateOf(30) }
    var moment by remember { mutableStateOf<LocalDateTime?>(LocalDateTime.of(2026, 10, 24, 18, 0)) }
    val popovers = rememberPopovers()
    LaunchedEffect(options.openPopover) { options.openPopover?.let { if (it.startsWith("moment") || it.startsWith("clock")) popovers.show(it) } }
    FloatingPopoverLayer(popovers) {
        ContentLayer {
            Demo("A month: today ringed, the chosen day lit, the past dimmed") {
                Box(Modifier.fillMaxWidth().panelCard().padding(14.dp)) {
                    MonthCalendar(day, { day = it }, earliest = options.today.atStartOfDay(), today = options.today)
                }
            }
            Demo("A time, and a moment that may be left open") {
                ClockChoice("clock", hour, minute, { hour = it }, { minute = it })
                MomentChoice("moment", moment, { moment = it }, earliest = options.today.atStartOfDay())
                MomentChoice("moment.unset", null, {})
            }
        }
    }
}

@Composable
private fun StagePage() = Page {
    Demo("Over a picture: round, words, paging, the page") {
        Box(Modifier.fillMaxWidth().height(220.dp)) {
            // The picture is a source of its own, beside the controls rather
            // than around them: glass cannot sample what it is inside of.
            Picture(Modifier.matchParentSize().glassSource(1f))
            Row(Modifier.align(Alignment.TopEnd).padding(10.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                PictureButton({}, label = t("Turn camera off")) { Symbol("arrow.up.left.and.arrow.down.right", size = 12.5f) }
                PictureButton({}, words = true) { KitText(t("Stop sharing")) }
            }
            PageIndicator(1, 3, Modifier.align(Alignment.TopCenter).padding(top = 10.dp))
            PageArrow("chevron.left", t("Previous page"), {}, Modifier.align(Alignment.CenterStart).padding(8.dp))
            PageArrow("chevron.right", t("Next page"), {}, Modifier.align(Alignment.CenterEnd).padding(8.dp))
        }
    }
}

/** A bright slide with sharp edges, for the glass over it to be seen to blur. */
@Composable
private fun Picture(modifier: Modifier) {
    Canvas(modifier.clip(rounded(8f))) {
        drawRect(Brush.linearGradient(listOf(Color(0xFFEDE6D8), Color(0xFF7C8FA3), Color(0xFF1B2330))))
        val stripe = size.width / 14
        for (index in 0 until 14 step 2) {
            drawRect(Color(0xFFE2563B).copy(alpha = 0.85f), Offset(index * stripe, 0f), Size(stripe * 0.5f, size.height * 0.22f))
        }
        for (line in 0 until 5) {
            drawRect(Color.White, Offset(size.width * 0.08f, size.height * (0.40f + line * 0.1f)),
                Size(size.width * (0.84f - line * 0.12f), size.height * 0.035f))
        }
        drawCircle(Color(0xFF2E6BE6), size.height * 0.16f, Offset(size.width * 0.86f, size.height * 0.72f))
    }
}

@Composable
private fun MediaPage() = Page {
    var volume by remember { mutableDoubleStateOf(0.7) }
    var place by remember { mutableDoubleStateOf(0.35) }
    var muted by remember { mutableStateOf(true) }
    var other by remember { mutableDoubleStateOf(0.0) }
    MediaPanelGlass(Modifier.fillMaxWidth()) {
        Column {
            MediaPanelHeader(t("Music"), t("Close"), {}, symbol = "music.note")
            MediaRule()
            Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                MediaField(t("Search songs"), "", {}, {}, symbol = "magnifyingglass")
                MediaRow(lit = true) {
                    MediaArtwork("", Modifier.size(36.dp))
                    Column(Modifier.weight(1f).padding(horizontal = 10.dp), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                        KitText("Moon River", font = Font(13f, Font.Medium), lines = 1)
                        Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                            MediaBadge(t("Lossless"), symbol = "waveform", lit = true)
                            MediaBadge(t("Paused"))
                        }
                    }
                    MediaIconButton("arrow.up.to.line", t("Play {title} next", mapOf("title" to "Moon River")), {}, size = 24.dp)
                    MediaIconButton("xmark", t("Remove"), {}, size = 24.dp)
                }
                MediaRow { hovering ->
                    MediaArtwork("", Modifier.size(36.dp), symbol = "film", corner = 6f)
                    KitText("Night Train", Modifier.weight(1f).padding(horizontal = 10.dp), font = Font(13f), lines = 1)
                    MediaIconButton("play.fill", t("Watch together"), {}, prominent = true)
                }
                MediaSlider(place, { place = it }, label = t("Videos"))
                MediaProgress(0.42f)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                    MediaButton({}, prominent = true) { KitText(t("Ask for a song")) }
                    MediaButton({}) {
                        Symbol("plus", size = 11f)
                        KitText(t("Add a video"))
                    }
                    MediaButton({}, quiet = true) { KitText(t("Close")) }
                    MediaSpinner()
                }
                MediaButton({}, enabled = false) { KitText(t("Ask for a song")) }
                HearingRow("Mei Lin", volume, false, { volume = it }, {})
                HearingRow("Andy", 1.0, muted, {}, { muted = it })
                HearingRow("guest", other, false, { other = it }, {})
                MediaEmpty("magnifyingglass", t("Nothing found."))
                MediaProblem(t("The media server did not answer. Try again."))
            }
        }
    }
}

@Composable
private fun ToastsPage(options: GalleryOptions) {
    // Notices as a call keeps them, in a list of the page's own: the gallery
    // has no call, and the corner only draws what it is handed.
    val said = remember {
        mutableStateListOf(
            CallToast(t("{name} joined", mapOf("name" to "Mei Lin")), lasts = null, closable = false),
            CallToast(t("That could not be done. Try again."), kind = CallToast.Kind.Failure, lasts = null, actionTitle = t("Try again")),
            CallToast(t("{name} left", mapOf("name" to "Andy")), detail = t("Leave this call to open that link."),
                kind = CallToast.Kind.Signal, lasts = null),
        )
    }
    val gone: (Long) -> Unit = { id -> said.removeAll { it.id == id } }
    Page {
        Demo("Piled: the newest in front, two peeking above it") {
            Box(Modifier.fillMaxWidth().height(110.dp), contentAlignment = Alignment.BottomEnd) {
                ToastStack(said, act = gone, dismiss = gone, hold = {}, spread = options.spreadToasts)
            }
        }
        Demo("Spread, as a tap on the corner spreads it") {
            Box(Modifier.fillMaxWidth().height(260.dp), contentAlignment = Alignment.BottomEnd) {
                ToastStack(said, act = gone, dismiss = gone, hold = {}, spread = true)
            }
        }
    }
}

private fun reading(grade: ConnectionGrade, measured: Boolean = true) = ConnectionReading(
    grade = grade,
    rttMs = if (grade == ConnectionGrade.LOST) null else 48.0,
    lossPercent = if (grade == ConnectionGrade.POOR) 3.4 else 0.2,
    upKbps = 1850.0,
    downKbps = 640.0,
    jitterMs = 6.0,
    share = ShareReading(1920.0, 1080.0, 24.0, true, ShareLimitation.BANDWIDTH),
    ownAddress = "203.0.113.24",
    holding = null,
    measured = measured,
)

@Composable
private fun SignalPage() = Page {
    var open by remember { mutableStateOf(true) }
    Demo("The bars, for each grade") {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for (grade in ConnectionGrade.entries) SignalBadge(reading(grade), detailed = false, open = false, onOpenChange = {})
        }
    }
    Demo("With details, opened") {
        SignalBadge(reading(ConnectionGrade.FAIR), detailed = true, open = open, onOpenChange = { open = it },
            relay = "hk-1", holding = "sg-2")
    }
    Demo("Not yet measured") {
        SignalBadge(reading(ConnectionGrade.GOOD, measured = false).copy(share = null, lossPercent = null), detailed = true,
            open = true, onOpenChange = {}, relay = "hk-1")
    }
}

@Composable
private fun SaidPage() = Page {
    var bounds by remember { mutableStateOf<Rect?>(null) }
    Demo("What was just said, in its corner") {
        Box(Modifier.fillMaxWidth().height(300.dp), contentAlignment = Alignment.BottomStart) {
            SaidColumn(
                listOf(
                    SaidLine("1", "participant-1", "Mei Lin", "Morning! The notes are at https://notes.example/standup."),
                    SaidLine("2", "andy", "Andy Wang", "看这个https://example.com/x 好的"),
                    SaidLine("3", "andy", "Andy Wang", t("That could not be done. Try again.")),
                ),
                onBounds = { bounds = it },
            )
        }
    }
}

@Composable
private fun PopoversPage(options: GalleryOptions) {
    val popovers = rememberPopovers()
    LaunchedEffect(options.openPopover) { popovers.show(options.openPopover ?: "more") }
    FloatingPopoverLayer(popovers) {
        Box(Modifier.fillMaxSize().padding(16.dp)) {
            Row(
                Modifier.align(Alignment.BottomCenter).glassSurface(Capsule).padding(7.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                BarButton("mic.fill", t("Mute microphone"), {})
                BarPopover(
                    "more", popovers, BarAxis.Horizontal,
                    panel = {
                        MenuCard {
                            MenuItem(t("Share your screen"), { popovers.close() }, symbol = "rectangle.on.rectangle")
                            MenuItem(t("Share only sound"), { popovers.close() }, symbol = "waveform", checked = true)
                            MenuRule()
                            MenuItem(t("Leave the call"), { popovers.close() }, symbol = "phone.down.fill", role = MenuRole.Destructive)
                        }
                    },
                ) {
                    BarButton("ellipsis", t("More"), { popovers.toggle("more") }, pressed = popovers.open == "more")
                }
                BarPopover(
                    "leave", popovers, BarAxis.Horizontal, place = BarPopoverPlace.End, width = 240.dp,
                    panel = {
                        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            KitText(t("Leave the call"), font = Font(13.5f, Font.Semibold))
                            PillButton(t("Leave"), { popovers.close() }, role = PillRole.Destructive, wide = true, height = 30.dp)
                        }
                    },
                ) {
                    BarButton("phone.down.fill", t("Leave"), { popovers.toggle("leave") })
                }
            }
            GlassPopover(
                "hang", popovers, Modifier.align(Alignment.TopStart), edge = PopoverEdge.Bottom,
                alignment = PopoverAlignment.Leading, width = 220.dp,
                panel = { MenuCard { MenuItem(t("Copy link"), {}, symbol = "link") } },
            ) {
                GlassButton({ popovers.toggle("hang") }) { KitText(t("Copy link")) }
            }
            FloatingPopover(
                "floating", popovers, Modifier.align(Alignment.TopEnd),
                panel = { MenuCard { MenuItem(t("Same as the system"), {}, symbol = "globe", checked = true) } },
            ) {
                GlassButton({ popovers.toggle("floating") }) { KitText(t("Language")) }
            }
        }
    }
}

@Composable
private fun SymbolsPage() = Page {
    Demo("Every SF Symbol the Swift views name, at 17 points") {
        WrappingRow(spacing = 4.dp) {
            for (name in symbolNames.sorted()) {
                Column(
                    Modifier.width(92.dp).height(58.dp).panelCard().padding(4.dp).testTag("symbol:$name"),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.SpaceBetween,
                ) {
                    Box(Modifier.height(30.dp), contentAlignment = Alignment.Center) { Symbol(name, size = 17f) }
                    KitText(name, font = Font(7.5f), color = Theme.muted, lines = 2, align = androidx.compose.ui.text.style.TextAlign.Center)
                }
            }
        }
    }
}

