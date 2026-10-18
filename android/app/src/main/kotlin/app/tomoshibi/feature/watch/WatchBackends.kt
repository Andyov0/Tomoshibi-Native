package app.tomoshibi.feature.watch

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Color
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.view.TextureView
import android.webkit.JavascriptInterface
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.annotation.OptIn
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.setValue
import androidx.media3.common.C
import androidx.media3.common.Format
import androidx.media3.common.MediaItem
import androidx.media3.common.MimeTypes
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.Tracks
import androidx.media3.common.VideoSize
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DefaultHttpDataSource
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import app.tomoshibi.model.Rehearsal
import org.json.JSONArray
import org.json.JSONObject

private const val TAG = "Tomoshibi"

/**
 * A file or an HLS playlist, from where it is or relayed: Media3's player,
 * as AVPlayer is on Apple (WatchBackends.swift `AVBackend`) and hls.js and
 * the video element are on the web.
 *
 * Kept here rather than in a view, so the picture survives the stage being
 * redrawn: the stage and the strip trading places only lends the player
 * another surface ([attach]).
 */
@OptIn(UnstableApi::class)
class ExoBackend(
    context: Context,
    private val url: String,
    private val hls: Boolean,
    private val capped: Boolean,
    private val owner: WatchPlayer,
) : WatchBackend {
    val player: ExoPlayer = ExoPlayer.Builder(context)
        .setMediaSourceFactory(
            // A redirect from http to https is followed, as AVPlayer and a
            // browser follow it; Media3 refuses one by default, and a link
            // pasted with the old scheme is common.
            DefaultMediaSourceFactory(DefaultHttpDataSource.Factory().setAllowCrossProtocolRedirects(true)),
        )
        .build()

    /** The picture's shape, width over height, for the stage to letterbox it as AVPlayerLayer's `resizeAspect` does. */
    var aspect by mutableFloatStateOf(16f / 9f)
        private set

    /** What this app last told the player, so a reload keeps playing only what was playing. */
    private var asked: Boolean? = null
    private var prepared = false
    private var held = false
    private var largest = 0 to 0
    /**
     * The viewer's own ceiling, read when the video is opened, as the web
     * reads it (WatchScreen.tsx: "read when the video is opened, so a change
     * on the account page applies from the next").
     */
    private val ceiling = WatchCeiling.stored(Rehearsal.choices)

    private val listener = object : Player.Listener {
        override fun onPlaybackStateChanged(state: Int) {
            when (state) {
                Player.STATE_READY -> if (!prepared) {
                    prepared = true
                    owner.backendPlayable(this@ExoBackend)
                }
                Player.STATE_ENDED -> owner.backendEnded(this@ExoBackend)
                else -> {}
            }
        }

        override fun onIsPlayingChanged(isPlaying: Boolean) {
            if (isPlaying) owner.backendBegan(this@ExoBackend)
        }

        override fun onPlayerError(error: PlaybackException) {
            owner.backendFailed(this@ExoBackend, "error ${error.errorCodeName}", format = unopenable(error))
        }

        override fun onVideoSizeChanged(videoSize: VideoSize) {
            if (videoSize.width > 0 && videoSize.height > 0) {
                aspect = videoSize.width * videoSize.pixelWidthHeightRatio / videoSize.height
            }
        }

        override fun onTracksChanged(tracks: Tracks) {
            if (hls && ceiling != WatchCeiling.Auto && !held) hold(tracks)
        }
    }

    init {
        player.addListener(listener)
        load(null)
    }

    private fun load(atMs: Long?) {
        prepared = false
        held = false
        // Said to be a playlist, whatever its address looks like: the relay's
        // is `/api/watch/media?t=...`, with nothing in it that names HLS.
        val item = MediaItem.Builder().setUri(url).apply { if (hls) setMimeType(MimeTypes.APPLICATION_M3U8) }.build()
        if (atMs != null) player.setMediaItem(item, atMs) else player.setMediaItem(item)
        player.prepare()
        if (atMs != null && asked == true) player.play()
    }

    /**
     * Holds a playlist to the viewer's ceiling (WatchScreen.tsx, `capping` on
     * hls.js's `autoLevelCapping`): the highest rung the ceiling allows,
     * ranked by bitrate as hls.js ranks its levels, becomes the most this
     * player may fetch. A bitrate rather than a size, because the size is
     * already the stage's to set ([shown]), which this must only ever lower:
     * the player honours both, so the lower wins. Half way to the rung above,
     * as on Apple, whose player kept a margin under an exact limit; the rung
     * above is never allowed this way, which is the point.
     */
    private fun hold(tracks: Tracks) {
        val formats = tracks.groups.filter { it.type == C.TRACK_TYPE_VIDEO }
            .flatMap { group -> (0 until group.length).map { group.getTrackFormat(it) } }
        if (formats.isEmpty()) return
        held = true
        fun rate(format: Format): Int = listOf(format.peakBitrate, format.averageBitrate, format.bitrate).firstOrNull { it > 0 } ?: 0
        val ranked = formats.sortedBy { rate(it) }
        val levels = ranked.map { format ->
            WatchCeiling.Level(format.width.takeIf { it > 0 }?.toDouble(), format.height.coerceAtLeast(0).toDouble())
        }
        val cap = ceiling.cap(levels) ?: return
        val allowed = rate(ranked[cap])
        if (allowed <= 0) return
        val above = ranked.drop(cap + 1).map { rate(it) }.firstOrNull { it > allowed } ?: (allowed * 2)
        val most = ((allowed.toLong() + above) / 2).toInt()
        player.trackSelectionParameters = player.trackSelectionParameters.buildUpon().setMaxVideoBitrate(most).build()
        Log.i(TAG, "watch: ceiling ${ceiling.word}, at most $most b/s, up to ${ranked[cap].width}x${ranked[cap].height}")
    }

    override fun reload() {
        load(player.currentPosition.coerceAtLeast(0))
    }

    /** The size the picture is shown at, in pixels, for a player held to it. */
    fun shown(width: Int, height: Int) {
        if (!capped || width <= 0 || height <= 0) return
        // Only ever grown: shrinking the ceiling while a larger rendition is
        // buffered throws away what was fetched, and the stage and the strip
        // trade places whenever somebody pins a picture.
        val grown = maxOf(largest.first, width) to maxOf(largest.second, height)
        if (grown == largest) return
        largest = grown
        player.trackSelectionParameters = player.trackSelectionParameters.buildUpon()
            .setViewportSize(grown.first, grown.second, false).build()
    }

    /** Draws into [view]; the previous surface, if any, is let go by the player. */
    fun attach(view: TextureView) {
        player.setVideoTextureView(view)
    }

    /** Lets [view] go, if it is still the one drawn into. */
    fun detach(view: TextureView) {
        player.clearVideoTextureView(view)
    }

    override val ready: Boolean get() = prepared
    override val time: Double get() = player.currentPosition.coerceAtLeast(0) / 1000.0
    override val playing: Boolean get() = player.playWhenReady && player.playbackState != Player.STATE_ENDED
    override val rate: Double get() = player.playbackParameters.speed.toDouble()
    override val nudges = true
    override val youtube = false
    override val flowing: Boolean get() = prepared && player.playbackState == Player.STATE_READY

    override fun play() {
        asked = true
        player.play()
    }

    override fun pause() {
        asked = false
        player.pause()
    }

    /**
     * Exactly where asked, as a video element's `currentTime` is set (Media3's
     * default seek is exact): a seek that lands up to a keyframe away would be
     * a gap the keeper then spends seconds closing.
     */
    override fun seek(to: Double) {
        player.seekTo((maxOf(0.0, to) * 1000).toLong())
    }

    override fun setRate(rate: Double) {
        // Unlike AVPlayer's `rate`, a speed set on a paused player does not
        // start it, so it is set whether it plays or not.
        if (player.playbackParameters.speed != rate.toFloat()) player.setPlaybackSpeed(rate.toFloat())
    }

    override fun setVolume(level: Double, muted: Boolean) {
        player.volume = if (muted) 0f else level.toFloat().coerceIn(0f, 1f)
    }

    override fun stop() {
        player.removeListener(listener)
        player.release()
    }

    companion object {
        /**
         * The errors for a file the player cannot open or decode, rather than
         * one it could not fetch: no extractor knows its container, or no
         * decoder on this phone takes its codec (WatchBackends.swift
         * `unopenable`). No relay cures either.
         */
        fun unopenable(error: PlaybackException): Boolean = error.errorCode in setOf(
            PlaybackException.ERROR_CODE_PARSING_CONTAINER_UNSUPPORTED,
            PlaybackException.ERROR_CODE_PARSING_MANIFEST_UNSUPPORTED,
            PlaybackException.ERROR_CODE_DECODER_INIT_FAILED,
            PlaybackException.ERROR_CODE_DECODER_QUERY_FAILED,
            PlaybackException.ERROR_CODE_DECODING_FAILED,
            PlaybackException.ERROR_CODE_DECODING_FORMAT_EXCEEDS_CAPABILITIES,
            PlaybackException.ERROR_CODE_DECODING_FORMAT_UNSUPPORTED,
        )
    }
}

/**
 * YouTube's own embedded player, where YouTube can be reached (youtube.ts;
 * WatchBackends.swift `YouTubeBackend`): its media addresses are bound to
 * whoever resolved them, so nobody can be handed one to play, and somebody
 * who can reach YouTube plays it through YouTube's player instead, driven
 * through its IFrame API.
 *
 * The page is given the deployment's address as its own, so the player is
 * embedded from the same origin the web client embeds it from, and YouTube
 * sees one site asking whichever client is in front of it.
 */
@SuppressLint("SetJavaScriptEnabled")
class YouTubeBackend(context: Context, id: String, base: String, private val owner: WatchPlayer) : WatchBackend {
    val web = WebView(context)
    private val main = Handler(Looper.getMainLooper())
    override var ready = false
        private set
    private var reported = 0.0
    private var reportedAt = 0L
    private var state = -1
    private var gone = false

    init {
        web.setBackgroundColor(Color.BLACK)
        web.settings.javaScriptEnabled = true
        // Played when asked, as the web's page is once somebody has pressed
        // anything on it; the asking is the show's, not a press here.
        web.settings.mediaPlaybackRequiresUserGesture = false
        web.settings.setSupportMultipleWindows(false)
        web.isVerticalScrollBarEnabled = false
        web.isHorizontalScrollBarEnabled = false
        web.addJavascriptInterface(Relay(this), "tomoshibiWatch")
        // The page and the player YouTube puts in it; nothing else is opened
        // in the view: a press on YouTube's name in the frame would otherwise
        // take the stage to YouTube's site.
        web.webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean = request.isForMainFrame
        }
        val origin = Regex("^([a-zA-Z][a-zA-Z0-9+.-]*://[^/?#]+)").find(base)?.groupValues?.get(1) ?: ""
        web.loadDataWithBaseURL(base, page(id, origin, API), "text/html", "utf-8", null)
    }

    override val time: Double
        get() {
            // The page says where it is ten times a second; between two words
            // a playing player has moved on by however long it has been.
            if (state != PLAYING) return reported
            return reported + (System.nanoTime() / 1_000_000 - reportedAt) / 1000.0
        }
    override val playing: Boolean get() = state == PLAYING || state == BUFFERING
    override val rate = 1.0
    override val nudges = false
    override val youtube = true
    override val flowing: Boolean get() = state == PLAYING

    override fun play() = run("watch.play()")
    override fun pause() = run("watch.pause()")
    override fun seek(to: Double) = run("watch.seek(${maxOf(0.0, to)})")
    override fun setRate(rate: Double) {}
    override fun setVolume(level: Double, muted: Boolean) = run("watch.volume($level, $muted)")
    override fun setTappable(on: Boolean) = run("watch.tappable($on)")

    override fun stop() {
        gone = true
        run("watch.destroy()")
        web.removeJavascriptInterface("tomoshibiWatch")
        web.stopLoading()
        (web.parent as? android.view.ViewGroup)?.removeView(web)
        web.destroy()
    }

    private fun run(script: String) {
        if (!gone) web.evaluateJavascript("window.watch && $script", null)
    }

    internal fun heard(body: String) {
        if (gone) return
        val message = try {
            JSONObject(body)
        } catch (_: Exception) {
            return
        }
        when (message.optString("t")) {
            "ready" -> {
                ready = true
                owner.backendPlayable(this)
            }
            "tick", "state" -> {
                if (message.has("time")) {
                    reported = message.optDouble("time", reported)
                    reportedAt = System.nanoTime() / 1_000_000
                }
                val was = state
                state = message.optInt("state", state)
                if (message.optString("t") == "state" || state != was) {
                    if (state == PLAYING) owner.backendBegan(this)
                    if (state == ENDED && was != ENDED) owner.backendEnded(this)
                }
            }
            "error" -> owner.youtubeRefused(this, message.optInt("code", 0))
            "unreachable" -> owner.youtubeUnreachable(this)
        }
    }

    /**
     * What the page says, carried to the main thread: the page's messages
     * arrive on the web view's own thread. Holds the backend only as long as
     * the view holds this.
     */
    private class Relay(backend: YouTubeBackend) {
        private val backend = java.lang.ref.WeakReference(backend)

        @JavascriptInterface
        fun post(body: String) {
            val target = backend.get() ?: return
            target.main.post { target.heard(body) }
        }
    }

    companion object {
        /** Long enough for a slow connection; short enough that a blocked one is not a long wait at a black tile (youtube.ts `LOAD_MS`). */
        const val LOAD_MS = 6000
        /** YT.PlayerState, by the numbers the API sends. */
        const val ENDED = 0
        const val PLAYING = 1
        const val BUFFERING = 3
        const val API = "https://www.youtube.com/iframe_api"

        /** Every value as a JSON string, so nothing in one can end the script. */
        private fun quoted(text: String): String = JSONArray().put(text).toString().removePrefix("[").removeSuffix("]")

        /** The page the player is put in: the Apple apps' own, its messages handed to this app's bridge rather than WebKit's. */
        fun page(id: String, origin: String, api: String): String = """
            <!doctype html><html><head><meta charset="utf-8">
            <meta name="viewport" content="width=device-width,initial-scale=1">
            <style>
            html,body{margin:0;width:100%;height:100%;background:#000;overflow:hidden}
            #player,iframe,video{position:absolute;inset:0;width:100%;height:100%;border:0}
            body.locked iframe,body.locked video{pointer-events:none}
            </style></head><body class="locked"><div id="player"></div><script>
            (() => {
              const say = (message) => window.tomoshibiWatch.post(JSON.stringify(message));
              let player, ready = false, gaveUp = false, sound = null;
              const hear = () => {
                if (!player || !ready || !sound) return;
                player.setVolume(Math.round(sound[0] * 100));
                sound[1] ? player.mute() : player.unMute();
              };
              // The API's script either arrives or it does not; from where it
              // does not, the connection usually hangs rather than fails.
              const fail = () => { if (!gaveUp && !ready) { gaveUp = true; say({ t: "unreachable" }); } };
              const timer = setTimeout(fail, $LOAD_MS);
              window.onYouTubeIframeAPIReady = () => {
                clearTimeout(timer);
                if (gaveUp) return;
                player = new YT.Player("player", {
                  videoId: ${quoted(id)}, width: "100%", height: "100%",
                  playerVars: { controls: 0, disablekb: 1, playsinline: 1, rel: 0, modestbranding: 1, iv_load_policy: 3, origin: ${quoted(origin)} },
                  events: {
                    onReady: () => { ready = true; hear(); say({ t: "ready" }); },
                    onStateChange: (event) => say({ t: "state", state: event.data, time: player.getCurrentTime() }),
                    onError: (event) => say({ t: "error", code: event.data }),
                  },
                });
              };
              const script = document.createElement("script");
              script.src = ${quoted(api)};
              script.async = true;
              script.onerror = () => { clearTimeout(timer); fail(); };
              document.head.append(script);
              setInterval(() => { if (ready) say({ t: "tick", time: player.getCurrentTime(), state: player.getPlayerState() }); }, 100);
              window.watch = {
                play: () => player && player.playVideo(),
                pause: () => player && player.pauseVideo(),
                seek: (to) => player && player.seekTo(to, true),
                // Kept until the player is ready to be told: asked before
                // then, a volume would be lost and the video start at full
                // sound.
                volume: (level, muted) => { sound = [level, muted]; hear(); },
                tappable: (on) => document.body.classList.toggle("locked", !on),
                destroy: () => { ready = false; player && player.destroy(); },
              };
            })();
            </script></body></html>
        """.trimIndent()
    }
}
