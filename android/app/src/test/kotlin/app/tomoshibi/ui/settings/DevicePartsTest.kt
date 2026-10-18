package app.tomoshibi.ui.settings

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.ImageDecoder
import android.graphics.Paint
import android.media.AudioDeviceInfo
import android.util.Base64
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.media.AudioDevice
import app.tomoshibi.media.CameraChoice
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer

/*
 * The parts of the settings that are rules rather than drawing: what the
 * devices are called in a list, and the picture as it is sent. What these
 * guard is a phone's two microphones both called "Pixel 9", a headset listed
 * twice, and a photograph sent whole or cut from its corner.
 */
@RunWith(AndroidJUnit4::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35])
class DevicePartsTest {
    @Before
    fun inEnglish() {
        val localization = Localization.install(ApplicationProvider.getApplicationContext())
        localization.keeps = false
        localization.choose(Language.En)
    }

    @Test
    fun aPhonesOwnDevicesAreNamedByKindAndAHeadsetByItsName() {
        val microphones = DeviceNames.audio(FakeRouting.inputs, input = true)
        assertEquals(listOf("Built-in microphone", "Built-in microphone 2", "Shokz OpenComm"), microphones.map { it.name })
        assertEquals(listOf("11", "12", "31"), microphones.map { it.id })

        // The speaker kept for alarms is the system's; the headset is listed
        // once, by its call's own route.
        val speakers = DeviceNames.audio(FakeRouting.outputs, input = false)
        assertEquals(listOf("Earpiece", "Built-in speaker", "Shokz OpenComm"), speakers.map { it.name })
        assertEquals("32", speakers.last().id)
    }

    @Test
    fun aDeviceWithNoNameOfItsOwnIsNumberedAsTheWebNumbersIt() {
        val named = DeviceNames.audio(
            listOf(AudioDevice("5", "", AudioDeviceInfo.TYPE_DOCK), AudioDevice("6", " ", AudioDeviceInfo.TYPE_BLUETOOTH_A2DP)),
            input = false,
        )
        assertEquals(listOf("Device 1", "Bluetooth"), named.map { it.name })
        val cameras = DeviceNames.cameras(
            listOf(CameraChoice("1", CameraChoice.Facing.Front), CameraChoice("0", CameraChoice.Facing.Back), CameraChoice("2", CameraChoice.Facing.Back)),
        )
        assertEquals(listOf("Front camera", "Back camera", "Back camera 2"), cameras.map { it.name })
    }

    /** A wide picture: red at the sides, green in the middle square. */
    private fun wide(): ByteArray {
        val image = Bitmap.createBitmap(900, 450, Bitmap.Config.ARGB_8888)
        Canvas(image).apply {
            drawColor(Color.RED)
            drawRect(225f, 0f, 675f, 450f, Paint().apply { color = Color.GREEN })
        }
        return ByteArrayOutputStream().also { image.compress(Bitmap.CompressFormat.PNG, 100, it) }.toByteArray()
    }

    @Test
    fun thePictureIsCutFromTheMiddleToTwoHundredAndFiftySixSquareAsJpeg() {
        val url = PictureShrinker.dataUrl(ImageDecoder.createSource(ByteBuffer.wrap(wide())))
        assertNotNull(url)
        assertTrue(url!!.startsWith("data:image/jpeg;base64,"))
        val bytes = Base64.decode(url.removePrefix("data:image/jpeg;base64,"), Base64.DEFAULT)
        // A JPEG's own mark, not a PNG passed through.
        assertEquals(0xFF.toByte(), bytes[0])
        assertEquals(0xD8.toByte(), bytes[1])
        val sent = BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
        assertEquals(256, sent.width)
        assertEquals(256, sent.height)
        // The middle, edge to edge: no red from the sides got in.
        for (x in listOf(4, 128, 251)) {
            val pixel = sent.getPixel(x, 128)
            assertTrue("red at $x: ${Integer.toHexString(pixel)}", Color.red(pixel) < 60 && Color.green(pixel) > 190)
        }
    }

    @Test
    fun somethingThatIsNotAPictureIsRefused() {
        assertNull(PictureShrinker.dataUrl(ImageDecoder.createSource(ByteBuffer.wrap("not a picture".toByteArray()))))
    }
}
