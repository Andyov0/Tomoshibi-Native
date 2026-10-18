package app.tomoshibi

import android.os.Bundle
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.lifecycleScope
import app.tomoshibi.core.CoreNative
import kotlinx.coroutines.launch
import uniffi.tomoshibi_core.Call
import uniffi.tomoshibi_core.CallEvent
import uniffi.tomoshibi_core.CallObserver
import uniffi.tomoshibi_core.CallOptions
import uniffi.tomoshibi_core.IceServer
import uniffi.tomoshibi_core.MediaKind
import uniffi.tomoshibi_core.VideoKind
import uniffi.tomoshibi_core.joinCall
import uniffi.tomoshibi_protocol.Deployment
import uniffi.tomoshibi_protocol.SoundKind

private const val TAG = "Tomoshibi"

/**
 * For now the probe that proves the core on Android: signs in, joins a room,
 * draws the first remote picture, plays (or, muted, measures) every remote
 * sound, and sends the camera and the microphone. Started with
 * `adb shell am start -n app.tomoshibi.android/app.tomoshibi.MainActivity
 *  -e base http://127.0.0.1:8080 -e name … -e pass … -e room … --ez mute true`.
 */
class MainActivity : ComponentActivity() {
    private val lines = mutableStateListOf<String>()
    private val picture = mutableStateOf<Pair<Call, String>?>(null)
    private var call: Call? = null
    private val players = mutableMapOf<String, PcmPlayer>()
    private var microphone: Microphone? = null
    private var camera: Camera? = null

    private fun say(line: String) {
        Log.i(TAG, line)
        runOnUiThread { lines.add(line) }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        CoreNative.load(this)
        setContent {
            Box(Modifier.fillMaxSize().background(Color.Black)) {
                picture.value?.let { (call, sid) ->
                    AndroidView(factory = { RemoteVideoView(it) }, update = { it.show(call, sid) }, modifier = Modifier.fillMaxSize())
                }
                Column(Modifier.safeDrawingPadding().padding(16.dp).fillMaxWidth()) {
                    for (line in lines) BasicText(line, style = TextStyle(color = Color.White, fontSize = 13.sp))
                }
            }
        }
        val base = intent.getStringExtra("base") ?: "http://127.0.0.1:8080"
        val name = intent.getStringExtra("name") ?: "Android"
        val pass = intent.getStringExtra("pass") ?: ""
        val room = intent.getStringExtra("room") ?: "android-probe"
        val muted = intent.getBooleanExtra("mute", true)
        val send = intent.getBooleanExtra("send", true)
        lifecycleScope.launch {
            try {
                val deployment = Deployment(base)
                if (pass.isNotEmpty()) {
                    val account = deployment.signIn(name, pass)
                    say("signed in as ${account.name}")
                }
                val grant = deployment.join(room, name, "", null, "", null)
                say("granted ${grant.room} as ${grant.identity}")
                val joined = joinCall(CallOptions(grant.url, grant.token, grant.room, null,
                    grant.forward?.let { IceServer(it.url, it.username, it.credential) }), observer(muted))
                call = joined
                say("joined ${joined.roomName()} with ${joined.participants().size} others")
                if (pass.isNotEmpty()) {
                    val invite = deployment.createStandingInvite(room, "Web Peer", null, null)
                    Log.i(TAG, "invite token=${invite.token}")
                }
                for (p in joined.participants()) for (t in p.tracks) want(joined, t.sid, t.kind, muted)
                if (send) {
                    val voice = joined.publishAudio(SoundKind.Voice(false))
                    microphone = Microphone(voice).also { it.start() }
                    say("microphone published ${voice.sid()}")
                    val video = joined.publishVideo(VideoKind.Camera, 1280u, 720u)
                    camera = Camera(this@MainActivity, this@MainActivity, video).also { it.start() }
                    say("camera published ${video.sid()}")
                }
            } catch (e: Exception) {
                say("failed: ${e.javaClass.simpleName} ${e.message}")
                Log.e(TAG, "probe failed", e)
            }
        }
    }

    private fun observer(muted: Boolean) = object : CallObserver {
        override fun onEvent(event: CallEvent) {
            when (event) {
                is CallEvent.StateChanged -> say("state ${event.state}")
                is CallEvent.ParticipantJoined -> say("joined: ${event.participant.name}")
                is CallEvent.ParticipantLeft -> say("left: ${event.identity}")
                is CallEvent.TrackSubscribed -> {
                    say("subscribed ${event.track.kind} ${event.track.source} from ${event.identity}")
                    call?.let { want(it, event.track.sid, event.track.kind, muted) }
                }
                is CallEvent.Ended -> say("ended: ${event.reason}")
                else -> {}
            }
        }
    }

    private fun want(call: Call, sid: String, kind: MediaKind, muted: Boolean) {
        when (kind) {
            MediaKind.VIDEO -> runOnUiThread { if (picture.value == null) picture.value = call to sid }
            MediaKind.AUDIO -> if (sid !in players) {
                val player = PcmPlayer(sid, muted)
                players[sid] = player
                call.attachPcm(sid, player)
            }
        }
    }

    override fun onDestroy() {
        microphone?.stop()
        camera?.stop()
        players.values.forEach { it.close() }
        val leaving = call
        call = null
        if (leaving != null) lifecycleScope.launch { leaving.leave() }
        super.onDestroy()
    }
}
