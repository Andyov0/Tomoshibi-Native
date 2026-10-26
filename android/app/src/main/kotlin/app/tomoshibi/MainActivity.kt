package app.tomoshibi

import android.content.Context
import android.content.Intent
import android.graphics.Color as AndroidColor
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.lifecycle.lifecycleScope
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.Localization
import app.tomoshibi.core.CoreNative
import app.tomoshibi.core.TestEncoders
import app.tomoshibi.media.DeviceAccess
import app.tomoshibi.media.Foreground
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.DebugLaunch
import app.tomoshibi.ui.TomoshibiApp
import app.tomoshibi.ui.lobby.LobbyDebug
import kotlinx.coroutines.launch

private const val TAG = "Tomoshibi"

/**
 * The app's one activity: loads the core, keeps the app's model for the life
 * of the process, and draws the app (ui/App.kt). It turns round for itself
 * (the manifest's configChanges), so that a Fold opened or closed in a call
 * keeps the call, the composition and everything a screen has open.
 *
 * A debug build can be driven from adb as the Apple apps are from their
 * environment (TOMOSHIBI_*):
 *
 *     adb shell am start -n app.tomoshibi.android/app.tomoshibi.MainActivity \
 *         -e base http://127.0.0.1:8080 -e name uitest -e pass … \
 *         -e autojoin some-room --ez mute true
 *
 * `base` names the deployment (only a debug build honours it; a release build
 * is held to the home deployment), `name` and `pass` sign in, `autojoin`
 * goes straight into a room, `prepare` stops at its join screen, `open` opens
 * a link, `passphrase` and `seal` are typed, `mic` joins with the microphone
 * on, `mute` plays nothing and only measures. A test's launch keeps nothing.
 * Once running, `-e do <command>` on a further start does what a press would:
 * `leave`, `back`, `join <room>`, `prepare <room>`, `enter <room>`,
 * `signout`, `mic`, `camera`, `hand`, `say <text>`, `invite [name]`, `knock`,
 * `cancel`, `state`. `lobby` opens one part of the lobby (`scheduled`,
 * `servers`, `start`, `join`, `again`, `settings`), `section` the section the
 * settings open at (`account`, `media`, ...), `room` is typed into the
 * start sheet's field, and `popover` opens one of a screen's popovers by its
 * id, as TOMOSHIBI_DEBUG_LOBBY, _ROOM and _POPOVER do on Apple.
 */
class MainActivity : ComponentActivity() {
    private lateinit var app: AppModel

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // An emulator has no hardware H.264 and the prebuilt WebRTC no
        // software one, so nothing it sent ever reached a browser; a debug
        // build there encodes with the platform's software codec instead
        // (TestEncoders), set before the first call makes the encoders.
        if (BuildConfig.DEBUG && Build.HARDWARE in setOf("ranchu", "goldfish")) TestEncoders.softwareH264 = true
        CoreNative.load(this)
        Localization.install(this)
        Appearance.install(this)
        DeviceAccess.install(this)
        // From edge to edge, the bars clear: every screen keeps its controls
        // inside the safe area, the display cutout and the keyboard included,
        // and lets its pictures and its backdrop run under the bars. Their
        // icons start light, as over the dark, and the app's root turns them
        // dark over a light screen from its first frame (ui/App.kt
        // `SystemBars`).
        enableEdgeToEdge(SystemBarStyle.dark(AndroidColor.TRANSPARENT), SystemBarStyle.dark(AndroidColor.TRANSPARENT))
        // The window gives way to the keyboard rather than being pushed up
        // under it, so that the screens can keep a field and its button
        // above it themselves (safeDrawing carries the keyboard's inset).
        // Deprecated as a resize, which an edge-to-edge window does not do,
        // and still what keeps the system from panning the window instead.
        @Suppress("DEPRECATION")
        window.setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE)
        app = Shell.app(this, intent)
        if (BuildConfig.DEBUG) Debug.lobby(intent)
        setContent { TomoshibiApp(app) }
        lifecycleScope.launch { app.start() }
        if (savedInstanceState == null) handle(intent, first = true)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handle(intent, first = false)
    }

    override fun onStart() {
        super.onStart()
        app.call?.setBackground(false)
    }

    override fun onResume() {
        super.onResume()
        // The update's system pages are opened from here (media/SelfInstall.kt),
        // and coming back from the one that allows installs is how the app
        // learns whether it was allowed.
        Foreground.resumed(this)
        app.updates?.resumed()
        // Back in front: what the deployment says may have changed while the
        // app was behind another (Remote `refresh`, which holds back a
        // question asked a moment ago).
        app.remote.refresh()
    }

    override fun onPause() {
        Foreground.paused(this)
        super.onPause()
    }

    override fun onStop() {
        // Readings taken in the background say nothing about a freeze (the
        // web skips a hidden tab).
        app.call?.setBackground(true)
        super.onStop()
    }

    override fun onDestroy() {
        // Finished for good, not turned round: the call goes with it. With no
        // service to hold it, a call left behind would go on in the room with
        // its microphone cut off by the system, a participant who is there
        // and silent.
        if (isFinishing && !isChangingConfigurations) {
            Shell.process.launch { app.call?.let { app.leave() } }
        }
        super.onDestroy()
    }

    private fun handle(intent: Intent, first: Boolean) {
        intent.dataString?.takeIf { intent.action == Intent.ACTION_VIEW }?.let { link ->
            lifecycleScope.launch { app.open(link) }
        }
        if (!BuildConfig.DEBUG) return
        val command = intent.getStringExtra("do") ?: return
        if (first && intent.getStringExtra("base") != null) return
        lifecycleScope.launch { Debug.perform(app, command) }
    }
}

/** The app's model, one for the process however many times the activity is made. */
object Shell {
    private var model: AppModel? = null

    /** Work that outlives the activity that started it: leaving a call as the app closes. */
    val process = kotlinx.coroutines.CoroutineScope(kotlinx.coroutines.SupervisorJob() + kotlinx.coroutines.Dispatchers.Main.immediate)

    fun app(context: Context, intent: Intent): AppModel =
        model ?: AppModel.create(context.applicationContext, if (BuildConfig.DEBUG) Debug.launch(intent) else null) { keeps ->
            Appearance.shared.keeps = keeps
            Localization.shared.keeps = keeps
        }.also { model = it }
}

/** A debug build's switches, from the launch's extras and later starts' `do`. */
private object Debug {
    fun launch(intent: Intent): DebugLaunch? {
        val base = intent.getStringExtra("base") ?: return null
        return DebugLaunch(
            base = base,
            name = intent.getStringExtra("name"),
            pass = intent.getStringExtra("pass"),
            passphrase = intent.getStringExtra("passphrase"),
            sealingWord = intent.getStringExtra("seal"),
            autojoin = intent.getStringExtra("autojoin"),
            prepare = intent.getStringExtra("prepare"),
            open = intent.getStringExtra("open"),
            microphone = intent.getBooleanExtra("mic", false),
            mute = intent.getBooleanExtra("mute", false),
            ended = intent.getStringExtra("ended"),
        )
    }

    /** The lobby's debug switches (ui/lobby `LobbyDebug`), from the first launch's extras. */
    fun lobby(intent: Intent) {
        LobbyDebug.open = intent.getStringExtra("lobby")
        LobbyDebug.room = intent.getStringExtra("room")
        LobbyDebug.popover = intent.getStringExtra("popover")
        LobbyDebug.section = intent.getStringExtra("section")
    }

    suspend fun perform(app: AppModel, line: String) {
        val word = line.substringBefore(' ')
        val rest = line.substringAfter(' ', "")
        Log.i(TAG, "debug: $word")
        when (word) {
            "leave" -> app.leave()
            "back" -> app.backToLobby()
            "join" -> {
                if (rest.isNotEmpty()) app.prepare(rest)
                app.join()
            }
            "prepare" -> app.prepare(rest)
            "enter" -> app.enter(rest)
            "signout" -> app.signOut()
            "mic" -> app.toggleMicrophone()
            "camera" -> app.call?.let { call -> runCatching { call.setCamera(!call.cameraOn) } }
            "hand" -> app.call?.let { call -> runCatching { call.setHand(!call.handUp) } }
            "say" -> app.call?.let { call ->
                call.chatDraft = rest
                call.sendDraft()
            }
            "invite" -> app.makeGuestLink(app.room, rest.ifEmpty { "Web Peer" }, null, null).fold(
                // For a browser peer to follow this app into its room.
                { Log.i(TAG, "debug: invite token=${it.token}") },
                { Log.i(TAG, "debug: invite refused: ${it.message}") },
            )
            "knock" -> app.knock()
            "cancel" -> app.cancelJoin()
            "state" -> Log.i(TAG, describe(app))
        }
        Log.i(TAG, "debug: $word done; ${describe(app)}")
    }

    fun describe(app: AppModel): String {
        val call = app.call
        val people = call?.people?.joinToString { "${it.id}(${it.tracks.values.joinToString("+") { t -> t.source.name.lowercase() }})" }
        return "screen=${app.screen.name.lowercase()} signedIn=${app.account != null} room=${app.room} problem=${app.problem}" +
            (if (call != null) " call=${call.state.name.lowercase()} mic=${call.microphoneOn} camera=${call.cameraOn} people=[$people] messages=${call.messages.size}" else "")
    }
}
