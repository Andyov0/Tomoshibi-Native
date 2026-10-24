package app.tomoshibi.media

import android.animation.ValueAnimator
import android.annotation.SuppressLint
import android.content.Context
import android.content.Intent
import android.content.res.Configuration
import android.graphics.PixelFormat
import android.graphics.Rect
import android.net.Uri
import android.os.Build
import android.provider.Settings
import android.util.Log
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import android.view.WindowInsets
import android.view.WindowManager
import android.view.accessibility.AccessibilityNodeInfo
import android.view.animation.PathInterpolator
import android.widget.FrameLayout
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.platform.ComposeView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.setViewTreeLifecycleOwner
import androidx.savedstate.SavedStateRegistry
import androidx.savedstate.SavedStateRegistryController
import androidx.savedstate.SavedStateRegistryOwner
import androidx.savedstate.setViewTreeSavedStateRegistryOwner
import app.tomoshibi.basics.Preferences
import app.tomoshibi.basics.t
import app.tomoshibi.model.CallModel
import app.tomoshibi.ui.call.CallBubble as CallBubbleView
import app.tomoshibi.ui.call.Docked
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch

private const val TAG = "Tomoshibi"

/**
 * A call with no picture, kept in a small bubble of the app's own at the
 * edge of the screen while the app is behind others, in place of the
 * system's small window (the owner, 2026-10-22: "if there is only voice or a
 * song playing, with no picture, make a smaller floating window"; and of the
 * window on the Fold unfolded, "still too big").
 *
 * Android sizes its small window from the display's shorter side alone
 * (CallWindow `windowShape`), and nothing an app sets makes it smaller on the
 * Fold's inner screen without making it smaller everywhere; a window of the
 * app's own is the only way to a small one. A shared screen or a video
 * watched together keeps the system's window, which shows the picture: the
 * bubble is for a call with nothing to see.
 *
 * Drawing over other apps needs the person's leave ("Display over other
 * apps"). The call asks once, in its own words, the first time it has no
 * picture (CallMedia `offerBubble`); without the leave the system's window
 * opens as before, and nothing asks again.
 *
 * Main thread only.
 */
object CallBubble {
    private const val OFFERED = "call.bubble.offered"

    /** Whether the app may draw over others: snapshot state, read again each time the app comes to the front. */
    var allowed by mutableStateOf(false)
        private set

    private var window: BubbleWindow? = null

    fun refresh(context: Context) {
        allowed = Settings.canDrawOverlays(context)
    }

    /** Whether the leave is still to be asked for: never asked, not given, and a small window to fall back on. */
    fun mayOffer(context: Context): Boolean {
        refresh(context)
        if (allowed || !CallWindow.supported) return false
        return !context.getSharedPreferences(Preferences.FILE, Context.MODE_PRIVATE).getBoolean(OFFERED, false)
    }

    /** Asked: not to be asked again, whatever the answer. */
    fun offered(context: Context) {
        context.getSharedPreferences(Preferences.FILE, Context.MODE_PRIVATE).edit().putBoolean(OFFERED, true).apply()
    }

    /** The system's page for this app's "Display over other apps". */
    fun askToBeAllowed(context: Context) {
        Foreground.start(Intent(Settings.ACTION_MANAGE_OVERLAY_PERMISSION, Uri.parse("package:${context.packageName}")))
    }

    val showing: Boolean get() = window != null

    /** Puts the bubble up for [call]; a tap on it runs [open]. False where Android would not have it. */
    fun show(context: Context, call: CallModel, open: () -> Unit): Boolean {
        if (window != null) return true
        refresh(context)
        if (!allowed) return false
        return try {
            window = BubbleWindow(context.applicationContext, call, open) { window = null }.also { it.add() }
            Log.i(TAG, "call bubble shown")
            true
        } catch (fault: RuntimeException) {
            // The leave taken back between the check and this, or a window
            // refused for any other reason: the call goes on without one.
            Log.w(TAG, "the call bubble would not open: ${fault.message}")
            window = null
            false
        }
    }

    /** Takes the bubble down, after it has finished leaving. */
    fun hide() {
        window?.leave()
    }
}

/**
 * The bubble's window: a view of its own over every app, holding the card
 * (ui/call/Bubble.kt), moved by a finger and settling against the nearer
 * side when it is let go, kept clear of the system's bars and the cut-out,
 * and put back against its side when the screen changes under it: a Fold
 * opened or folded, a phone turned.
 *
 * The finger is followed here, in screen coordinates, and not in the card:
 * a window moved under the finger that moves it sees each next touch in its
 * own new coordinates, and Compose's drag, measured in those, shook.
 */
@SuppressLint("ViewConstructor")
private class BubbleWindow(
    private val context: Context,
    call: CallModel,
    private val open: () -> Unit,
    private val gone: () -> Unit,
) : FrameLayout(context) {
    private val manager = context.getSystemService(WindowManager::class.java)
    private val shown = MutableTransitionState(false)
    private var touched by mutableStateOf(false)
    private var docked by mutableStateOf(BubblePlace.side)
    private val owner = OverlayOwner()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var settling: ValueAnimator? = null
    private var leaving: Job? = null

    private val params = WindowManager.LayoutParams(
        WindowManager.LayoutParams.WRAP_CONTENT,
        WindowManager.LayoutParams.WRAP_CONTENT,
        WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
        // Never the keyboard's, and touches outside it go to whatever is under.
        WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
            WindowManager.LayoutParams.FLAG_HARDWARE_ACCELERATED,
        PixelFormat.TRANSLUCENT,
    ).apply {
        gravity = Gravity.TOP or Gravity.START
        title = "Tomoshibi call"
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) fitInsetsTypes = 0
    }

    private val slop = ViewConfiguration.get(context).scaledTouchSlop
    private var downX = 0f
    private var downY = 0f
    private var startX = 0
    private var startY = 0
    private var dragging = false

    init {
        val compose = ComposeView(context).apply {
            setViewTreeLifecycleOwner(owner)
            setViewTreeSavedStateRegistryOwner(owner)
            setContent { CallBubbleView(call, shown, touched, docked) }
        }
        setViewTreeLifecycleOwner(owner)
        setViewTreeSavedStateRegistryOwner(owner)
        addView(compose)
        contentDescription = t("Back to the call")
        isClickable = true
        importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_YES
    }

    fun add() {
        owner.start()
        val (x, y) = place(width = 0, height = 0, initial = true)
        params.x = x
        params.y = y
        manager.addView(this, params)
        shown.targetState = true
    }

    /** Leaves as it arrived, then takes the window away once the departure has been seen. */
    fun leave() {
        if (leaving != null) return
        shown.targetState = false
        leaving = scope.launch {
            snapshotFlow { shown.isIdle && !shown.currentState }.collect { done ->
                if (done) {
                    remove()
                    leaving?.cancel()
                }
            }
        }
    }

    private fun remove() {
        settling?.cancel()
        try {
            manager.removeView(this)
        } catch (_: IllegalArgumentException) {
        }
        owner.stop()
        scope.coroutineContext[Job]?.cancel()
        gone()
        Log.i(TAG, "call bubble gone")
    }

    @SuppressLint("ClickableViewAccessibility")
    override fun dispatchTouchEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                settling?.cancel()
                downX = event.rawX
                downY = event.rawY
                startX = params.x
                startY = params.y
                dragging = false
                touched = true
            }
            MotionEvent.ACTION_MOVE -> {
                val dx = event.rawX - downX
                val dy = event.rawY - downY
                if (!dragging && dx * dx + dy * dy > slop * slop) {
                    dragging = true
                    touched = false
                }
                if (dragging) {
                    params.x = startX + dx.toInt()
                    params.y = startY + dy.toInt()
                    update()
                }
            }
            MotionEvent.ACTION_UP -> {
                touched = false
                if (dragging) settle() else performClick()
            }
            MotionEvent.ACTION_CANCEL -> {
                touched = false
                if (dragging) settle()
            }
        }
        return true
    }

    override fun performClick(): Boolean {
        super.performClick()
        open()
        return true
    }

    override fun onInitializeAccessibilityNodeInfo(info: AccessibilityNodeInfo) {
        super.onInitializeAccessibilityNodeInfo(info)
        info.className = android.widget.Button::class.java.name
    }

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        // Wider while it says a share has started: it grows away from its
        // side, so the side it is against stays where it was.
        if (settling == null && !dragging && isAttachedToWindow) {
            val (x, y) = place(w, h, initial = false)
            params.x = x
            params.y = y
            update()
        }
    }

    override fun onConfigurationChanged(newConfig: Configuration?) {
        super.onConfigurationChanged(newConfig)
        // A Fold opened or closed, or a phone turned: the same side and the
        // same height on it, as a share of the screen, on the screen it is
        // on now.
        post {
            if (!isAttachedToWindow) return@post
            val (x, y) = place(width, height, initial = true)
            params.x = x
            params.y = y
            update()
        }
    }

    /** Let go: against the nearer side, inside the bars, in the kit's arrival curve. */
    private fun settle() {
        val area = area()
        val middle = params.x + width / 2
        BubblePlace.side = if (middle < area.centerX()) Docked.Start else Docked.End
        docked = BubblePlace.side
        val targetX = if (docked == Docked.Start) area.left else area.right - width
        val targetY = params.y.coerceIn(area.top, (area.bottom - height).coerceAtLeast(area.top))
        val span = (area.bottom - height - area.top).coerceAtLeast(1)
        BubblePlace.height = (targetY - area.top).toFloat() / span
        val fromX = params.x
        val fromY = params.y
        settling = ValueAnimator.ofFloat(0f, 1f).apply {
            duration = 320
            interpolator = PathInterpolator(0.2f, 0.9f, 0.3f, 1f)
            addUpdateListener {
                val f = it.animatedValue as Float
                params.x = (fromX + (targetX - fromX) * f).toInt()
                params.y = (fromY + (targetY - fromY) * f).toInt()
                update()
            }
            addListener(object : android.animation.AnimatorListenerAdapter() {
                override fun onAnimationEnd(animation: android.animation.Animator) {
                    settling = null
                }
            })
            start()
        }
    }

    /** Where the bubble goes: its side, and its remembered height on it, or where it is now. */
    private fun place(width: Int, height: Int, initial: Boolean): Pair<Int, Int> {
        val area = area()
        val x = if (docked == Docked.Start) area.left else area.right - width
        val span = (area.bottom - height - area.top).coerceAtLeast(0)
        val y = if (initial) area.top + (span * BubblePlace.height).toInt() else params.y.coerceIn(area.top, area.top + span)
        return x to y
    }

    /** The part of the screen the bubble may sit in: inside the system's bars and the cut-out. */
    private fun area(): Rect {
        val metrics = manager.currentWindowMetricsCompat()
        val bounds = metrics.first
        val insets = metrics.second
        return Rect(bounds.left + insets.left, bounds.top + insets.top, bounds.right - insets.right, bounds.bottom - insets.bottom)
    }

    private fun WindowManager.currentWindowMetricsCompat(): Pair<Rect, Rect> {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val metrics = currentWindowMetrics
            val insets = metrics.windowInsets.getInsetsIgnoringVisibility(WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout())
            return Rect(metrics.bounds) to Rect(insets.left, insets.top, insets.right, insets.bottom)
        }
        val display = android.util.DisplayMetrics()
        @Suppress("DEPRECATION")
        defaultDisplay.getRealMetrics(display)
        val density = display.density
        return Rect(0, 0, display.widthPixels, display.heightPixels) to Rect(0, (24 * density).toInt(), 0, (48 * density).toInt())
    }

    private fun update() {
        if (!isAttachedToWindow) return
        try {
            manager.updateViewLayout(this, params)
        } catch (_: IllegalArgumentException) {
        }
    }
}

/** Where the person last left the bubble, for the next one in this process. */
private object BubblePlace {
    var side = Docked.End
    /** How far down its side, from the top of the space it may use to the bottom. */
    var height = 0.3f
}

/** The lifecycle a view outside any activity needs for Compose to draw it. */
private class OverlayOwner : SavedStateRegistryOwner {
    private val registry = LifecycleRegistry(this)
    private val saved = SavedStateRegistryController.create(this)
    override val lifecycle: Lifecycle get() = registry
    override val savedStateRegistry: SavedStateRegistry get() = saved.savedStateRegistry

    fun start() {
        saved.performRestore(null)
        registry.currentState = Lifecycle.State.RESUMED
    }

    fun stop() {
        registry.currentState = Lifecycle.State.DESTROYED
    }
}
