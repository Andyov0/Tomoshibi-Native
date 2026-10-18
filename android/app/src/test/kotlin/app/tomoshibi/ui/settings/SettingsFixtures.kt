package app.tomoshibi.ui.settings

import android.media.AudioDeviceInfo
import android.media.AudioDeviceInfo.TYPE_BLUETOOTH_A2DP
import android.media.AudioDeviceInfo.TYPE_BLUETOOTH_SCO
import android.media.AudioDeviceInfo.TYPE_BUILTIN_EARPIECE
import android.media.AudioDeviceInfo.TYPE_BUILTIN_MIC
import android.media.AudioDeviceInfo.TYPE_BUILTIN_SPEAKER
import android.net.Uri
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import app.tomoshibi.media.AudioDevice
import app.tomoshibi.media.AudioRouting
import app.tomoshibi.media.CallAudio
import app.tomoshibi.media.CameraChoice
import app.tomoshibi.media.PictureInPicture
import app.tomoshibi.media.Sharing
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.CallModel
import app.tomoshibi.model.MemoryDefaults
import app.tomoshibi.model.MemorySecrets
import kotlinx.coroutines.Dispatchers
import uniffi.tomoshibi_core.Call
import uniffi.tomoshibi_core.NoHandle
import uniffi.tomoshibi_protocol.Account
import uniffi.tomoshibi_protocol.Deployment
import uniffi.tomoshibi_protocol.DeploymentInfo
import uniffi.tomoshibi_protocol.Joining
import uniffi.tomoshibi_protocol.Opening

/*
 * What the settings are drawn from in a test: the app's model with somebody
 * signed in and nothing behind it, a platform with devices and no hardware,
 * and a call whose sound goes nowhere. No core is loaded: the JVM a unit
 * test runs on cannot load the Android build of it.
 */

/** The account the pictures are of. */
internal val Ada = Account(
    name = "Ada Lovelace",
    trip = "k7m2qx4bnd",
    avatar = null,
    admin = false,
    created = "2026-03-14T09:30:00Z",
    scopes = listOf("lab", "studio"),
    can = listOf("watch", "music", "arrange", "invite", "open"),
)

/**
 * The app's model, signed in as [account] where one is given. The model's
 * setters are its own on purpose (only the deployment says who is signed
 * in), so a test reaches the state behind them, as nothing in the app may.
 */
internal fun fakeApp(account: Account? = Ada, info: DeploymentInfo? = FakeInfo): AppModel {
    val app = AppModel(MemoryDefaults(), MemorySecrets(), media = null, home = "https://example.test/", context = Dispatchers.Unconfined)
    app.poke("account", account)
    app.poke("info", info)
    return app
}

internal val FakeInfo = DeploymentInfo(
    openedBy = Opening.SIGNED,
    joinedBy = Joining.INVITED,
    source = "https://github.com/5t-RawBeRry/Tomoshibi",
    build = "4f1c2a9e7b03d55e",
)

/** Sets the snapshot state behind one of a model's properties. */
@Suppress("UNCHECKED_CAST")
internal fun Any.poke(property: String, value: Any?) {
    val field = javaClass.getDeclaredField("$property\$delegate").apply { isAccessible = true }
    (field.get(this) as MutableState<Any?>).value = value
}

/** Devices, a check that opens nothing, and the core's rules as the web writes them. */
internal class FakePlatform : SettingsPlatform {
    val check = FakeCheck()
    override val appVersion = "0.1.0 (1)"
    override fun coreVersion() = "0.1.0"

    /** names.ts, near enough for a test: lowercase, and anything else a dash. */
    override fun normaliseRoomName(typed: String) = typed.lowercase().map { if (it in 'a'..'z' || it in '0'..'9') it else '-' }.joinToString("")
    override fun validRoomName(name: String): Boolean {
        val room = name.substringBefore('@')
        return room.isNotEmpty() && room.first() != '-' && room.last() != '-'
    }
    override fun validScope(scope: String) = scope.isNotEmpty() && scope.all { it in 'a'..'z' || it in '0'..'9' || it == '-' }
    override fun cameras() = DeviceNames.cameras(listOf(CameraChoice("1", CameraChoice.Facing.Front), CameraChoice("0", CameraChoice.Facing.Back)))
    override fun microphones() = DeviceNames.audio(FakeRouting.inputs, input = true)
    override fun speakers() = DeviceNames.audio(FakeRouting.outputs, input = false)
    override fun deviceCheck(): DeviceCheck = check
    override suspend fun picture(uri: Uri): String? = null
}

/** A check that says it hears somebody and has no camera to show. */
internal class FakeCheck : DeviceCheck {
    override var level by mutableFloatStateOf(0.55f)
    override var cameraRunning by mutableStateOf(false)
    override var cameraRefused by mutableStateOf(false)
    override var microphoneRefused by mutableStateOf(false)
    override var started = false
    var starts = 0

    override fun start(camera: String?, microphone: String?) {
        started = true
        starts++
    }

    override fun stop() {
        started = false
    }

    override fun close() = stop()

    @Composable
    override fun Picture(modifier: Modifier) {
        Box(modifier)
    }
}

/** A phone's devices, with a headset paired: it is listed for calls and for music. */
internal object FakeRouting : AudioRouting {
    val inputs = listOf(
        AudioDevice("11", "Pixel 9", TYPE_BUILTIN_MIC),
        AudioDevice("12", "Pixel 9", TYPE_BUILTIN_MIC),
        AudioDevice("31", "Shokz OpenComm", TYPE_BLUETOOTH_SCO),
    )
    val outputs = listOf(
        AudioDevice("21", "Pixel 9", TYPE_BUILTIN_EARPIECE),
        AudioDevice("22", "Pixel 9", TYPE_BUILTIN_SPEAKER),
        AudioDevice("23", "Pixel 9", AudioDeviceInfo.TYPE_BUILTIN_SPEAKER_SAFE),
        AudioDevice("32", "Shokz OpenComm", TYPE_BLUETOOTH_SCO),
        AudioDevice("33", "Shokz OpenComm", TYPE_BLUETOOTH_A2DP),
    )

    override fun devices(input: Boolean) = if (input) inputs else outputs
    override val choosable = true
    override fun info(id: String?, input: Boolean): AudioDeviceInfo? = null
    override val offersUnprocessed = false
    override fun communicating(on: Boolean) {}
}

private class FakeCall : Call(NoHandle) {
    override fun roomName() = "team-room"
    override fun localIdentity() = "a-me"
    override fun localName() = "Ada Lovelace"
}

/** A call on the headset, its microphone on, with the cameras of a phone and no core under it. */
internal fun fakeCall(): CallModel {
    val audio = CallAudio(FakeRouting, muted = true)
    audio.prefer(input = "31", output = "32", original = false)
    val call = CallModel(
        FakeCall(), object : Deployment(uniffi.tomoshibi_protocol.NoHandle) {}, region = null, sealed = false,
        audio = audio, makeCamera = { error("no camera in a test") }, sharing = Sharing(), pictureInPicture = PictureInPicture(),
        context = Dispatchers.Unconfined,
    )
    call.poke("microphoneOn", true)
    return call
}

internal object FakeCameras : CallCameras {
    override fun list(call: CallModel) = listOf(CameraChoice("1", CameraChoice.Facing.Front), CameraChoice("0", CameraChoice.Facing.Back))
    override fun current(call: CallModel) = "1"
}
