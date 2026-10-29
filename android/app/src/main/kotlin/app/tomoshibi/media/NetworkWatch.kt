package app.tomoshibi.media

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.util.Log
import app.tomoshibi.model.DefaultNetwork
import java.lang.ref.WeakReference
import uniffi.tomoshibi_core.Call
import uniffi.tomoshibi_protocol.NetworkPath

/**
 * Tells the call what the device's default network is, every time the system
 * says (`Call.pathSeen`), for as long as the call lasts.
 *
 * Without it a call on a network that went away noticed only when the
 * server's ping went unanswered, a call gone quiet for that long, and a call
 * already reconnecting when the network came back sat out the rest of its
 * wait, up to eight seconds, before it tried; the web resumes or retries the
 * moment the browser says it is online. Every update is passed on as it comes,
 * and the core decides whether the route really changed (DefaultNetwork).
 */
class NetworkWatch(context: Context, call: Call) {
    private val connectivity = context.getSystemService(ConnectivityManager::class.java)
    private val call = WeakReference(call)
    private val network = DefaultNetwork()
    private var started = false

    private val callback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) {
            say(this@NetworkWatch.network.available(network.networkHandle))
        }

        override fun onLost(network: Network) {
            this@NetworkWatch.network.lost(network.networkHandle)?.let(::say)
        }
    }

    private fun say(path: NetworkPath) {
        Log.i(TAG, "network path: online=${path.online} route=${path.route}")
        call.get()?.pathSeen(path)
    }

    fun start() {
        val connectivity = connectivity ?: return
        // Refused (a process may hold only so many callbacks) is a call that
        // reconnects on its own schedule, as it did before this existed.
        started = try {
            connectivity.registerDefaultNetworkCallback(callback)
            true
        } catch (fault: RuntimeException) {
            Log.w(TAG, "no network callback: ${fault.message}")
            false
        }
    }

    fun stop() {
        if (!started) return
        started = false
        try {
            connectivity?.unregisterNetworkCallback(callback)
        } catch (_: IllegalArgumentException) {
            // Already gone.
        }
    }

    private companion object {
        const val TAG = "Tomoshibi"
    }
}
