package app.tomoshibi.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.tomoshibi_protocol.NetworkPath

/*
 * What the call is told of the default network. The core resumes the call
 * when the route it is told differs from the one before, so what these guard
 * is a hand-over read as no change (Wi-Fi to mobile data, which the callback
 * reports as the new default arriving and the old one being lost afterwards,
 * must not end on "offline"), and the same network read as another.
 */
class DefaultNetworkTest {
    @Test
    fun theSameNetworkIsTheSameRouteAndAnotherIsAnother() {
        val network = DefaultNetwork()
        val wifi = network.available(100L)
        assertEquals(NetworkPath(online = true, route = "network 100"), wifi)
        assertEquals(wifi, network.available(100L))
        assertNotEquals(wifi, network.available(200L))
    }

    @Test
    fun theOldDefaultGoingAfterTheNewOneArrivedIsNothing() {
        val network = DefaultNetwork()
        network.available(100L)
        network.available(200L)
        assertNull(network.lost(100L))
    }

    @Test
    fun losingTheDefaultWithNothingInItsPlaceIsOffline() {
        val network = DefaultNetwork()
        network.available(100L)
        assertEquals(NetworkPath(online = false, route = ""), network.lost(100L))
        assertNull(network.lost(100L))
    }
}
