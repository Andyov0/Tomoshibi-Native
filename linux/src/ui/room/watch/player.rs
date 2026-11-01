//! The player of watching together: GStreamer's playbin, given sinks of the
//! app's own, so that the picture is drawn the way every remote picture is
//! (render::VideoView, fed packed I420 on GStreamer's thread as the core
//! feeds it on its own) and the sound goes into the call's own mixer
//! (`Media::local_sound`), where the echo canceller hears it as it hears
//! everybody's voice: played straight to the speakers, the film would be
//! heard by the microphone and sent back into the call.
//!
//! Plays what a video element plays from where it is: a file or an HLS
//! playlist, by its address, or through the deployment's relay for whoever
//! cannot reach it (watch-api.ts `proxy`), with the account's session for a
//! film the relay plays only to the account that chose it. YouTube's own
//! player needs a web view, which this app does not carry: a YouTube video
//! says that this app cannot play that kind of video, and everybody else
//! goes on watching it.
//!
//! The keeper (`WatchKeeper`) decides; this only reads the player and does
//! what it is told. It does not run a little fast or slow (`nudges` is
//! false): a rate change through GStreamer flushes the pipeline or bends the
//! sound's pitch, and the keeper holds a player that cannot nudge to the
//! show by jumping once it is a second out, as it holds YouTube's.

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, OnceLock,
    },
};

use gst::prelude::*;
use gstreamer as gst;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use gstreamer_video::prelude::VideoFrameExt as _;
use tomoshibi_core::call::{PcmSink, RemoteFrame, VideoSink};
use tomoshibi_protocol::{
    api::{ResolvedPlayKind, ResolvedVideo},
    watch::{WatchKeepStep, WatchPlayerReading, WatchVideo},
};

use super::Engine;
use crate::{app::log, render::VideoView};

/// Where the video's sound goes.
pub enum Sound {
    /// Into the call's mixer, beside everybody's voice.
    Mixer(Arc<dyn PcmSink>),
    /// The system's own output: only where the devices have no mixer to
    /// give, which a build of the media module without one is.
    System,
    /// Nowhere, in a test run that plays nothing (TOMOSHIBI_MUTE).
    Nowhere,
}

/// Why the video does not play here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lost {
    /// A kind of video this player cannot open, from where it is or through
    /// the relay alike: YouTube's, or a format with no plugin here.
    Format,
    /// The site would not play it here.
    Refused,
    /// Where it is cannot be reached from here, and there is no relay.
    Unreachable,
}

/// GStreamer, started once for the process; `None` where it could not be.
fn started() -> bool {
    static STARTED: OnceLock<bool> = OnceLock::new();
    *STARTED.get_or_init(|| {
        from_the_image();
        match gst::init() {
            Ok(()) => true,
            Err(fault) => {
                log(&format!("GStreamer did not start: {fault}"));
                false
            }
        }
    })
}

/// Run from the AppImage, GStreamer reads the plugins the image carries
/// (appimage-inside.sh) and keeps its registry of them apart from the
/// system's. Its own default is the build machine's plugin directory, which
/// on another system is that system's plugins, built against another
/// GStreamer than the one in the image. Set before GStreamer starts, and
/// only what AppRun has not set already.
fn from_the_image() {
    let Some(appdir) = std::env::var_os("APPDIR").map(std::path::PathBuf::from) else {
        return;
    };
    let plugins = appdir.join("usr/lib/gstreamer-1.0");
    if !plugins.is_dir() {
        return;
    }
    let set = |name: &str, value: &std::ffi::OsStr| {
        if std::env::var_os(name).is_none() {
            std::env::set_var(name, value);
        }
    };
    set("GST_PLUGIN_SYSTEM_PATH_1_0", plugins.as_os_str());
    set("GST_PLUGIN_PATH_1_0", std::ffi::OsStr::new(""));
    set(
        "GST_PLUGIN_SCANNER_1_0",
        appdir
            .join("usr/lib/gstreamer1.0/gstreamer-1.0/gst-plugin-scanner")
            .as_os_str(),
    );
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".cache"))
        });
    if let Some(cache) = cache {
        let registry = cache
            .join("tomoshibi")
            .join(format!("gstreamer-registry-{}.bin", std::env::consts::ARCH));
        set("GST_REGISTRY_1_0", registry.as_os_str());
    }
}

struct State {
    /// The pipeline has prerolled: it can say where it is.
    ready: bool,
    lost: Option<Lost>,
    /// Played through the deployment's relay.
    relayed: bool,
    /// The keeper's last word: nothing to hold to yet, and where the clock
    /// under the video should read.
    waiting: bool,
    position: f64,
    /// Asked to play: the pipeline is going to, or is, playing.
    playing: bool,
    told_ready: bool,
    volume: f64,
    muted: bool,
}

/// One video's player; one per video, as the web keys its player by the
/// video's key.
pub struct Player {
    pub key: String,
    pub video: WatchVideo,
    pub info: ResolvedVideo,
    view: VideoView,
    pipeline: Option<gst::Element>,
    bus: RefCell<Option<gst::bus::BusWatchGuard>>,
    state: RefCell<State>,
    engine: Weak<Engine>,
    /// Bumped whenever what the screens draw of the player changes.
    generation: Cell<u64>,
    /// Frames the picture has been handed, for a test to read.
    frames: Arc<AtomicU64>,
    /// Bytes of sound handed to the mixer, for a test to read.
    sound: Arc<AtomicU64>,
}

impl Player {
    pub fn new(engine: Weak<Engine>, video: WatchVideo, info: ResolvedVideo) -> Rc<Player> {
        let view = VideoView::new();
        view.set_fit(crate::render::Fit::Contain);
        let (sound_sink, cookie) = engine
            .upgrade()
            .map(|engine| (engine.sound_sink(), engine.session_cookie()))
            .unwrap_or((Sound::Nowhere, None));
        // From where it is, unless it plays only through the relay or the
        // gateway said to start there.
        let direct = match info.play.kind {
            ResolvedPlayKind::File | ResolvedPlayKind::Hls => info.play.url.clone(),
            _ => None,
        };
        let proxy = info.proxy.as_ref().map(|proxy| proxy.url.clone());
        let youtube = info.play.kind == ResolvedPlayKind::Youtube;
        let (address, relayed) = match (info.relay, direct, proxy) {
            (true, _, Some(proxy)) => (Some(proxy), true),
            (_, Some(direct), _) => (Some(direct), false),
            (_, None, Some(proxy)) if !youtube => (Some(proxy), true),
            _ => (None, false),
        };
        let frames = Arc::new(AtomicU64::new(0));
        let sound = Arc::new(AtomicU64::new(0));
        let pipeline = match address.as_ref() {
            Some(address) if started() => build(
                address,
                &view,
                sound_sink,
                cookie,
                engine.upgrade().map(|engine| engine.deployment.base_url()),
                frames.clone(),
                sound.clone(),
            ),
            _ => None,
        };
        let lost = if pipeline.is_none() {
            Some(Lost::Format)
        } else {
            None
        };
        let player = Rc::new(Player {
            key: video.key.clone(),
            video,
            info,
            view,
            pipeline,
            bus: RefCell::new(None),
            state: RefCell::new(State {
                ready: false,
                lost,
                relayed,
                waiting: true,
                position: 0.0,
                playing: false,
                told_ready: false,
                volume: 1.0,
                muted: false,
            }),
            engine,
            generation: Cell::new(0),
            frames,
            sound,
        });
        if let Some(pipeline) = player.pipeline.as_ref() {
            player.watch_bus(pipeline);
            // Prerolled, still: the keeper says when to play.
            if pipeline.set_state(gst::State::Paused).is_err() {
                player.lose(Lost::Format);
            }
        }
        player
    }

    fn watch_bus(self: &Rc<Self>, pipeline: &gst::Element) {
        let Some(bus) = pipeline.bus() else { return };
        let weak = Rc::downgrade(self);
        let guard = bus.add_watch_local(move |_, message| {
            let Some(player) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            player.on_message(message);
            glib::ControlFlow::Continue
        });
        match guard {
            Ok(guard) => {
                self.bus.replace(Some(guard));
            }
            Err(fault) => log(&format!("the player's bus is not watched: {fault}")),
        }
    }

    fn on_message(&self, message: &gst::Message) {
        use gst::MessageView;
        match message.view() {
            MessageView::AsyncDone(_) => {
                let first = {
                    let mut state = self.state.borrow_mut();
                    let first = !state.ready;
                    state.ready = true;
                    first && !std::mem::replace(&mut state.told_ready, true)
                };
                if first {
                    self.bump();
                    // The holder's player can play it: the timeline starts
                    // now, for everybody.
                    if let Some(engine) = self.engine.upgrade() {
                        engine.player_ready(&self.key);
                    }
                }
            }
            MessageView::Eos(_) => {
                if let Some(engine) = self.engine.upgrade() {
                    engine.player_ended(&self.key);
                }
            }
            MessageView::Error(fault) => {
                let error = fault.error();
                log(&format!(
                    "the video did not play: {error} ({})",
                    fault
                        .debug()
                        .map(|debug| debug.to_string())
                        .unwrap_or_default()
                ));
                let format = error.matches(gst::CoreError::MissingPlugin)
                    || error.matches(gst::StreamError::CodecNotFound)
                    || error.matches(gst::StreamError::WrongType)
                    || error.matches(gst::StreamError::TypeNotFound)
                    || error.matches(gst::StreamError::Decode)
                    || error.matches(gst::StreamError::Demux);
                let refused = error.matches(gst::ResourceError::NotAuthorized);
                // Where it is cannot be reached: through the relay, where
                // there is one and it is not already what failed.
                let relayed = self.state.borrow().relayed;
                let proxy = self.info.proxy.as_ref().map(|proxy| proxy.url.clone());
                if !relayed && !format {
                    if let (Some(proxy), Some(pipeline)) = (proxy, self.pipeline.as_ref()) {
                        log("the video goes through the relay");
                        let _ = pipeline.set_state(gst::State::Null);
                        pipeline.set_property("uri", &proxy);
                        {
                            let mut state = self.state.borrow_mut();
                            state.relayed = true;
                            state.ready = false;
                        }
                        let _ = pipeline.set_state(gst::State::Paused);
                        self.bump();
                        return;
                    }
                }
                self.lose(if format {
                    Lost::Format
                } else if refused {
                    Lost::Refused
                } else {
                    Lost::Unreachable
                });
            }
            _ => {}
        }
    }

    fn lose(&self, lost: Lost) {
        self.state.borrow_mut().lost = Some(lost);
        if let Some(pipeline) = self.pipeline.as_ref() {
            let _ = pipeline.set_state(gst::State::Null);
        }
        self.bump();
    }

    fn bump(&self) {
        self.generation.set(self.generation.get() + 1);
        if let Some(engine) = self.engine.upgrade() {
            engine.announce();
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation.get()
    }

    /// The picture, for the stage to show.
    pub fn view(&self) -> &VideoView {
        &self.view
    }

    pub fn lost(&self) -> Option<Lost> {
        self.state.borrow().lost
    }

    pub fn waiting(&self) -> bool {
        let state = self.state.borrow();
        state.waiting && state.lost.is_none()
    }

    pub fn relayed(&self) -> bool {
        self.state.borrow().relayed
    }

    /// Where the clock under the video reads.
    pub fn position(&self) -> f64 {
        self.state.borrow().position
    }

    /// How long the video is, as the show says or, failing that, as the
    /// file does.
    pub fn duration(&self) -> f64 {
        if self.video.duration > 0.0 {
            return self.video.duration;
        }
        if self.info.duration > 0.0 {
            return self.info.duration;
        }
        self.pipeline
            .as_ref()
            .and_then(|pipeline| pipeline.query_duration::<gst::ClockTime>())
            .map(|at| at.mseconds() as f64 / 1000.0)
            .unwrap_or(0.0)
    }

    pub fn volume(&self) -> f64 {
        self.state.borrow().volume
    }

    pub fn muted(&self) -> bool {
        self.state.borrow().muted
    }

    /// This viewer's own volume for the video, and its own mute: nothing the
    /// show carries.
    pub fn set_volume(&self, level: f64) {
        let level = level.clamp(0.0, 1.0);
        {
            let mut state = self.state.borrow_mut();
            state.volume = level;
            state.muted = false;
        }
        if let Some(pipeline) = self.pipeline.as_ref() {
            pipeline.set_property("volume", level);
            pipeline.set_property("mute", false);
        }
    }

    pub fn set_muted(&self, muted: bool) {
        self.state.borrow_mut().muted = muted;
        if let Some(pipeline) = self.pipeline.as_ref() {
            pipeline.set_property("mute", muted);
        }
    }

    /// The player as the keeper reads it, four times a second.
    pub fn reading(&self) -> WatchPlayerReading {
        let state = self.state.borrow();
        let time = self
            .pipeline
            .as_ref()
            .filter(|_| state.ready)
            .and_then(|pipeline| pipeline.query_position::<gst::ClockTime>())
            .map(|at| at.mseconds() as f64 / 1000.0)
            .unwrap_or(0.0);
        WatchPlayerReading {
            // Refused for good counts as not ready, so the keeper does not
            // go on asking it.
            ready: state.ready && state.lost.is_none(),
            time,
            playing: state.playing,
            rate: 1.0,
            nudges: false,
            youtube: false,
        }
    }

    /// Does what the keeper says, in the order of its fields: play or
    /// pause, then jump.
    pub fn apply(&self, step: &WatchKeepStep) {
        {
            let mut state = self.state.borrow_mut();
            state.waiting = step.waiting;
            if let Some(position) = step.position {
                state.position = position;
            }
        }
        if step.play {
            self.play_now();
        }
        if step.pause {
            self.pause_now();
        }
        if let Some(to) = step.seek {
            self.seek_now(to);
        }
    }

    pub fn play_now(&self) {
        if self.state.borrow().lost.is_some() {
            return;
        }
        self.state.borrow_mut().playing = true;
        if let Some(pipeline) = self.pipeline.as_ref() {
            let _ = pipeline.set_state(gst::State::Playing);
        }
    }

    pub fn pause_now(&self) {
        self.state.borrow_mut().playing = false;
        if let Some(pipeline) = self.pipeline.as_ref() {
            let _ = pipeline.set_state(gst::State::Paused);
        }
    }

    /// Jumps to `to` seconds, to the frame rather than the nearest key frame:
    /// a jump that lands a second away is asked again by the keeper a moment
    /// later, and again.
    pub fn seek_now(&self, to: f64) {
        let Some(pipeline) = self.pipeline.as_ref() else {
            return;
        };
        if !to.is_finite() || !self.state.borrow().ready {
            return;
        }
        let at = gst::ClockTime::from_mseconds((to.max(0.0) * 1000.0) as u64);
        if let Err(fault) =
            pipeline.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE, at)
        {
            log(&format!("the video did not move: {fault}"));
        }
        self.state.borrow_mut().position = to;
    }

    pub fn stop(&self) {
        self.bus.replace(None);
        if let Some(pipeline) = self.pipeline.as_ref() {
            let _ = pipeline.set_state(gst::State::Null);
        }
        self.view.clear();
    }

    /// One line for a test's report.
    pub fn describe(&self) -> String {
        let reading = self.reading();
        let state = self.state.borrow();
        format!(
            "key {} ready {} lost {:?} relayed {} playing {} at {:.2} frames {} sound bytes {} drawn {}",
            self.key,
            state.ready,
            state.lost,
            state.relayed,
            state.playing,
            reading.time,
            self.frames.load(Ordering::Relaxed),
            self.sound.load(Ordering::Relaxed),
            self.view.frames_drawn(),
        )
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        if let Some(pipeline) = self.pipeline.as_ref() {
            let _ = pipeline.set_state(gst::State::Null);
        }
    }
}

/// The pipeline: playbin, its picture into `view` and its sound to `sound`.
fn build(
    address: &str,
    view: &VideoView,
    sound: Sound,
    cookie: Option<String>,
    base: Option<String>,
    frames: Arc<AtomicU64>,
    heard: Arc<AtomicU64>,
) -> Option<gst::Element> {
    let made = || -> Result<gst::Element, String> {
        let playbin = gst::ElementFactory::make("playbin")
            .property("uri", address)
            .build()
            .map_err(|fault| fault.to_string())?;

        // The picture, as packed I420, which is what the view's slot takes
        // from the core.
        let picture = gst::parse::bin_from_description(
            "videoconvert ! appsink name=sink caps=video/x-raw,format=I420 max-buffers=2 drop=true",
            true,
        )
        .map_err(|fault| fault.to_string())?;
        let sink = picture
            .by_name("sink")
            .and_then(|sink| sink.downcast::<gst_app::AppSink>().ok())
            .ok_or("no picture sink")?;
        let slot: Arc<dyn VideoSink> = view.sink();
        sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    if let Some(frame) = packed_i420(&sample) {
                        frames.fetch_add(1, Ordering::Relaxed);
                        slot.on_frame(frame);
                    }
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
        playbin.set_property("video-sink", &picture);

        // The sound, as the call's own: 48 kHz, two channels, 16 bits.
        let audio: gst::Element = match sound {
            Sound::Mixer(mixer) => {
                let bin = gst::parse::bin_from_description(
                    "audioconvert ! audioresample ! appsink name=sink caps=audio/x-raw,format=S16LE,rate=48000,channels=2,layout=interleaved",
                    true,
                )
                .map_err(|fault| fault.to_string())?;
                let sink = bin
                    .by_name("sink")
                    .and_then(|sink| sink.downcast::<gst_app::AppSink>().ok())
                    .ok_or("no sound sink")?;
                sink.set_callbacks(
                    gst_app::AppSinkCallbacks::builder()
                        .new_sample(move |sink| {
                            let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                            if let Some(buffer) = sample.buffer() {
                                if let Ok(map) = buffer.map_readable() {
                                    heard.fetch_add(map.len() as u64, Ordering::Relaxed);
                                    mixer.on_pcm(map.as_slice().to_vec(), 48_000, 2);
                                }
                            }
                            Ok(gst::FlowSuccess::Ok)
                        })
                        .build(),
                );
                bin.upcast()
            }
            Sound::System => gst::ElementFactory::make("autoaudiosink")
                .build()
                .map_err(|fault| fault.to_string())?,
            // Held to the clock still, so that the picture keeps its pace.
            Sound::Nowhere => gst::ElementFactory::make("fakesink")
                .property("sync", true)
                .build()
                .map_err(|fault| fault.to_string())?,
        };
        playbin.set_property("audio-sink", &audio);

        // The account's session, only for the deployment's own relay, which
        // plays a media server's film only to the account that chose it.
        if let (Some(cookie), Some(base)) = (cookie, base) {
            playbin.connect("source-setup", false, move |values| {
                let source = values
                    .get(1)
                    .and_then(|value| value.get::<gst::Element>().ok())?;
                let address = source
                    .find_property("location")
                    .map(|_| {
                        source
                            .property::<Option<String>>("location")
                            .unwrap_or_default()
                    })
                    .unwrap_or_default();
                if address.starts_with(base.trim_end_matches('/'))
                    && source.find_property("cookies").is_some()
                {
                    source.set_property_from_value("cookies", &vec![cookie.clone()].to_value());
                }
                None
            });
        }
        Ok(playbin)
    };
    match made() {
        Ok(playbin) => Some(playbin),
        Err(fault) => {
            log(&format!("the player could not be made: {fault}"));
            None
        }
    }
}

/// A sample of I420 as the core's frames are: each plane packed without the
/// padding GStreamer leaves at the end of a row.
fn packed_i420(sample: &gst::Sample) -> Option<RemoteFrame> {
    let buffer = sample.buffer()?;
    let info = gst_video::VideoInfo::from_caps(sample.caps()?).ok()?;
    let frame = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info).ok()?;
    let (width, height) = (frame.width(), frame.height());
    if width == 0 || height == 0 {
        return None;
    }
    let (chroma_w, chroma_h) = (width.div_ceil(2), height.div_ceil(2));
    let mut packed = Vec::with_capacity((width * height + 2 * chroma_w * chroma_h) as usize);
    for plane in 0..3u32 {
        let (plane_w, plane_h) = if plane == 0 {
            (width, height)
        } else {
            (chroma_w, chroma_h)
        };
        let data = frame.plane_data(plane).ok()?;
        let stride = *frame.plane_stride().get(plane as usize)? as usize;
        for row in 0..plane_h as usize {
            let start = row * stride;
            packed.extend_from_slice(data.get(start..start + plane_w as usize)?);
        }
    }
    Some(RemoteFrame {
        pixel_buffer: 0,
        width,
        height,
        rotation: 0,
        timestamp_us: 0,
        i420: Some(packed),
    })
}
