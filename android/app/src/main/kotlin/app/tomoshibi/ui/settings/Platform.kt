package app.tomoshibi.ui.settings

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.ImageDecoder
import android.graphics.Paint
import android.graphics.Rect
import android.media.AudioDeviceInfo
import android.net.Uri
import android.util.Base64
import android.util.Log
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.platform.LocalContext
import app.tomoshibi.BuildConfig
import app.tomoshibi.basics.t
import app.tomoshibi.media.AudioDevice
import app.tomoshibi.media.CameraCapture
import app.tomoshibi.media.CameraChoice
import app.tomoshibi.media.SystemAudioRouting
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.ByteArrayOutputStream

/** A device somebody can pick, named for a list in the reader's language. */
data class DeviceOption(val id: String, val name: String)

/**
 * What the settings ask of the device and the core, kept apart from the
 * views so that the pages can be drawn with no core and no hardware: the
 * app's version, the core's, the rules a room name is held to, the devices
 * to choose from, the check of the camera and the microphone, and the
 * picture somebody picked.
 */
internal interface SettingsPlatform {
    /** "0.1.0 (1)": the version and the build, as the Apple apps write them. */
    val appVersion: String
    fun coreVersion(): String
    fun normaliseRoomName(typed: String): String
    fun validRoomName(name: String): Boolean
    fun validScope(scope: String): Boolean
    fun cameras(): List<DeviceOption>
    fun microphones(): List<DeviceOption>
    fun speakers(): List<DeviceOption>
    /** A check of the camera and the microphone, for one page while it shows. */
    fun deviceCheck(): DeviceCheck
    /** The picture at [uri], cut and scaled to what the server is sent, as a data URL; null if it could not be read. */
    suspend fun picture(uri: Uri): String?
}

internal val LocalSettingsPlatform = staticCompositionLocalOf<SettingsPlatform?> { null }

/** Hands the device's own platform to what is inside, unless one was handed in already (a test's). */
@Composable
internal fun ProvideSettingsPlatform(content: @Composable () -> Unit) {
    if (LocalSettingsPlatform.current != null) {
        content()
        return
    }
    val context = LocalContext.current.applicationContext
    val platform = remember(context) { SystemSettingsPlatform(context) }
    DisposableEffect(platform) { onDispose { platform.close() } }
    CompositionLocalProvider(LocalSettingsPlatform provides platform, content = content)
}

@Composable
internal fun platform(): SettingsPlatform = checkNotNull(LocalSettingsPlatform.current) { "the settings are drawn inside ProvideSettingsPlatform" }

/** The device and the core as they are. */
internal class SystemSettingsPlatform(private val context: Context) : SettingsPlatform {
    private var captureMade = false
    private val capture by lazy {
        captureMade = true
        CameraCapture(context)
    }
    private val routing by lazy { SystemAudioRouting(context) }

    override val appVersion: String get() = "${BuildConfig.VERSION_NAME} (${BuildConfig.VERSION_CODE})"
    override fun coreVersion(): String = uniffi.tomoshibi_core.coreVersion()
    override fun normaliseRoomName(typed: String): String = uniffi.tomoshibi_protocol.normaliseRoomName(typed)
    override fun validRoomName(name: String): Boolean = uniffi.tomoshibi_protocol.validRoomName(name)
    override fun validScope(scope: String): Boolean = uniffi.tomoshibi_protocol.validScope(scope)
    override fun cameras(): List<DeviceOption> = DeviceNames.cameras(capture.cameras())
    override fun microphones(): List<DeviceOption> = DeviceNames.audio(routing.devices(input = true), input = true)
    override fun speakers(): List<DeviceOption> = DeviceNames.audio(routing.devices(input = false), input = false)
    override fun deviceCheck(): DeviceCheck = SystemDeviceCheck(capture, routing)

    override suspend fun picture(uri: Uri): String? = withContext(Dispatchers.IO) {
        try {
            PictureShrinker.dataUrl(ImageDecoder.createSource(context.contentResolver, uri))
        } catch (fault: Exception) {
            Log.w("Tomoshibi", "the picture could not be read: ${fault.javaClass.simpleName}")
            null
        }
    }

    /** The camera's thread, which is the only thing this holds open. */
    fun close() {
        if (captureMade) capture.close()
    }
}

/**
 * What the devices are called in a list. A phone names its own microphone
 * and loudspeaker after itself ("Pixel 8"), which says nothing about which
 * is which, so the ones built in are named by their kind in the reader's
 * language; a headset or a speaker connected to it keeps the name it gives,
 * and the web's "Device {number}" stands in where there is none. Two of the
 * same name are numbered, as a phone with a microphone at each end has.
 */
internal object DeviceNames {
    fun audio(devices: List<AudioDevice>, input: Boolean): List<DeviceOption> {
        val shown = devices.filter { pickable(it.type, input) }
        // A Bluetooth headset is listed once for calls and once for music,
        // under the same name; the call's own (SCO, LE audio) is the one a
        // call's voice can go to.
        val bluetooth = shown.filter { it.type in BLUETOOTH }.groupBy { it.name }
        val kept = shown.filter { device ->
            if (device.type !in BLUETOOTH || device.name.isBlank()) return@filter true
            val same = bluetooth[device.name].orEmpty()
            same.size == 1 || device == (same.firstOrNull { it.type != AudioDeviceInfo.TYPE_BLUETOOTH_A2DP } ?: same.first())
        }
        return numbered(kept.mapIndexed { index, device -> DeviceOption(device.id, name(device, input, index)) })
    }

    fun cameras(cameras: List<CameraChoice>): List<DeviceOption> = numbered(
        cameras.map { camera ->
            DeviceOption(
                camera.id,
                when (camera.facing) {
                    CameraChoice.Facing.Front -> t("Front camera")
                    CameraChoice.Facing.Back -> t("Back camera")
                    CameraChoice.Facing.External -> t("External camera")
                },
            )
        },
    )

    private fun name(device: AudioDevice, input: Boolean, index: Int): String {
        val given = device.name.trim().takeIf { it.isNotEmpty() }
        return when (device.type) {
            AudioDeviceInfo.TYPE_BUILTIN_MIC -> t("Built-in microphone")
            AudioDeviceInfo.TYPE_BUILTIN_SPEAKER -> t("Built-in speaker")
            AudioDeviceInfo.TYPE_BUILTIN_EARPIECE -> t("Earpiece")
            AudioDeviceInfo.TYPE_WIRED_HEADSET, AudioDeviceInfo.TYPE_WIRED_HEADPHONES ->
                if (input) t("Headset microphone") else t("Wired headphones")
            in BLUETOOTH -> given ?: t("Bluetooth")
            AudioDeviceInfo.TYPE_USB_DEVICE, AudioDeviceInfo.TYPE_USB_HEADSET, AudioDeviceInfo.TYPE_USB_ACCESSORY ->
                given ?: t("USB audio")
            else -> given ?: t("Device {number}", mapOf("number" to index + 1))
        }
    }

    /**
     * Whether a person picks this: the loudspeaker kept for alarms, the
     * buses and tuners and loopbacks a phone also lists are the system's.
     */
    private fun pickable(type: Int, input: Boolean): Boolean = type !in if (input) SYSTEM_INPUTS else SYSTEM_OUTPUTS

    private fun numbered(options: List<DeviceOption>): List<DeviceOption> {
        val seen = HashMap<String, Int>()
        return options.map { option ->
            val count = (seen[option.name] ?: 0) + 1
            seen[option.name] = count
            if (count == 1) option else option.copy(name = "${option.name} $count")
        }
    }

    /** The echo canceller's copy of what is played, which the platform lists and does not name to apps (AudioDeviceInfo's hidden TYPE_ECHO_REFERENCE). */
    private const val ECHO_REFERENCE = 28

    private val BLUETOOTH = setOf(
        AudioDeviceInfo.TYPE_BLUETOOTH_SCO, AudioDeviceInfo.TYPE_BLUETOOTH_A2DP, AudioDeviceInfo.TYPE_BLE_HEADSET,
        AudioDeviceInfo.TYPE_BLE_SPEAKER, AudioDeviceInfo.TYPE_BLE_BROADCAST,
    )
    private val SYSTEM_OUTPUTS = setOf(
        AudioDeviceInfo.TYPE_BUILTIN_SPEAKER_SAFE, AudioDeviceInfo.TYPE_BUS, AudioDeviceInfo.TYPE_IP,
        AudioDeviceInfo.TYPE_FM, AudioDeviceInfo.TYPE_AUX_LINE, AudioDeviceInfo.TYPE_REMOTE_SUBMIX,
        AudioDeviceInfo.TYPE_TELEPHONY, ECHO_REFERENCE,
    )
    private val SYSTEM_INPUTS = setOf(
        AudioDeviceInfo.TYPE_FM_TUNER, AudioDeviceInfo.TYPE_TV_TUNER, AudioDeviceInfo.TYPE_BUS, AudioDeviceInfo.TYPE_IP,
        AudioDeviceInfo.TYPE_REMOTE_SUBMIX, AudioDeviceInfo.TYPE_TELEPHONY, ECHO_REFERENCE,
    )
}

/**
 * Account.tsx `shrink`: 256 pixels square, cut from the middle, as JPEG at
 * 0.82 (Settings.swift `PictureShrinker`). A photograph off a phone is
 * megabytes of something drawn at forty-eight pixels, and the server refuses
 * one past its limit anyway. Decoded by ImageDecoder, which turns a
 * photograph the way its camera said it was held, as the browser's
 * `createImageBitmap` does, and at a fraction of its size where it is many
 * times larger than what is sent. What was transparent comes out black, as
 * it does from the web's canvas.
 */
internal object PictureShrinker {
    const val SIDE = 256

    fun dataUrl(source: ImageDecoder.Source): String? {
        val image = try {
            ImageDecoder.decodeBitmap(source) { decoder, info, _ ->
                decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
                val shorter = minOf(info.size.width, info.size.height)
                var sample = 1
                while (shorter / (sample * 2) >= SIDE) sample *= 2
                decoder.setTargetSampleSize(sample)
            }
        } catch (_: Exception) {
            return null
        }
        return dataUrl(image)
    }

    fun dataUrl(image: Bitmap): String? {
        if (image.width <= 0 || image.height <= 0) return null
        val side = minOf(image.width, image.height)
        val left = (image.width - side) / 2
        val top = (image.height - side) / 2
        val square = Bitmap.createBitmap(SIDE, SIDE, Bitmap.Config.ARGB_8888)
        Canvas(square).apply {
            drawColor(android.graphics.Color.BLACK)
            drawBitmap(image, Rect(left, top, left + side, top + side), Rect(0, 0, SIDE, SIDE), Paint(Paint.FILTER_BITMAP_FLAG))
        }
        val out = ByteArrayOutputStream()
        if (!square.compress(Bitmap.CompressFormat.JPEG, 82, out)) return null
        return "data:image/jpeg;base64," + Base64.encodeToString(out.toByteArray(), Base64.NO_WRAP)
    }
}
