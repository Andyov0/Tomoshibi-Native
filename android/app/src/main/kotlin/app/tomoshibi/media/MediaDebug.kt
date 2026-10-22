package app.tomoshibi.media

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.util.Log
import androidx.core.content.ContextCompat
import app.tomoshibi.model.AppModel
import com.google.mediapipe.tasks.core.Delegate
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.tomoshibi_protocol.ShareQuality

private const val TAG = "Tomoshibi"

/**
 * A debug build's switches for the media layer, from adb, beside the
 * activity's own `-e do` (MainActivity): the share, its sound, the routes,
 * blur, the small window and the heat, each of which is otherwise reached
 * only by a press on a screen that does not exist yet, or not at all.
 *
 *     adb shell am broadcast -a app.tomoshibi.debug.MEDIA -p app.tomoshibi.android --es do 'share'
 *
 * `share`, `stopshare`, `sound`, `stopsound`, `quality <0|1080|1440|2160>
 * [fps]`, `restart <0|1080|1440|2160> <fps>`, `routes`, `route <id|auto>`,
 * `blur on|off`, `blurtest <file in the app's files> [gpu]`,
 * `sharedpicture`, `pip`, `thermal <status>|real`, `secondcamera on|off`,
 * `state`. Registered only in a debug build.
 */
internal object MediaDebug {
    const val ACTION = "app.tomoshibi.debug.MEDIA"
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var installed = false
    private var second: CameraCapture? = null

    fun install(context: Context, app: AppModel) {
        if (installed) return
        installed = true
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(context: Context, intent: Intent) {
                val line = intent.getStringExtra("do") ?: return
                scope.launch { perform(context, app, line) }
            }
        }
        ContextCompat.registerReceiver(context, receiver, IntentFilter(ACTION), ContextCompat.RECEIVER_EXPORTED)
    }

    private fun quality(word: String?): ShareQuality = when (word) {
        "1080" -> ShareQuality.P1080
        "1440" -> ShareQuality.P1440
        "2160" -> ShareQuality.P4K
        else -> ShareQuality.AUTO
    }

    private suspend fun perform(context: Context, app: AppModel, line: String) {
        val words = line.split(' ').filter { it.isNotEmpty() }
        Log.i(TAG, "media debug: $line")
        val call = app.call
        try {
            when (words.firstOrNull()) {
                "share" -> call?.let { it.sharing.start(it.call) }
                "stopshare" -> call?.sharing?.stopScreen()
                "sound" -> call?.let { it.sharing.startSoundOnly(it.call, it.sealed) }
                "stopsound" -> call?.sharing?.stopSound()
                "quality" -> call?.sharing?.choose(quality(words.getOrNull(1)), words.getOrNull(2)?.toIntOrNull())
                "restart" -> call?.let { it.sharing.restart(quality(words.getOrNull(1)), words.getOrNull(2)?.toIntOrNull() ?: 30, it.call) }
                "routes" -> call?.let {
                    Log.i(TAG, "media debug: out ${it.audio.devices(input = false).joinToString { d -> "${d.id}(${d.kind})" }}; in ${it.audio.devices(input = true).joinToString { d -> "${d.id}(${d.kind})" }}; current ${it.audio.current(input = false)}")
                }
                "route" -> call?.let { c ->
                    val id = words.getOrNull(1)
                    val device = c.audio.devices(input = false).firstOrNull { it.id == id }
                    if (device != null) c.useAudio(device, input = false) else Log.i(TAG, "media debug: no route $id")
                }
                "blur" -> call?.setBlur(words.getOrNull(1) == "on")
                "blurtest" -> blurTest(context, File(context.filesDir, words.getOrNull(1) ?: "blur-in.jpg"), gpu = words.getOrNull(2) == "gpu")
                "sharedpicture" -> call?.sharing?.picture()?.let { sharedPicture(context, it) }
                "pip" -> CallWindow.enterFromDebug()
                "thermal" -> Thermal.pretend(words.getOrNull(1)?.toIntOrNull())
                // A second capture of the same camera beside the call's, as
                // the join screen's picture of oneself overlaps the call's
                // camera for a moment: started, then stopped late.
                "secondcamera" -> if (words.getOrNull(1) == "on") {
                    second = (second ?: CameraCapture(context)).also { it.start() }
                } else {
                    second?.close()
                    second = null
                }
                "state" -> {}
            }
        } catch (fault: Exception) {
            Log.i(TAG, "media debug: $line failed: ${fault.javaClass.simpleName} ${fault.message}")
        }
        val now = app.call
        Log.i(TAG, "media debug: $line done; sharing ${now?.sharing?.state ?: "-"}; out ${now?.audio?.current(input = false) ?: "-"};" +
            " window visible ${CallWindow.visible} small ${CallWindow.inPictureInPicture} wanted ${CallWindow.wanted};" +
            " thermal ${Thermal.status}; blur ${now?.blurring};" +
            " down ${now?.connection?.downKbps?.toInt()} kb/s up ${now?.connection?.upKbps?.toInt()} kb/s;" +
            " camera sent ${if (now?.cameraOn == true) now.camera.sent else "-"};" +
            " publishing ${now?.localTracks?.values?.joinToString { "${it.source.name.lowercase()}:${it.name}:${it.width}x${it.height}" }}")
    }

    /**
     * The screen share's texture as it is, read back through WebRTC's own
     * conversion at video range: the check that what the encoders are handed
     * is the right way up, the right way round and the right colours, beside
     * what a browser receives of it.
     */
    private fun sharedPicture(context: Context, picture: ScreenFrames.Picture) {
        val width = picture.width
        val height = picture.height
        val pixels = IntArray(width * height)
        val chroma = picture.stride / 2
        for (row in 0 until height) for (column in 0 until width) {
            val luma = 1.164f * ((picture.y[row * picture.stride + column].toInt() and 0xff) - 16)
            val c = (row / 2) * chroma + column / 2
            val cb = (picture.u[c].toInt() and 0xff) - 128f
            val cr = (picture.v[c].toInt() and 0xff) - 128f
            val r = (luma + 1.596f * cr).toInt().coerceIn(0, 255)
            val g = (luma - 0.392f * cb - 0.813f * cr).toInt().coerceIn(0, 255)
            val b = (luma + 2.017f * cb).toInt().coerceIn(0, 255)
            pixels[row * width + column] = (0xff shl 24) or (r shl 16) or (g shl 8) or b
        }
        val target = File(context.filesDir, "shared.jpg")
        target.outputStream().use { Bitmap.createBitmap(pixels, width, height, Bitmap.Config.ARGB_8888).compress(Bitmap.CompressFormat.JPEG, 85, it) }
        Log.i(TAG, "media debug: wrote $target, ${width}x$height")
    }

    /**
     * Blurs a still picture through the same segmenter and arithmetic as a
     * camera frame, and writes it beside the original as `-blurred.jpg`: a
     * check, with a real person in the picture, that the mask keeps the
     * person and blurs the rest, which an emulator's camera cannot show.
     */
    private suspend fun blurTest(context: Context, file: File, gpu: Boolean) {
        val bitmap = BitmapFactory.decodeFile(file.path) ?: run {
            Log.i(TAG, "media debug: no picture at $file")
            return
        }
        val width = bitmap.width and 1.inv()
        val height = bitmap.height and 1.inv()
        val pixels = IntArray(width * height)
        bitmap.getPixels(pixels, 0, width, 0, 0, width, height)
        val cw = width / 2
        val y = ByteArray(width * height)
        val u = ByteArray(cw * (height / 2))
        val v = ByteArray(cw * (height / 2))
        for (row in 0 until height) for (column in 0 until width) {
            val p = pixels[row * width + column]
            val r = (p shr 16) and 0xff
            val g = (p shr 8) and 0xff
            val b = p and 0xff
            y[row * width + column] = (0.299 * r + 0.587 * g + 0.114 * b).toInt().coerceIn(0, 255).toByte()
            if (row % 2 == 0 && column % 2 == 0) {
                val c = (row / 2) * cw + column / 2
                u[c] = (128 - 0.168736 * r - 0.331264 * g + 0.5 * b).toInt().coerceIn(0, 255).toByte()
                v[c] = (128 + 0.5 * r - 0.418688 * g - 0.081312 * b).toInt().coerceIn(0, 255).toByte()
            }
        }
        val blur = BackgroundBlur(context, if (gpu) Delegate.GPU else Delegate.CPU)
        // The first pass hands the picture to the segmenter and blurs all
        // of it; the second, once a mask is back, keeps the person.
        blur.process(y.copyOf(), u.copyOf(), v.copyOf(), width, height, 0, 1_000_000L)
        delay(1500)
        val began = System.nanoTime()
        blur.process(y, u, v, width, height, 0, 2_000_000_000L)
        Log.i(TAG, "media debug: blurred ${width}x$height in ${(System.nanoTime() - began) / 1_000_000} ms")
        blur.close()
        for (row in 0 until height) for (column in 0 until width) {
            val luma = (y[row * width + column].toInt() and 0xff).toFloat()
            val c = (row / 2) * cw + column / 2
            val cb = (u[c].toInt() and 0xff) - 128f
            val cr = (v[c].toInt() and 0xff) - 128f
            val r = (luma + 1.402f * cr).toInt().coerceIn(0, 255)
            val g = (luma - 0.344136f * cb - 0.714136f * cr).toInt().coerceIn(0, 255)
            val b = (luma + 1.772f * cb).toInt().coerceIn(0, 255)
            pixels[row * width + column] = (0xff shl 24) or (r shl 16) or (g shl 8) or b
        }
        val out = Bitmap.createBitmap(pixels, width, height, Bitmap.Config.ARGB_8888)
        val target = File(file.parentFile, file.nameWithoutExtension + "-blurred.jpg")
        target.outputStream().use { out.compress(Bitmap.CompressFormat.JPEG, 90, it) }
        Log.i(TAG, "media debug: wrote $target")
    }
}
