package app.tomoshibi.media

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.tomoshibi_core.Call
import uniffi.tomoshibi_core.NoHandle
import uniffi.tomoshibi_core.PictureShape

/*
 * Who draws which remote picture, and the size the server is asked for
 * (VideoHub), with the core stood in for by UniFFI's fake object. What these
 * guard went wrong unseen: the first picture of a call let its hub go before
 * it was added to it, so every view held a hub nobody could find. The server
 * was never asked for a size, and no window was ever taken back from the
 * core, which drew into destroyed surfaces after the small window closed.
 */
class VideoHubTest {
    private class FakeCall : Call(NoHandle) {
        val attached = mutableListOf<Pair<String, ULong>>()
        val detached = mutableListOf<Pair<String, ULong>>()
        val asked = mutableListOf<Triple<String, UInt, UInt>>()

        override fun attachVideoWindow(sid: String, window: ULong, shape: PictureShape) {
            attached += sid to window
        }
        override fun detachVideoWindow(sid: String, window: ULong) {
            detached += sid to window
        }
        override fun requestVideoSize(sid: String, width: UInt, height: UInt) {
            asked += Triple(sid, width, height)
        }
    }

    private val shape = object : PictureShape {
        override fun changed(width: UInt, height: UInt, rotation: UInt) {}
    }
    private val call = FakeCall()
    private val tile = Any()
    private val small = Any()

    @After
    fun forget() = VideoHub.forget(call)

    @Test
    fun theFirstPictureOfACallKeepsItsHub() {
        val hub = VideoHub.of(call)
        hub.attach(tile, "TR_cam", 11L, shape)
        assertSame(hub, VideoHub.existing(call))
        assertEquals(listOf("TR_cam" to 11uL), call.attached)
    }

    @Test
    fun theSizeAPictureIsDrawnAtIsAskedFor() {
        VideoHub.of(call).attach(tile, "TR_cam", 11L, shape)
        // What the view does once it is laid out: through the table, as
        // RemoteVideoView asks.
        VideoHub.existing(call)!!.want(tile, "TR_cam", 640, 360)
        assertEquals(listOf(Triple("TR_cam", 640u, 360u)), call.asked)
        // The small window beside the tile, larger: the larger is asked for.
        VideoHub.of(call).attach(small, "TR_cam", 12L, shape)
        VideoHub.existing(call)!!.want(small, "TR_cam", 1280, 720)
        assertEquals(Triple("TR_cam", 1280u, 720u), call.asked.last())
        // And when it goes, the tile's again.
        VideoHub.existing(call)!!.detach(small)
        assertEquals(Triple("TR_cam", 640u, 360u), call.asked.last())
    }

    @Test
    fun everyWindowGoesBackToTheCoreWhenItsViewGoes() {
        VideoHub.of(call).attach(tile, "TR_cam", 11L, shape)
        VideoHub.of(call).attach(small, "TR_cam", 12L, shape)
        VideoHub.existing(call)!!.detach(small)
        VideoHub.existing(call)!!.detach(tile)
        assertEquals(listOf("TR_cam" to 12uL, "TR_cam" to 11uL), call.detached)
        // Nothing left drawn, the hub lets its call go.
        assertNull(VideoHub.existing(call))
    }

    @Test
    fun aViewShownSomethingElseTakesBackWhatItDrewBefore() {
        val hub = VideoHub.of(call)
        hub.attach(tile, "TR_cam", 11L, shape)
        hub.attach(tile, "TR_screen", 13L, shape)
        assertEquals(listOf("TR_cam" to 11uL), call.detached)
        assertSame(hub, VideoHub.existing(call))
        assertTrue("TR_cam" !in hub.shapes)
    }
}
