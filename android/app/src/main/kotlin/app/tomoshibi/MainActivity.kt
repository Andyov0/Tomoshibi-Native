package app.tomoshibi

import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.lifecycleScope
import app.tomoshibi.core.CoreNative
import app.tomoshibi.media.DeviceAccess
import app.tomoshibi.media.RemoteVideo
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.CallModel
import app.tomoshibi.model.DebugLaunch
import app.tomoshibi.model.Surface
import kotlinx.coroutines.launch

private const val TAG = "Tomoshibi"

/**
 * The app's one activity: loads the core, keeps the app's model for the life
 * of the process, and draws a placeholder for each of the model's screens
 * until the real ones are written.
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
 * `cancel`, `state`.
 */
class MainActivity : ComponentActivity() {
    private lateinit var app: AppModel

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        CoreNative.load(this)
        DeviceAccess.install(this)
        app = Shell.app(this, intent)
        setContent { Screens(app) }
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
        model ?: AppModel.create(context.applicationContext, if (BuildConfig.DEBUG) Debug.launch(intent) else null).also { model = it }
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

private val plain = TextStyle(color = Color.White, fontSize = 15.sp)
private val quiet = TextStyle(color = Color(0xFFB0B0B0), fontSize = 13.sp)
private val press = TextStyle(color = Color(0xFFFFC46B), fontSize = 16.sp)

/**
 * A placeholder for each screen, enough to drive the flow by hand; the real
 * screens come from the views being ported from the Apple apps, against the
 * same models.
 */
@Composable
private fun Screens(app: AppModel) {
    val scope = rememberCoroutineScope()
    LaunchedEffect(app) {
        // The lobby's lists, whenever the lobby is reached.
        snapshotFlow { app.screen to app.account?.name }.collect { (screen, _) ->
            if (screen == AppModel.Screen.Lobby) app.lobby.refreshArrangements()
        }
    }
    Box(Modifier.fillMaxSize().background(Color.Black)) {
        Column(Modifier.safeDrawingPadding().padding(16.dp).fillMaxWidth().verticalScroll(rememberScrollState())) {
            BasicText(app.screen.name, style = TextStyle(color = Color.White, fontSize = 22.sp))
            Spacer(Modifier.height(8.dp))
            when (app.screen) {
                AppModel.Screen.Starting -> BasicText("…", style = quiet)
                AppModel.Screen.Unreachable -> Press("Try again") { scope.launch { app.connect() } }
                AppModel.Screen.Lobby -> {
                    BasicText(app.account?.let { "Signed in as ${it.name}" } ?: "Not signed in", style = plain)
                    for (recent in app.lobby.recentHere) Press(recent.room) { scope.launch { app.enter(recent.room) } }
                    for (meeting in app.lobby.upcoming) BasicText("${meeting.room} at ${meeting.at}", style = quiet)
                    if (app.account != null) Press("Sign out") { scope.launch { app.signOut() } }
                }
                AppModel.Screen.Prejoin -> {
                    BasicText("Room ${app.room} as ${app.joiningName}", style = plain)
                    Press(if (app.joining) "Joining…" else "Join") { scope.launch { app.join() } }
                    if (app.canKnock) Press("Ask to be let in") { scope.launch { app.knock() } }
                    Press("Back") { app.backToLobby() }
                }
                AppModel.Screen.Knocking -> {
                    BasicText("Waiting to be let into ${app.room}", style = plain)
                    Press("Stop") { app.stopKnocking() }
                }
                AppModel.Screen.Waiting -> {
                    BasicText("Waiting for ${app.arrangement?.room ?: app.room}", style = plain)
                    Press("Join") { scope.launch { app.goArranged() } }
                    Press("Back") { app.stopWaiting() }
                }
                AppModel.Screen.Call -> app.call?.let { CallPlaceholder(it, app) } ?: BasicText("Moving to ${app.movingTo}", style = quiet)
                AppModel.Screen.Ended -> {
                    BasicText(app.ending.name, style = plain)
                    Press("Join again") { scope.launch { app.join() } }
                    Press("Back") { app.backToLobby() }
                }
            }
            app.problem?.let {
                Spacer(Modifier.height(12.dp))
                BasicText(it, style = TextStyle(color = Color(0xFFFF8A80), fontSize = 14.sp))
                if (app.problemStays) Press("Close") { app.problem = null }
            }
        }
    }
}

@Composable
private fun CallPlaceholder(call: CallModel, app: AppModel) {
    val scope = rememberCoroutineScope()
    BasicText("${call.room}: ${call.state.name.lowercase()}", style = plain)
    Row {
        Press(if (call.microphoneOn) "Mute" else "Unmute") { scope.launch { app.toggleMicrophone() } }
        Spacer(Modifier.padding(8.dp))
        Press(if (call.cameraOn) "Camera off" else "Camera on") { scope.launch { runCatching { call.setCamera(!call.cameraOn) } } }
        Spacer(Modifier.padding(8.dp))
        Press("Leave") { scope.launch { app.leave() } }
    }
    for (person in call.people) {
        BasicText(
            "${CallModel.named(person.name, person.id)}${if (person.speaking) " (speaking)" else ""}${if (person.handUp) " (hand up)" else ""}",
            style = plain,
        )
    }
    val pictures = call.surfaces.filter { it.sid != null }
    for (surface in pictures.take(2)) {
        RemoteVideo(
            call.call, surface.sid,
            Modifier.fillMaxWidth().aspectRatio(if (surface.kind == Surface.Kind.Screen) 16f / 9 else 4f / 3).padding(vertical = 4.dp),
        )
    }
    for (toast in call.toasts) BasicText(toast.title + (toast.detail?.let { " — $it" } ?: ""), style = quiet)
    for (line in call.messages.takeLast(5)) BasicText("${line.name}: ${line.text}", style = quiet)
}

@Composable
private fun Press(label: String, action: () -> Unit) {
    BasicText(label, style = press, modifier = Modifier.clickable(onClick = action).padding(vertical = 8.dp))
}
