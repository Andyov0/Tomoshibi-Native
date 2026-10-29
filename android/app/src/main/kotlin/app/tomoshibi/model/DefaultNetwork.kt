package app.tomoshibi.model

import uniffi.tomoshibi_protocol.NetworkPath

/**
 * What the system's default network callback says, as the call is told it
 * (the core's `NetworkPath`).
 *
 * The core decides from these whether the default route really changed and
 * only then makes the call resume or retry (protocol network.rs), since each
 * change it acts on is a resume of a call that may have had nothing wrong
 * with it. What it needs from here is a route that is equal for the same
 * network and different for another: a network's handle, which Android gives
 * every connection afresh, so another Wi-Fi, or the same one joined again, is
 * another route. A loss is only offline when it is the default network that
 * was lost; the callback also reports the old default going after a new one
 * has already taken over.
 */
internal class DefaultNetwork {
    private var current: Long? = null

    @Synchronized
    fun available(handle: Long): NetworkPath {
        current = handle
        return NetworkPath(online = true, route = "network $handle")
    }

    /** What to say when [handle] is lost: offline if it was the default, nothing otherwise. */
    @Synchronized
    fun lost(handle: Long): NetworkPath? {
        if (current != handle) return null
        current = null
        return NetworkPath(online = false, route = "")
    }
}
