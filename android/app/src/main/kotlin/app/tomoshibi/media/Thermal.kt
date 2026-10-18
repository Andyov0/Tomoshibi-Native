package app.tomoshibi.media

import android.content.Context
import android.os.PowerManager
import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.setValue
import androidx.core.content.ContextCompat

private const val TAG = "Tomoshibi"

/**
 * How hot the device says it is (PowerManager's thermal status), for what is
 * asked of the server and drawn (the Apple apps read ProcessInfo's
 * thermalState for the same, VideoSurface.swift `askAgain`).
 *
 * The owner's iPhone reached thermal Warn within a minute and a half of
 * watching a shared screen, from decoding and drawing a rung far larger than
 * the picture it was shown in (2026-10-17). Android's statuses are finer than
 * Apple's four; SEVERE is the first the platform describes as the device
 * throttling itself to cool down, Apple's "serious", and is where pictures
 * are asked for at their points alone.
 *
 * Snapshot state, read on the main thread; [install] is idempotent.
 */
object Thermal {
    var status by mutableIntStateOf(PowerManager.THERMAL_STATUS_NONE)
        private set

    private var installed = false
    private val listeners = LinkedHashSet<() -> Unit>()

    /** Whether the device is hot enough that every picture is asked for at its points rather than its pixels. */
    val hot: Boolean get() = status >= PowerManager.THERMAL_STATUS_SEVERE

    /** Once, on the main thread, with any context. */
    fun install(context: Context) {
        if (installed) return
        installed = true
        val power = context.getSystemService(PowerManager::class.java) ?: return
        real = power.currentThermalStatus
        status = pretending ?: real
        power.addThermalStatusListener(ContextCompat.getMainExecutor(context.applicationContext)) { changed ->
            real = changed
            if (pretending != null || changed == status) return@addThermalStatusListener
            Log.i(TAG, "thermal status $status -> $changed")
            status = changed
            for (listener in listeners.toList()) listener()
        }
    }

    /**
     * A debug build's `thermal`: the device said to be at [status] (one of
     * PowerManager's THERMAL_STATUS_*), or back to what it really is for
     * null, so that what heat changes can be watched without heating
     * anything.
     */
    internal fun pretend(status: Int?) {
        pretending = status
        val next = status ?: real
        if (next == this.status) return
        this.status = next
        for (listener in listeners.toList()) listener()
    }

    private var pretending: Int? = null
    private var real = PowerManager.THERMAL_STATUS_NONE

    /** Told on the main thread whenever the status changes, until the returned handle is closed. */
    fun watch(listener: () -> Unit): AutoCloseable {
        listeners.add(listener)
        return AutoCloseable { listeners.remove(listener) }
    }
}
