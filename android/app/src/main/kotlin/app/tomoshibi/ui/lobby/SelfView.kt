package app.tomoshibi.ui.lobby

import android.content.Context
import android.util.Log
import androidx.camera.view.PreviewView
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import app.tomoshibi.basics.Theme
import app.tomoshibi.media.CameraCapture
import app.tomoshibi.media.DeviceAccess
import app.tomoshibi.media.DeviceKind
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.DeviceMemory
import app.tomoshibi.model.RemoteFeature
import app.tomoshibi.ui.kit.Appear
import app.tomoshibi.ui.kit.Backdrop
import app.tomoshibi.ui.kit.ContinuousShape
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.collectLatest

/**
 * The camera, live, where somebody has it on; a flat ground otherwise
 * (`CameraStage`). Mark it as a glass source where glass floats over it.
 *
 * Framed (a corner given), it is a card on a page, the lobby's or the start
 * sheet's, and with the camera off it is the grey of a camera-off tile in the
 * call (Theme.surface), which is what the person will look like when they
 * join: the call's darkest ground there vanished into the black page round
 * the phone's lobby card. Filling the screen it is the screen's ground
 * ([Backdrop]), so the glass card over it stands out of it.
 *
 * [glassWhenOff]: with the camera off, a pane of glass with children drawn
 * on it instead of the grey ([PreviewGlass]): the phone lobby's card asks for
 * it, as on the iPhone, and the start sheet's preview keeps the grey.
 * [glassHeld]: the card is covered, and its drawing holds still.
 */
@Composable
fun CameraStage(
    app: AppModel,
    modifier: Modifier = Modifier,
    cornerRadius: Dp = 0.dp,
    glassWhenOff: Boolean = false,
    glassHeld: Boolean = false,
) {
    val shape = if (cornerRadius > 0.dp) ContinuousShape(cornerRadius) else RectangleShape
    Box(modifier.clip(shape)) {
        if (cornerRadius > 0.dp && glassWhenOff) {
            // The glass and its people go as the camera comes on, and the
            // grey a picture stands on comes with it; the other way round as
            // it goes off. Both fade, so the card is never seen empty of
            // either, and the drawing is not drawn at all under a picture.
            Appear(!app.startWithCamera, Modifier.matchParentSize(), enter = fadeIn(Theme.settle()), exit = fadeOut(Theme.leave())) {
                PreviewGlass(cornerRadius, glassHeld, Modifier.fillMaxSize())
            }
            Appear(app.startWithCamera, Modifier.matchParentSize(), enter = fadeIn(Theme.settle()), exit = fadeOut(Theme.leave())) {
                Box(Modifier.fillMaxSize().background(Theme.surface))
            }
        } else if (cornerRadius > 0.dp) {
            Box(Modifier.matchParentSize().background(Theme.surface))
        } else {
            Backdrop(Modifier.matchParentSize())
        }
        Appear(
            app.startWithCamera,
            Modifier.matchParentSize(),
            enter = fadeIn(Theme.settle()),
            exit = fadeOut(Theme.settle()),
        ) { SelfView(Modifier.fillMaxSize()) }
    }
}

/**
 * Every picture of yourself on screen before a call, and the one camera
 * they are fed from.
 *
 * One camera for all of them, and only the latest fed. Each picture had a
 * camera of its own on Apple (`@State private var preview = CameraCapture()`),
 * and the start sheet over the lobby opened a second while the lobby's card
 * still held the first. Here one camera is opened once: a phone opens a
 * camera for one user at a time, and a second preview, or the call's own
 * camera, would wait on the first or take it. So the preview stops the
 * moment a join is under way, before the call can turn its camera on, and
 * never again after that until it is wanted.
 */
internal object SelfViews {
    /** The pictures on screen, the one drawn last at the end: only that one is fed. */
    val shown = mutableStateListOf<PreviewView>()

    private var capture: CameraCapture? = null

    fun capture(context: Context): CameraCapture = capture ?: CameraCapture(context).also { capture = it }

    /** Stops the preview if it is running, which is the only time a stop is safe (see above). */
    fun stop() {
        val running = capture ?: return
        if (running.running) running.stop()
    }
}

/**
 * Feeds the latest picture of yourself while somebody wants the camera and
 * is not on the way into a call, at the app's root, once.
 */
@Composable
fun FeedSelfViews(app: AppModel) {
    val context = LocalContext.current.applicationContext
    // Not while the app is out of sight: the system takes the camera from an
    // app in the background, and the card would come back black.
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    val state = lifecycle.currentStateFlow.collectAsState()
    LaunchedEffect(app) {
        snapshotFlow {
            val seen = state.value.isAtLeast(Lifecycle.State.STARTED)
            SelfViews.shown.lastOrNull()?.takeIf { seen && app.startWithCamera && !app.joining && app.call == null }
        }.collectLatest { view ->
            if (view == null) {
                SelfViews.stop()
                return@collectLatest
            }
            try {
                // A preview of a camera the system has not been allowed to
                // open asks, as the iPhone's capture session does the first
                // time it runs.
                DeviceAccess.ensure(DeviceKind.Camera)
                val capture = SelfViews.capture(context)
                // Blurred as the call will send it, where blur is chosen:
                // not while the deployment has blur switched off, when the
                // call will not blur it either (CallModel `blurKept`).
                capture.blurring = app.deviceMemory.blur && app.remote.allows(RemoteFeature.BackgroundBlur)
                capture.showPreview(view.surfaceProvider)
                if (!capture.running) {
                    // The camera the call will start on: the one last
                    // picked, where it is still there, and the front one
                    // otherwise (CallModel `restoreDevices`). The web
                    // previews the remembered one for the same reason
                    // (PreJoin.tsx ~373: "a preview of another camera would
                    // be a rehearsal of the wrong picture").
                    val remembered = app.deviceMemory.restorable(DeviceMemory.Kind.Camera, capture.cameras().map { it.id })
                    capture.start(remembered)
                }
            } catch (fault: CancellationException) {
                throw fault
            } catch (fault: Exception) {
                // Refused, or no camera: the flat ground stays, as it does
                // on Apple where the session runs and shows nothing.
                Log.w("Tomoshibi", "the preview did not start: ${fault.javaClass.simpleName}")
            }
        }
    }
    DisposableEffect(app) { onDispose { SelfViews.stop() } }
}

/**
 * The picture of yourself, mirrored as a front camera's is (CameraX's own
 * view turns, mirrors and crops it as the camera delivers it). Drawn into a
 * texture rather than a surface of its own, so that it is clipped to the
 * card it sits in and fades with it.
 */
@Composable
private fun SelfView(modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val view = remember {
        PreviewView(context).apply {
            implementationMode = PreviewView.ImplementationMode.COMPATIBLE
            scaleType = PreviewView.ScaleType.FILL_CENTER
        }
    }
    DisposableEffect(view) {
        SelfViews.shown.add(view)
        onDispose { SelfViews.shown.remove(view) }
    }
    AndroidView({ view }, modifier)
}
