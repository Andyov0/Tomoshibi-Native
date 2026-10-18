package app.tomoshibi.ui.kit

import android.util.Log
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.automirrored.rounded.ArrowForward
import androidx.compose.material.icons.automirrored.rounded.EventNote
import androidx.compose.material.icons.automirrored.rounded.KeyboardReturn
import androidx.compose.material.icons.automirrored.rounded.ListAlt
import androidx.compose.material.icons.automirrored.rounded.Logout
import androidx.compose.material.icons.automirrored.rounded.ManageSearch
import androidx.compose.material.icons.automirrored.rounded.OpenInNew
import androidx.compose.material.icons.automirrored.rounded.QueueMusic
import androidx.compose.material.icons.automirrored.rounded.ScreenShare
import androidx.compose.material.icons.automirrored.rounded.VolumeDown
import androidx.compose.material.icons.automirrored.rounded.VolumeOff
import androidx.compose.material.icons.automirrored.rounded.VolumeUp
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.AccountCircle
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.AddCircle
import androidx.compose.material.icons.rounded.Apps
import androidx.compose.material.icons.rounded.ArrowUpward
import androidx.compose.material.icons.rounded.AutoAwesome
import androidx.compose.material.icons.rounded.AutoFixHigh
import androidx.compose.material.icons.rounded.BackHand
import androidx.compose.material.icons.rounded.BlurOn
import androidx.compose.material.icons.rounded.CalendarToday
import androidx.compose.material.icons.rounded.CallEnd
import androidx.compose.material.icons.rounded.ChatBubble
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.CheckCircle
import androidx.compose.material.icons.rounded.ChevronLeft
import androidx.compose.material.icons.rounded.ChevronRight
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.CloseFullscreen
import androidx.compose.material.icons.rounded.CloudOff
import androidx.compose.material.icons.rounded.Code
import androidx.compose.material.icons.rounded.ContentCopy
import androidx.compose.material.icons.rounded.ContentPaste
import androidx.compose.material.icons.rounded.CoPresent
import androidx.compose.material.icons.rounded.DeleteOutline
import androidx.compose.material.icons.rounded.Dns
import androidx.compose.material.icons.rounded.DoorFront
import androidx.compose.material.icons.rounded.Error
import androidx.compose.material.icons.rounded.ErrorOutline
import androidx.compose.material.icons.rounded.EventAvailable
import androidx.compose.material.icons.rounded.ExpandLess
import androidx.compose.material.icons.rounded.ExpandMore
import androidx.compose.material.icons.rounded.FastForward
import androidx.compose.material.icons.rounded.Folder
import androidx.compose.material.icons.rounded.Forum
import androidx.compose.material.icons.rounded.Fullscreen
import androidx.compose.material.icons.rounded.FullscreenExit
import androidx.compose.material.icons.rounded.Gesture
import androidx.compose.material.icons.rounded.GraphicEq
import androidx.compose.material.icons.rounded.HighlightOff
import androidx.compose.material.icons.rounded.Home
import androidx.compose.material.icons.rounded.HourglassTop
import androidx.compose.material.icons.rounded.HowToReg
import androidx.compose.material.icons.rounded.Info
import androidx.compose.material.icons.rounded.Key
import androidx.compose.material.icons.rounded.Language
import androidx.compose.material.icons.rounded.Link
import androidx.compose.material.icons.rounded.Lock
import androidx.compose.material.icons.rounded.LockOpen
import androidx.compose.material.icons.rounded.LockPerson
import androidx.compose.material.icons.rounded.MeetingRoom
import androidx.compose.material.icons.rounded.Memory
import androidx.compose.material.icons.rounded.Mic
import androidx.compose.material.icons.rounded.MicNone
import androidx.compose.material.icons.rounded.MicOff
import androidx.compose.material.icons.rounded.MoreHoriz
import androidx.compose.material.icons.rounded.Movie
import androidx.compose.material.icons.rounded.MusicNote
import androidx.compose.material.icons.rounded.NoAccounts
import androidx.compose.material.icons.rounded.NorthEast
import androidx.compose.material.icons.rounded.OndemandVideo
import androidx.compose.material.icons.rounded.OpenInFull
import androidx.compose.material.icons.rounded.Pause
import androidx.compose.material.icons.rounded.Person
import androidx.compose.material.icons.rounded.PersonOutline
import androidx.compose.material.icons.rounded.PersonRemove
import androidx.compose.material.icons.rounded.Photo
import androidx.compose.material.icons.rounded.PhotoCamera
import androidx.compose.material.icons.rounded.PictureInPicture
import androidx.compose.material.icons.rounded.PictureInPictureAlt
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.PlayCircle
import androidx.compose.material.icons.rounded.PresentToAll
import androidx.compose.material.icons.rounded.QuestionAnswer
import androidx.compose.material.icons.rounded.RecordVoiceOver
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.Schedule
import androidx.compose.material.icons.rounded.SdCardAlert
import androidx.compose.material.icons.rounded.Search
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.rounded.SignalWifiStatusbarConnectedNoInternet4
import androidx.compose.material.icons.rounded.SkipNext
import androidx.compose.material.icons.rounded.SmartDisplay
import androidx.compose.material.icons.rounded.Speaker
import androidx.compose.material.icons.rounded.Speed
import androidx.compose.material.icons.rounded.StopCircle
import androidx.compose.material.icons.rounded.Tag
import androidx.compose.material.icons.rounded.Tune
import androidx.compose.material.icons.rounded.UnfoldMore
import androidx.compose.material.icons.rounded.VerticalAlignTop
import androidx.compose.material.icons.rounded.Videocam
import androidx.compose.material.icons.rounded.VideocamOff
import androidx.compose.material.icons.rounded.VideoLibrary
import androidx.compose.material.icons.rounded.VideoSettings
import androidx.compose.material.icons.rounded.Visibility
import androidx.compose.material.icons.rounded.VisibilityOff
import androidx.compose.material.icons.rounded.WarningAmber
import androidx.compose.material.icons.rounded.Waves
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.graphics.vector.VectorGroup
import androidx.compose.ui.graphics.vector.VectorPath
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.tomoshibi.BuildConfig
import app.tomoshibi.basics.Theme

// The SF Symbols the Swift views name, drawn on Android.
//
// The screens are ported keeping the Swift names (`symbol: "mic.fill"`), so
// a view reads the same on both sides and a name changed on one is found on
// the other. Each name is drawn as the nearest Material Rounded icon, the
// set whose round ends sit closest to SF Symbols; the crown, which Material
// does not have, is drawn here.
//
// SwiftUI sizes a symbol by the font it is given, and every SF Symbol has its
// own frame and its own ink inside it at that size: a 13-point xmark is a
// smaller mark than a 13-point mic. A Material icon is a 24-unit square with
// a different margin for each. So the frame and ink of each SF Symbol were
// measured (ImageRenderer and an NSImage drawn at 100 points, macOS 27), and
// the Material glyph is scaled to fill the SF ink and placed in the SF frame:
// a row of a symbol and words takes the room it takes on the iPhone, and a
// symbol is as heavy beside its words.

/**
 * One SF Symbol: what draws it, and its frame and ink in em of the font size.
 * The icon is built the first time it is drawn: building all of them at once
 * would make the first symbol on screen pay for every other.
 */
private class SfSymbol(
    make: () -> ImageVector,
    val frameWidth: Float,
    val frameHeight: Float,
    val inkX: Float,
    val inkY: Float,
    val inkWidth: Float,
    val inkHeight: Float,
) {
    val icon: ImageVector by lazy(make)
}

/** Every SF Symbol named under apple/TomoshibiKit/Sources, with its measure. */
private val symbols: Map<String, SfSymbol> by lazy {
    mapOf(
    "app.badge" to SfSymbol({ Icons.Rounded.Apps }, 1.150f, 1.240f, 0.120f, 0.160f, 1.000f, 0.980f),
    "arrow.clockwise" to SfSymbol({ Icons.Rounded.Refresh }, 1.050f, 1.230f, 0.090f, 0.090f, 0.860f, 1.040f),
    "arrow.down.right.and.arrow.up.left" to SfSymbol({ Icons.Rounded.CloseFullscreen }, 1.170f, 1.120f, 0.120f, 0.100f, 0.940f, 0.920f),
    "arrow.down.right.and.arrow.up.left.rectangle" to SfSymbol({ Icons.Rounded.FullscreenExit }, 1.400f, 1.080f, 0.120f, 0.090f, 1.160f, 0.890f),
    "arrow.right" to SfSymbol({ Icons.AutoMirrored.Rounded.ArrowForward }, 1.150f, 0.930f, 0.120f, 0.100f, 0.920f, 0.730f),
    "arrow.up" to SfSymbol({ Icons.Rounded.ArrowUpward }, 0.990f, 1.110f, 0.120f, 0.090f, 0.760f, 0.920f),
    "arrow.up.left.and.arrow.down.right" to SfSymbol({ Icons.Rounded.OpenInFull }, 1.170f, 1.120f, 0.120f, 0.090f, 0.940f, 0.930f),
    "arrow.up.left.and.arrow.down.right.rectangle" to SfSymbol({ Icons.Rounded.Fullscreen }, 1.400f, 1.080f, 0.120f, 0.090f, 1.160f, 0.890f),
    "arrow.up.right" to SfSymbol({ Icons.Rounded.NorthEast }, 0.970f, 0.910f, 0.120f, 0.090f, 0.740f, 0.720f),
    "arrow.up.right.square" to SfSymbol({ Icons.AutoMirrored.Rounded.OpenInNew }, 1.140f, 1.080f, 0.120f, 0.090f, 0.910f, 0.890f),
    "arrow.up.to.line" to SfSymbol({ Icons.Rounded.VerticalAlignTop }, 0.990f, 1.190f, 0.120f, 0.090f, 0.760f, 1.000f),
    "bubble.left.and.bubble.right.fill" to SfSymbol({ Icons.Rounded.QuestionAnswer }, 1.650f, 1.370f, 0.120f, 0.090f, 1.420f, 1.120f),
    "bubble.left.and.text.bubble.right" to SfSymbol({ Icons.Rounded.Forum }, 1.650f, 1.300f, 0.120f, 0.090f, 1.420f, 1.110f),
    "bubble.left.fill" to SfSymbol({ Icons.Rounded.ChatBubble }, 1.320f, 1.200f, 0.120f, 0.090f, 1.090f, 1.010f),
    "calendar" to SfSymbol({ Icons.Rounded.CalendarToday }, 1.220f, 1.080f, 0.120f, 0.090f, 0.990f, 0.890f),
    "calendar.badge.clock" to SfSymbol({ Icons.AutoMirrored.Rounded.EventNote }, 1.220f, 1.240f, 0.120f, 0.090f, 1.190f, 0.990f),
    "calendar.badge.plus" to SfSymbol({ Icons.Rounded.EventAvailable }, 1.220f, 1.240f, 0.120f, 0.090f, 1.190f, 0.990f),
    "camera.fill" to SfSymbol({ Icons.Rounded.PhotoCamera }, 1.440f, 1.120f, 0.120f, 0.100f, 1.210f, 0.920f),
    "checkmark" to SfSymbol({ Icons.Rounded.Check }, 1.090f, 1.020f, 0.120f, 0.090f, 0.850f, 0.830f),
    "checkmark.circle.fill" to SfSymbol({ Icons.Rounded.CheckCircle }, 1.180f, 1.180f, 0.090f, 0.090f, 0.990f, 0.990f),
    "chevron.down" to SfSymbol({ Icons.Rounded.ExpandMore }, 1.090f, 0.660f, 0.120f, 0.090f, 0.860f, 0.470f),
    "chevron.left" to SfSymbol({ Icons.Rounded.ChevronLeft }, 0.730f, 1.030f, 0.060f, 0.090f, 0.480f, 0.840f),
    "chevron.left.forwardslash.chevron.right" to SfSymbol({ Icons.Rounded.Code }, 1.610f, 1.120f, 0.120f, 0.100f, 1.380f, 0.920f),
    "chevron.right" to SfSymbol({ Icons.Rounded.ChevronRight }, 0.730f, 1.030f, 0.190f, 0.090f, 0.480f, 0.840f),
    "chevron.up" to SfSymbol({ Icons.Rounded.ExpandLess }, 1.090f, 0.660f, 0.120f, 0.090f, 0.860f, 0.470f),
    "chevron.up.chevron.down" to SfSymbol({ Icons.Rounded.UnfoldMore }, 0.890f, 1.130f, 0.120f, 0.100f, 0.660f, 0.930f),
    "clock" to SfSymbol({ Icons.Rounded.Schedule }, 1.180f, 1.180f, 0.090f, 0.090f, 0.990f, 0.990f),
    "cpu" to SfSymbol({ Icons.Rounded.Memory }, 1.230f, 1.170f, 0.120f, 0.090f, 1.000f, 0.980f),
    "crown" to SfSymbol({ CrownOutline }, 1.500f, 1.140f, 0.120f, 0.090f, 1.270f, 0.950f),
    "crown.fill" to SfSymbol({ Crown }, 1.500f, 1.140f, 0.120f, 0.090f, 1.270f, 0.950f),
    "doc.on.doc" to SfSymbol({ Icons.Rounded.ContentCopy }, 1.240f, 1.410f, 0.120f, 0.090f, 1.010f, 1.220f),
    // The paste control's, measured as the others; its frame and ink are doc.on.doc's.
    "doc.on.clipboard" to SfSymbol({ Icons.Rounded.ContentPaste }, 1.240f, 1.410f, 0.120f, 0.090f, 1.010f, 1.220f),
    "door.left.hand.closed" to SfSymbol({ Icons.Rounded.DoorFront }, 0.990f, 1.220f, 0.120f, 0.090f, 0.760f, 1.030f),
    "door.left.hand.open" to SfSymbol({ Icons.Rounded.MeetingRoom }, 0.990f, 1.220f, 0.120f, 0.090f, 0.760f, 1.030f),
    "ellipsis" to SfSymbol({ Icons.Rounded.MoreHoriz }, 1.100f, 0.370f, 0.090f, 0.090f, 0.900f, 0.180f),
    "exclamationmark.circle" to SfSymbol({ Icons.Rounded.ErrorOutline }, 1.180f, 1.180f, 0.090f, 0.090f, 0.990f, 0.990f),
    "exclamationmark.circle.fill" to SfSymbol({ Icons.Rounded.Error }, 1.180f, 1.180f, 0.090f, 0.090f, 0.990f, 0.990f),
    "exclamationmark.triangle" to SfSymbol({ Icons.Rounded.WarningAmber }, 1.230f, 1.110f, 0.100f, 0.090f, 1.030f, 0.920f),
    "externaldrive.badge.exclamationmark" to SfSymbol({ Icons.Rounded.SdCardAlert }, 1.400f, 1.160f, 0.060f, 0.090f, 1.340f, 0.910f),
    "eye" to SfSymbol({ Icons.Rounded.Visibility }, 1.540f, 1.020f, 0.100f, 0.090f, 1.340f, 0.830f),
    "eye.fill" to SfSymbol({ Icons.Rounded.Visibility }, 1.530f, 1.010f, 0.100f, 0.090f, 1.330f, 0.820f),
    "eye.slash" to SfSymbol({ Icons.Rounded.VisibilityOff }, 1.540f, 1.230f, 0.100f, 0.170f, 1.340f, 0.890f),
    "film" to SfSymbol({ Icons.Rounded.Movie }, 1.400f, 1.080f, 0.120f, 0.090f, 1.160f, 0.890f),
    "film.stack" to SfSymbol({ Icons.Rounded.VideoLibrary }, 1.290f, 1.340f, 0.120f, 0.090f, 1.030f, 1.070f),
    "folder.fill" to SfSymbol({ Icons.Rounded.Folder }, 1.350f, 1.080f, 0.120f, 0.090f, 1.120f, 0.890f),
    "forward.end.fill" to SfSymbol({ Icons.Rounded.SkipNext }, 1.150f, 0.930f, 0.120f, 0.090f, 0.920f, 0.740f),
    "forward.fill" to SfSymbol({ Icons.Rounded.FastForward }, 1.470f, 0.930f, 0.120f, 0.090f, 1.340f, 0.740f),
    "gauge.with.dots.needle.33percent" to SfSymbol({ Icons.Rounded.Speed }, 1.180f, 1.180f, 0.090f, 0.090f, 0.990f, 0.990f),
    "gearshape" to SfSymbol({ Icons.Rounded.Settings }, 1.230f, 1.230f, 0.090f, 0.090f, 1.040f, 1.040f),
    "gearshape.fill" to SfSymbol({ Icons.Rounded.Settings }, 1.200f, 1.210f, 0.090f, 0.100f, 1.010f, 1.010f),
    "globe" to SfSymbol({ Icons.Rounded.Language }, 1.180f, 1.180f, 0.090f, 0.090f, 0.990f, 0.990f),
    "hand.raised.fill" to SfSymbol({ Icons.Rounded.BackHand }, 1.180f, 1.230f, 0.240f, 0.090f, 0.790f, 1.040f),
    "hifispeaker" to SfSymbol({ Icons.Rounded.Speaker }, 0.990f, 1.200f, 0.120f, 0.100f, 0.760f, 1.000f),
    "hourglass" to SfSymbol({ Icons.Rounded.HourglassTop }, 0.820f, 1.080f, 0.120f, 0.090f, 0.590f, 0.890f),
    "house" to SfSymbol({ Icons.Rounded.Home }, 1.410f, 1.210f, 0.120f, 0.090f, 1.180f, 1.020f),
    "icloud.slash" to SfSymbol({ Icons.Rounded.CloudOff }, 1.410f, 1.230f, 0.090f, 0.090f, 1.220f, 0.980f),
    "info.circle" to SfSymbol({ Icons.Rounded.Info }, 1.180f, 1.180f, 0.090f, 0.090f, 0.990f, 0.990f),
    "info.circle.fill" to SfSymbol({ Icons.Rounded.Info }, 1.180f, 1.180f, 0.090f, 0.090f, 0.990f, 0.990f),
    "key" to SfSymbol({ Icons.Rounded.Key }, 0.900f, 1.410f, 0.120f, 0.090f, 0.670f, 1.220f),
    "key.fill" to SfSymbol({ Icons.Rounded.Key }, 0.870f, 1.390f, 0.120f, 0.090f, 0.640f, 1.200f),
    "link" to SfSymbol({ Icons.Rounded.Link }, 1.260f, 1.280f, 0.110f, 0.120f, 1.040f, 1.030f),
    "list.bullet.rectangle" to SfSymbol({ Icons.AutoMirrored.Rounded.ListAlt }, 1.400f, 1.080f, 0.120f, 0.090f, 1.160f, 0.890f),
    "lock" to SfSymbol({ Icons.Rounded.LockOpen }, 0.950f, 1.140f, 0.140f, 0.090f, 0.660f, 0.950f),
    "lock.fill" to SfSymbol({ Icons.Rounded.Lock }, 0.950f, 1.140f, 0.140f, 0.090f, 0.660f, 0.950f),
    "magnifyingglass" to SfSymbol({ Icons.Rounded.Search }, 1.180f, 1.150f, 0.110f, 0.090f, 0.960f, 0.960f),
    "mic" to SfSymbol({ Icons.Rounded.MicNone }, 1.000f, 1.230f, 0.150f, 0.090f, 0.700f, 1.040f),
    "mic.fill" to SfSymbol({ Icons.Rounded.Mic }, 1.000f, 1.220f, 0.150f, 0.090f, 0.700f, 1.030f),
    "mic.slash.fill" to SfSymbol({ Icons.Rounded.MicOff }, 1.000f, 1.220f, 0.080f, 0.090f, 0.840f, 1.030f),
    // Named by MusicPanelView.swift's empty search, but no such SF Symbol
    // exists (not in macOS 27's list), so the Apple apps draw an empty
    // circle there. Drawn here as what it was meant to be, a search among
    // songs, in magnifyingglass's measure.
    "music.magnifyingglass" to SfSymbol({ Icons.AutoMirrored.Rounded.ManageSearch }, 1.180f, 1.150f, 0.110f, 0.090f, 0.960f, 0.960f),
    "music.note" to SfSymbol({ Icons.Rounded.MusicNote }, 0.840f, 1.170f, 0.120f, 0.100f, 0.610f, 0.970f),
    "music.note.list" to SfSymbol({ Icons.AutoMirrored.Rounded.QueueMusic }, 1.240f, 1.170f, 0.120f, 0.100f, 1.010f, 0.970f),
    "number" to SfSymbol({ Icons.Rounded.Tag }, 1.120f, 1.140f, 0.120f, 0.100f, 0.890f, 0.940f),
    "pause.fill" to SfSymbol({ Icons.Rounded.Pause }, 0.840f, 0.990f, 0.120f, 0.090f, 0.610f, 0.800f),
    "person" to SfSymbol({ Icons.Rounded.PersonOutline }, 1.100f, 1.090f, 0.120f, 0.090f, 0.870f, 0.900f),
    "person.and.background.dotted" to SfSymbol({ Icons.Rounded.BlurOn }, 1.400f, 1.180f, 0.120f, 0.100f, 1.170f, 0.910f),
    "person.badge.minus" to SfSymbol({ Icons.Rounded.PersonRemove }, 1.100f, 1.320f, 0.120f, 0.090f, 1.100f, 1.070f),
    "person.crop.circle.badge.exclamationmark" to SfSymbol({ Icons.Rounded.LockPerson }, 1.180f, 1.250f, 0.060f, 0.090f, 1.190f, 1.000f),
    "person.crop.circle.badge.xmark" to SfSymbol({ Icons.Rounded.NoAccounts }, 1.180f, 1.250f, 0.060f, 0.090f, 1.190f, 1.000f),
    "person.crop.circle.fill" to SfSymbol({ Icons.Rounded.AccountCircle }, 1.180f, 1.180f, 0.090f, 0.090f, 0.990f, 0.990f),
    "person.fill" to SfSymbol({ Icons.Rounded.Person }, 1.070f, 1.060f, 0.120f, 0.090f, 0.830f, 0.870f),
    "person.fill.checkmark" to SfSymbol({ Icons.Rounded.HowToReg }, 1.440f, 1.060f, 0.120f, 0.090f, 1.210f, 0.870f),
    "person.wave.2.fill" to SfSymbol({ Icons.Rounded.RecordVoiceOver }, 1.250f, 1.210f, 0.120f, 0.100f, 1.120f, 1.010f),
    "phone.down.fill" to SfSymbol({ Icons.Rounded.CallEnd }, 1.340f, 0.600f, 0.090f, 0.090f, 1.150f, 0.410f),
    "photo" to SfSymbol({ Icons.Rounded.Photo }, 1.400f, 1.080f, 0.120f, 0.090f, 1.160f, 0.890f),
    "pip.enter" to SfSymbol({ Icons.Rounded.PictureInPictureAlt }, 1.510f, 1.210f, 0.120f, 0.090f, 1.280f, 1.020f),
    "pip.exit" to SfSymbol({ Icons.Rounded.PictureInPicture }, 1.510f, 1.210f, 0.120f, 0.090f, 1.280f, 1.020f),
    "play.circle.fill" to SfSymbol({ Icons.Rounded.PlayCircle }, 1.180f, 1.180f, 0.090f, 0.090f, 0.990f, 0.990f),
    "play.fill" to SfSymbol({ Icons.Rounded.PlayArrow }, 0.870f, 1.000f, 0.120f, 0.090f, 0.740f, 0.810f),
    "play.rectangle" to SfSymbol({ Icons.Rounded.OndemandVideo }, 1.400f, 1.080f, 0.120f, 0.090f, 1.160f, 0.890f),
    "play.rectangle.fill" to SfSymbol({ Icons.Rounded.SmartDisplay }, 1.400f, 1.080f, 0.120f, 0.090f, 1.160f, 0.890f),
    "plus" to SfSymbol({ Icons.Rounded.Add }, 1.050f, 0.990f, 0.120f, 0.090f, 0.820f, 0.800f),
    "plus.circle.fill" to SfSymbol({ Icons.Rounded.AddCircle }, 1.180f, 1.180f, 0.090f, 0.090f, 0.990f, 0.990f),
    "rectangle.inset.filled.and.person.filled" to SfSymbol({ Icons.Rounded.CoPresent }, 1.410f, 1.150f, 0.120f, 0.090f, 1.370f, 0.900f),
    "rectangle.inset.filled.on.rectangle" to SfSymbol({ Icons.Rounded.PresentToAll }, 1.460f, 1.170f, 0.120f, 0.090f, 1.230f, 0.980f),
    "rectangle.on.rectangle" to SfSymbol({ Icons.AutoMirrored.Rounded.ScreenShare }, 1.460f, 1.170f, 0.120f, 0.090f, 1.230f, 0.980f),
    "rectangle.portrait.and.arrow.right" to SfSymbol({ Icons.AutoMirrored.Rounded.Logout }, 1.390f, 1.230f, 0.120f, 0.090f, 1.140f, 1.040f),
    "return" to SfSymbol({ Icons.AutoMirrored.Rounded.KeyboardReturn }, 1.280f, 1.080f, 0.120f, 0.090f, 1.050f, 0.890f),
    "server.rack" to SfSymbol({ Icons.Rounded.Dns }, 1.400f, 1.080f, 0.120f, 0.090f, 1.160f, 0.890f),
    "signature" to SfSymbol({ Icons.Rounded.Gesture }, 1.710f, 1.260f, 0.120f, 0.090f, 1.480f, 1.070f),
    "slider.horizontal.3" to SfSymbol({ Icons.Rounded.Tune }, 1.230f, 1.010f, 0.120f, 0.090f, 1.000f, 0.820f),
    "sparkles" to SfSymbol({ Icons.Rounded.AutoAwesome }, 1.110f, 1.300f, 0.100f, 0.090f, 0.910f, 1.110f),
    // Material's HQ has letters cut out of it, which close up at the size a
    // settings row draws it; a picture with its settings reads at any size.
    "sparkles.tv" to SfSymbol({ Icons.Rounded.VideoSettings },1.470f, 1.160f, 0.120f, 0.090f, 1.230f, 0.970f),
    "speaker.slash.fill" to SfSymbol({ Icons.AutoMirrored.Rounded.VolumeOff }, 1.000f, 1.230f, 0.060f, 0.150f, 0.920f, 0.920f),
    "speaker.wave.1.fill" to SfSymbol({ Icons.AutoMirrored.Rounded.VolumeDown }, 1.170f, 1.050f, 0.150f, 0.100f, 0.870f, 0.850f),
    "speaker.wave.2" to SfSymbol({ Icons.AutoMirrored.Rounded.VolumeUp }, 1.390f, 1.050f, 0.150f, 0.100f, 1.090f, 0.850f),
    "speaker.wave.2.fill" to SfSymbol({ Icons.AutoMirrored.Rounded.VolumeUp }, 1.390f, 1.050f, 0.150f, 0.100f, 1.090f, 0.850f),
    "stop.circle" to SfSymbol({ Icons.Rounded.StopCircle }, 1.180f, 1.180f, 0.090f, 0.090f, 0.990f, 0.990f),
    "trash" to SfSymbol({ Icons.Rounded.DeleteOutline }, 1.090f, 1.270f, 0.070f, 0.090f, 0.960f, 1.080f),
    "video" to SfSymbol({ Icons.Rounded.Videocam }, 1.430f, 0.990f, 0.150f, 0.090f, 1.210f, 0.800f),
    "video.fill" to SfSymbol({ Icons.Rounded.Videocam }, 1.420f, 0.980f, 0.150f, 0.090f, 1.210f, 0.790f),
    "video.slash.fill" to SfSymbol({ Icons.Rounded.VideocamOff }, 1.420f, 1.380f, 0.070f, 0.170f, 1.310f, 1.040f),
    "wand.and.stars" to SfSymbol({ Icons.Rounded.AutoFixHigh }, 1.240f, 1.150f, 0.110f, 0.090f, 0.960f, 0.960f),
    "waveform" to SfSymbol({ Icons.Rounded.GraphicEq }, 1.140f, 1.210f, 0.120f, 0.100f, 0.910f, 1.010f),
    "waveform.circle.fill" to SfSymbol({ Icons.Rounded.GraphicEq }, 1.180f, 1.180f, 0.090f, 0.090f, 0.990f, 0.990f),
    "waveform.path" to SfSymbol({ Icons.Rounded.Waves }, 1.400f, 1.320f, 0.100f, 0.110f, 1.200f, 1.100f),
    "wifi.exclamationmark" to SfSymbol({ Icons.Rounded.SignalWifiStatusbarConnectedNoInternet4 }, 1.330f, 1.200f, 0.120f, 0.170f, 1.080f, 0.860f),
    "xmark" to SfSymbol({ Icons.Rounded.Close }, 1.030f, 0.970f, 0.120f, 0.100f, 0.780f, 0.770f),
    "xmark.circle" to SfSymbol({ Icons.Rounded.HighlightOff }, 1.180f, 1.180f, 0.090f, 0.090f, 0.990f, 0.990f),
    )
}

/** The names the kit can draw, for a test to hold the Swift sources to. */
val symbolNames: Set<String> get() = symbols.keys

/**
 * The Material icon an SF Symbol name is drawn as. A name with no icon is a
 * fault in the port, and a debug build stops on it rather than drawing a
 * blank where a control should be; a release build draws nothing there.
 */
fun symbol(name: String): ImageVector = lookup(name).icon

private val missing = SfSymbol({ ImageVector.Builder("missing", 24.dp, 24.dp, 24f, 24f).build() },
    1f, 1f, 0f, 0f, 1f, 1f)

private fun lookup(name: String): SfSymbol {
    symbols[name]?.let { return it }
    check(!BuildConfig.DEBUG) { "No Android symbol for the SF Symbol \"$name\": add it to Symbols.kt" }
    Log.w("Tomoshibi", "no Android symbol for the SF Symbol \"$name\"")
    return missing
}

/**
 * A symbol at a font size, as `Image(systemName:).font(.system(size:weight:))`
 * draws it: in the SF Symbol's own frame, in the inherited ink. Hidden from
 * a screen reader unless it is given words, as an image inside a labelled
 * button is.
 */
@Composable
fun Symbol(
    name: String,
    modifier: Modifier = Modifier,
    size: Float = LocalFont.current.size,
    weight: FontWeight = LocalFont.current.weight,
    tint: Color = LocalInk.current,
    contentDescription: String? = null,
) {
    val spec = lookup(name)
    val vector = remember(spec, weight) { weighted(spec.icon, weight) }
    val fit = remember(spec) { fit(spec) }
    val painter = rememberVectorPainter(vector)
    val filter = remember(tint) { ColorFilter.tint(tint) }
    val described = if (contentDescription == null) Modifier else Modifier.semantics {
        this.contentDescription = contentDescription
        role = Role.Image
    }
    Spacer(
        modifier
            .then(described)
            .size((fit.width * size).dp, (fit.height * size).dp)
            .drawBehind {
                if (fit.scale <= 0f) return@drawBehind
                val em = size.dp.toPx()
                val scale = fit.scale * em
                translate(fit.left * em, fit.top * em) {
                    with(painter) {
                        draw(Size(spec.icon.viewportWidth * scale, spec.icon.viewportHeight * scale), colorFilter = filter)
                    }
                }
            },
    )
}

/**
 * Where an icon is drawn for its SF Symbol, in em of the font size: the frame
 * it takes, the em per viewport unit it is drawn at, and where its viewport's
 * corner goes.
 */
private class Fit(val width: Float, val height: Float, val scale: Float, val left: Float, val top: Float)

/**
 * The icon's ink fitted to the SF Symbol's: inside it where the two are the
 * same shape. Where they are not (SF's key stands upright, Material's lies on
 * its side), fitting inside would draw a speck; the icon is given the SF
 * ink's area instead, centred where the SF ink is, and the frame grows to
 * hold it so that it does not run into the words beside it.
 */
private fun fit(spec: SfSymbol): Fit {
    val ink = inkBounds(spec.icon)
    if (ink.isEmpty) return Fit(spec.frameWidth, spec.frameHeight, 0f, 0f, 0f)
    val sfAspect = spec.inkWidth / spec.inkHeight
    val iconAspect = ink.width / ink.height
    val unlike = maxOf(sfAspect, iconAspect) / minOf(sfAspect, iconAspect) > 1.5f
    val scale = if (unlike) {
        kotlin.math.sqrt(spec.inkWidth * spec.inkHeight / (ink.width * ink.height))
    } else {
        minOf(spec.inkWidth / ink.width, spec.inkHeight / ink.height)
    }
    val drawnWidth = ink.width * scale
    val drawnHeight = ink.height * scale
    val width = maxOf(spec.frameWidth, drawnWidth + (spec.frameWidth - spec.inkWidth))
    val height = maxOf(spec.frameHeight, drawnHeight + (spec.frameHeight - spec.inkHeight))
    val centreX = spec.inkX + spec.inkWidth / 2 + (width - spec.frameWidth) / 2
    val centreY = spec.inkY + spec.inkHeight / 2 + (height - spec.frameHeight) / 2
    return Fit(width, height, scale, centreX - drawnWidth / 2 - ink.left * scale, centreY - drawnHeight / 2 - ink.top * scale)
}

/**
 * A symbol that changes with what it stands for (a microphone turned off,
 * a link copied): the old one shrinks away as the new one grows in, as
 * `.contentTransition(.symbolEffect(.replace))` does, rather than one
 * swapping for the other between two frames.
 */
@Composable
fun SwappingSymbol(
    name: String,
    modifier: Modifier = Modifier,
    size: Float = LocalFont.current.size,
    weight: FontWeight = LocalFont.current.weight,
    tint: Color = LocalInk.current,
    contentDescription: String? = null,
) {
    AnimatedContent(
        targetState = name,
        modifier = modifier,
        transitionSpec = {
            (fadeIn(Theme.settle()) + scaleIn(Theme.settle(), initialScale = 0.6f)) togetherWith
                (fadeOut(Theme.leave()) + scaleOut(Theme.leave(), targetScale = 0.6f))
        },
        label = "symbol",
    ) { shown ->
        Symbol(shown, size = size, weight = weight, tint = tint, contentDescription = contentDescription)
    }
}

/**
 * The icon drawn heavier for a heavier font. SF Symbols come in nine
 * weights and the Swift views ask for semibold and bold on most controls;
 * a Material icon has one, and drawn at it every control looked thin beside
 * its words. Each shape is outlined in its own colour by a width that grows
 * with the weight, which thickens every stroke of it evenly.
 */
private fun weighted(icon: ImageVector, weight: FontWeight): ImageVector {
    val width = when {
        weight.weight >= 700 -> 0.6f
        weight.weight >= 600 -> 0.4f
        weight.weight >= 500 -> 0.2f
        else -> return icon
    }
    val builder = ImageVector.Builder(
        name = icon.name + "@" + weight.weight,
        defaultWidth = icon.defaultWidth,
        defaultHeight = icon.defaultHeight,
        viewportWidth = icon.viewportWidth,
        viewportHeight = icon.viewportHeight,
        autoMirror = icon.autoMirror,
    )
    fun copy(group: VectorGroup) {
        for (node in group) {
            when (node) {
                is VectorGroup -> {
                    builder.addGroup(node.name, node.rotation, node.pivotX, node.pivotY, node.scaleX, node.scaleY,
                        node.translationX, node.translationY, node.clipPathData)
                    copy(node)
                    builder.clearGroup()
                }
                is VectorPath -> builder.addPath(
                    pathData = node.pathData,
                    pathFillType = node.pathFillType,
                    name = node.name,
                    fill = node.fill,
                    fillAlpha = node.fillAlpha,
                    stroke = node.stroke ?: node.fill ?: SolidColor(Color.Black),
                    strokeAlpha = if (node.stroke != null) node.strokeAlpha else node.fillAlpha,
                    strokeLineWidth = node.strokeLineWidth + width,
                    strokeLineCap = if (node.stroke != null) node.strokeLineCap else StrokeCap.Round,
                    strokeLineJoin = if (node.stroke != null) node.strokeLineJoin else StrokeJoin.Round,
                    strokeLineMiter = node.strokeLineMiter,
                )
            }
        }
    }
    copy(icon.root)
    return builder.build()
}

private val boundsCache = HashMap<ImageVector, Rect>()

/** Where an icon's ink is, in its own viewport units. */
private fun inkBounds(icon: ImageVector): Rect = synchronized(boundsCache) {
    boundsCache.getOrPut(icon) {
        var bounds: Rect? = null
        fun visit(group: VectorGroup) {
            for (node in group) {
                when (node) {
                    is VectorGroup -> visit(node)
                    is VectorPath -> {
                        val path = PathParser().addPathNodes(node.pathData).toPath().getBounds()
                        // A stroked path's ink reaches half its width past the line.
                        val half = if (node.stroke != null) node.strokeLineWidth / 2 else 0f
                        val inked = Rect(path.left - half, path.top - half, path.right + half, path.bottom + half)
                        bounds = bounds?.let {
                            Rect(minOf(it.left, inked.left), minOf(it.top, inked.top),
                                maxOf(it.right, inked.right), maxOf(it.bottom, inked.bottom))
                        } ?: inked
                    }
                }
            }
        }
        visit(icon.root)
        bounds ?: Rect.Zero
    }
}

/**
 * SF Symbols' `crown.fill`, which Material has no icon for: the host's mark,
 * on the button that opens the room's controls and over the panel they are
 * in. Three points with a bead on each over a band, round at every corner as
 * the Rounded set is.
 */
private val Crown: ImageVector by lazy {
    ImageVector.Builder("crown.fill", 24.dp, 24.dp, 24f, 24f).apply {
        path(fill = SolidColor(Color.Black), stroke = SolidColor(Color.Black), strokeLineWidth = 1.4f,
            strokeLineJoin = StrokeJoin.Round) {
            moveTo(5.2f, 16.2f)
            lineTo(3.4f, 8.6f)
            lineTo(8.3f, 11.6f)
            lineTo(12f, 5.6f)
            lineTo(15.7f, 11.6f)
            lineTo(20.6f, 8.6f)
            lineTo(18.8f, 16.2f)
            close()
        }
        path(fill = SolidColor(Color.Black)) {
            moveTo(6.2f, 17.6f)
            lineTo(17.8f, 17.6f)
            arcTo(1.1f, 1.1f, 0f, false, true, 17.8f, 19.8f)
            lineTo(6.2f, 19.8f)
            arcTo(1.1f, 1.1f, 0f, false, true, 6.2f, 17.6f)
            close()
        }
        bead(3.3f, 7.4f, 1.45f)
        bead(12f, 4.5f, 1.45f)
        bead(20.7f, 7.4f, 1.45f)
    }.build()
}

/** SF Symbols' `crown`: the same crown drawn as its outline. */
private val CrownOutline: ImageVector by lazy {
    ImageVector.Builder("crown", 24.dp, 24.dp, 24f, 24f).apply {
        path(stroke = SolidColor(Color.Black), strokeLineWidth = 1.7f, strokeLineJoin = StrokeJoin.Round,
            strokeLineCap = StrokeCap.Round) {
            moveTo(5.4f, 15.8f)
            lineTo(3.8f, 9.2f)
            lineTo(8.4f, 12.1f)
            lineTo(12f, 6.3f)
            lineTo(15.6f, 12.1f)
            lineTo(20.2f, 9.2f)
            lineTo(18.6f, 15.8f)
            close()
            moveTo(6.4f, 18.9f)
            lineTo(17.6f, 18.9f)
        }
        bead(3.4f, 7.5f, 1.25f)
        bead(12f, 4.6f, 1.25f)
        bead(20.6f, 7.5f, 1.25f)
    }.build()
}

private fun ImageVector.Builder.bead(x: Float, y: Float, radius: Float) {
    path(fill = SolidColor(Color.Black)) {
        moveTo(x - radius, y)
        arcTo(radius, radius, 0f, true, true, x + radius, y)
        arcTo(radius, radius, 0f, true, true, x - radius, y)
        close()
    }
}
