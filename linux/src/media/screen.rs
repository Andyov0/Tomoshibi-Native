//! Sharing the screen, through the xdg-desktop-portal's ScreenCast and
//! PipeWire.
//!
//! A share (core-map section 5, after windows/Tomoshibi/Media/Sharing.cs):
//!
//! 1. The portal's picker (portal.rs): the person picks a monitor or a
//!    window in the desktop's own dialog, and the app is handed a PipeWire
//!    node and a connection to it.
//! 2. The stream (video.rs): read on a thread of its own, each frame
//!    copied into a slot as rows of blue, green, red (pixels.rs).
//! 3. Published as `VideoKind::Screen { fps, quality }` at the size the
//!    setting fits the stream's to (`pixels::fit`), so the publication's
//!    ladder is the web's for that capture (the core's `screen_options`).
//! 4. The pump (pump.rs): the newest frame into `push_bgra` at the share's
//!    rate, the last one again every 500 ms while the screen is still.
//! 5. Its sound, when asked (sound.rs): every other application's playback
//!    through PipeWire, this app's own left out, as
//!    `SoundKind::ScreenSound`. Where it cannot be captured the share goes
//!    without it, as Windows's does, and says so (`sends_sound`).
//! 6. The end: the person stopping it here (`stop`), or from the system
//!    (the compositor closes the portal's session, GNOME's Stop button) or
//!    the shared screen or window going away (its PipeWire node removed),
//!    which take the tracks down and call `on_ended` once.
//!
//! A change of size, rate or sound while sharing (`change`) publishes the
//! picture again at the new setting and takes the old one down once the
//! pump feeds the new one, as Windows's `Retune` does: nobody watching
//! sees the share leave the stage, and nothing is asked again.
//!
//! Everything here is called on GTK's main thread; the capture, the pump
//! and the sound run on threads of their own and push into the core from
//! there, which the core allows, and tell the main thread what happened
//! through a channel ([`Event`]) that the share reads from GLib's main
//! context. Nothing GTK is touched off the main thread.
//!
//! The media module's `Media` hands its screen part to [`Screens`]:
//! `share_screen` is [`Screens::share`] and `can_share_screen` is
//! [`Screens::can_share`]. Every module that uses PipeWire calls
//! [`init_pipewire`] first, in place of `pipewire::init()` (linkage.rs says
//! why: without it the first PipeWire call crashes the app).

mod linkage;
mod pixels;
mod portal;
mod pump;
mod sound;
mod synthetic;
mod video;

#[cfg(test)]
mod live;

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::Duration,
};

use ashpd::desktop::{screencast::Screencast, Session};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tomoshibi_core::call::{AudioInput, Call, VideoInput, VideoKind};
use tomoshibi_protocol::share::SoundKind;

pub use linkage::init_pipewire;
pub use pixels::{fit, fitted};

use self::{
    pump::{FrameSlot, Pump},
    sound::{SoundCapture, SoundSink},
    synthetic::Generator,
    video::VideoCapture,
};
use super::{Local, MediaError, ScreenShare, ShareChoice};
use crate::app::log;

/// Where a share's picture and sound come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The portal's picker, the stream it hands over, and the computer's
    /// sound through PipeWire.
    Portal,
    /// A generated screen and chord (synthetic.rs), for tests: nothing of a
    /// desktop or a room is ever sent, and no portal is needed.
    Synthetic,
}

impl Source {
    /// `TOMOSHIBI_SCREEN=synthetic` in a debug build; the portal otherwise,
    /// and always in a release build.
    pub fn from_environment() -> Source {
        #[cfg(debug_assertions)]
        if std::env::var("TOMOSHIBI_SCREEN").is_ok_and(|value| value.trim() == "synthetic") {
            return Source::Synthetic;
        }
        Source::Portal
    }
}

/// What the capture's threads tell the share, on the main thread.
pub(crate) enum Event {
    /// The stream's format: its size, which a window's resize changes.
    Format { width: u32, height: u32 },
    /// The picture is gone: the stream or the portal's session ended.
    Gone(String),
    /// The sound's capture failed; the picture goes on without it.
    SoundGone(String),
}

/// The screen part of the app's `Media`: one per app.
pub struct Screens {
    source: Source,
    /// Whether the portal offers screen casting, once it has answered.
    known: Rc<Cell<Option<bool>>>,
}

impl Screens {
    /// Asks the portal, in the background, whether sharing is possible.
    /// Called on the main thread, with GLib's main context running or
    /// about to.
    pub fn new(source: Source) -> Screens {
        let known = Rc::new(Cell::new(None));
        match source {
            Source::Synthetic => known.set(Some(true)),
            Source::Portal => {
                let known = known.clone();
                glib::spawn_future_local(async move {
                    known.set(Some(portal::available().await));
                });
            }
        }
        Screens { source, known }
    }

    /// The share button's question: true until the portal has said no,
    /// since a share asked for in the moment before it answers fails with
    /// its own reason anyway.
    pub fn can_share(&self) -> bool {
        self.known.get().unwrap_or(true)
    }

    /// Asks for a screen or a window and starts sending it into `call`.
    pub fn share(
        &self,
        call: Arc<Call>,
        choice: ShareChoice,
    ) -> Local<Result<Rc<dyn ScreenShare>, MediaError>> {
        let source = self.source;
        Box::pin(async move {
            let sharing: Rc<dyn ScreenShare> = begin(source, call, choice).await?;
            Ok(sharing)
        })
    }

    /// Shares the computer's sound alone (the web's "Share only sound"):
    /// every other application, as the screen's own sound is captured, sent
    /// as `SoundKind::Listening` and, where `lossless`, as its lossless
    /// stream too. There is no picker: the portal names no application, and
    /// the capture leaves only this app out. Fails where there is no
    /// PipeWire to capture from.
    pub fn share_sound(
        &self,
        call: Arc<Call>,
        lossless: bool,
    ) -> Local<Result<Rc<dyn ScreenShare>, MediaError>> {
        let source = self.source;
        Box::pin(async move {
            let listening: Rc<dyn ScreenShare> = listen(source, call, lossless).await?;
            Ok(listening)
        })
    }
}

/// Sound shared on its own, as the room holds it.
struct Listening {
    me: Weak<Listening>,
    running: RefCell<Option<(Hearing, Arc<AudioInput>)>>,
    lossless: bool,
    ended: RefCell<Option<Box<dyn Fn()>>>,
    ended_by_itself: Cell<bool>,
    follower: RefCell<Option<glib::JoinHandle<()>>>,
}

/// Starts sharing the computer's sound alone: the capture first, so that a
/// computer whose sound cannot be captured publishes no silent track.
async fn listen(
    source: Source,
    call: Arc<Call>,
    lossless: bool,
) -> Result<Rc<Listening>, MediaError> {
    let (events, mut heard) = unbounded_channel::<Event>();
    let hearing = Hearing::start(source, events)?;
    let kind = SoundKind::Listening {
        lossless_offered: lossless,
    };
    let input = match call.publish_audio(kind).await {
        Ok(input) => input,
        Err(fault) => {
            join(vec![hearing.stop()]).await;
            return Err(MediaError::Call(fault.to_string()));
        }
    };
    hearing.feed(Some(input.clone()));
    let listening = Rc::new_cyclic(|me| Listening {
        me: me.clone(),
        running: RefCell::new(Some((hearing, input))),
        lossless,
        ended: RefCell::new(None),
        ended_by_itself: Cell::new(false),
        follower: RefCell::new(None),
    });
    let me = listening.me.clone();
    let follower = glib::spawn_future_local(async move {
        while let Some(event) = heard.recv().await {
            if let Event::SoundGone(why) | Event::Gone(why) = event {
                let Some(listening) = me.upgrade() else {
                    return;
                };
                log(&format!("the sound shared on its own stopped: {why}"));
                let running = listening.running.borrow_mut().take();
                if let Some(running) = running {
                    stop_listening(running).await;
                }
                listening.ended_by_itself.set(true);
                let ended = listening.ended.borrow_mut().take();
                if let Some(ended) = ended {
                    ended();
                }
                return;
            }
        }
    });
    listening.follower.replace(Some(follower));
    log(&format!(
        "sharing the computer's sound ({source:?}), lossless offered {lossless}"
    ));
    Ok(listening)
}

async fn stop_listening((hearing, input): (Hearing, Arc<AudioInput>)) {
    let pushed = hearing.pushed();
    join(vec![hearing.stop()]).await;
    quietly("the sound", input.unpublish()).await;
    log(&format!(
        "the sound shared on its own is over: {pushed} pieces sent"
    ));
}

impl ScreenShare for Listening {
    fn stop(&self) -> Local<()> {
        let running = self.running.borrow_mut().take();
        if let Some(follower) = self.follower.borrow_mut().take() {
            follower.abort();
        }
        Box::pin(async move {
            if let Some(running) = running {
                stop_listening(running).await;
            }
        })
    }

    fn on_ended(&self, ended: Box<dyn Fn()>) {
        if self.ended_by_itself.get() {
            ended();
        } else {
            self.ended.replace(Some(ended));
        }
    }

    fn sends_sound(&self) -> bool {
        self.running.borrow().is_some()
    }

    fn sends_lossless(&self) -> bool {
        self.lossless && self.running.borrow().is_some()
    }
}

impl Drop for Listening {
    fn drop(&mut self) {
        if let Some(running) = self.running.borrow_mut().take() {
            glib::spawn_future_local(stop_listening(running));
        }
    }
}

/// Where the picture comes from in one share.
enum Picture {
    Portal {
        capture: VideoCapture,
        portal: Rc<Screencast>,
        session: Rc<Session<Screencast>>,
    },
    Synthetic(Generator),
}

/// Where the sound comes from.
enum Hearing {
    Pipewire(SoundCapture),
    Synthetic {
        sink: SoundSink,
        generator: Generator,
    },
}

impl Hearing {
    fn start(source: Source, events: UnboundedSender<Event>) -> Result<Hearing, MediaError> {
        match source {
            Source::Portal => SoundCapture::start(events).map(Hearing::Pipewire),
            Source::Synthetic => {
                let sink: SoundSink = Arc::new(Mutex::new(None));
                let generator = synthetic::sound(sink.clone());
                Ok(Hearing::Synthetic { sink, generator })
            }
        }
    }

    fn feed(&self, input: Option<Arc<AudioInput>>) {
        match self {
            Hearing::Pipewire(capture) => capture.feed(input),
            Hearing::Synthetic { sink, .. } => {
                *sink.lock().unwrap_or_else(|e| e.into_inner()) = input
            }
        }
    }

    fn pushed(&self) -> u64 {
        match self {
            Hearing::Pipewire(capture) => capture.chunks(),
            Hearing::Synthetic { generator, .. } => {
                generator.made.load(std::sync::atomic::Ordering::Relaxed)
            }
        }
    }

    fn stop(self) -> Option<JoinHandle<()>> {
        self.feed(None);
        match self {
            Hearing::Pipewire(capture) => capture.stop(),
            Hearing::Synthetic { generator, .. } => generator.stop(),
        }
    }
}

/// Everything a running share holds.
struct Running {
    picture: Picture,
    slot: Arc<FrameSlot>,
    video: Arc<VideoInput>,
    pump: Pump,
    sound: Option<(Hearing, Arc<AudioInput>)>,
    choice: ShareChoice,
    /// The stream's size when the share was published, for a change made
    /// before any frame has come.
    content: (u32, u32),
}

/// One share, as the room holds it.
struct Sharing {
    me: Weak<Sharing>,
    call: Arc<Call>,
    source: Source,
    events: UnboundedSender<Event>,
    running: RefCell<Option<Running>>,
    ended: RefCell<Option<Box<dyn Fn()>>>,
    ended_by_itself: Cell<bool>,
    changing: Cell<bool>,
    follower: RefCell<Option<glib::JoinHandle<()>>>,
    closer: RefCell<Option<glib::JoinHandle<()>>>,
}

/// Starts a share: the picture, published; the sound where asked and
/// possible; and the share following its own end.
async fn begin(
    source: Source,
    call: Arc<Call>,
    choice: ShareChoice,
) -> Result<Rc<Sharing>, MediaError> {
    let (events, mut heard) = unbounded_channel::<Event>();
    let slot = Arc::new(FrameSlot::default());
    let (picture, content) = match source {
        Source::Portal => {
            let picked = portal::pick().await?;
            let (portal, session) = (Rc::new(picked.portal), Rc::new(picked.session));
            let capture = match VideoCapture::start(
                picked.remote,
                picked.node,
                choice.fps,
                picked.size,
                slot.clone(),
                events.clone(),
            ) {
                Ok(capture) => capture,
                Err(fault) => {
                    let _ = session.close().await;
                    return Err(fault);
                }
            };
            match first_format(&mut heard).await {
                Ok(content) => (
                    Picture::Portal {
                        capture,
                        portal,
                        session,
                    },
                    content,
                ),
                Err(fault) => {
                    join(vec![capture.stop()]).await;
                    let _ = session.close().await;
                    return Err(fault);
                }
            }
        }
        Source::Synthetic => (
            Picture::Synthetic(synthetic::screen(slot.clone())),
            (synthetic::WIDTH, synthetic::HEIGHT),
        ),
    };

    let size = fit(content, choice.quality, choice.fps);
    let kind = VideoKind::Screen {
        fps: choice.fps,
        quality: choice.quality,
    };
    let video = match call.clone().publish_video(kind, size.0, size.1).await {
        Ok(video) => video,
        Err(fault) => {
            close_picture(picture, &mut Vec::new()).await;
            return Err(MediaError::Call(fault.to_string()));
        }
    };
    let pump = Pump::start(slot.clone(), video.clone(), choice.fps, size);
    let sharing = Rc::new_cyclic(|me| Sharing {
        me: me.clone(),
        call,
        source,
        events,
        running: RefCell::new(Some(Running {
            picture,
            slot,
            video,
            pump,
            sound: None,
            choice,
            content,
        })),
        ended: RefCell::new(None),
        ended_by_itself: Cell::new(false),
        changing: Cell::new(false),
        follower: RefCell::new(None),
        closer: RefCell::new(None),
    });
    sharing.follow(heard);
    if choice.with_sound {
        sharing.sound_on().await;
    }
    log(&format!(
        "sharing the screen ({source:?}) at {:?} and {} fps: {}x{} captured, {}x{} sent, its sound {}",
        choice.quality,
        choice.fps,
        content.0,
        content.1,
        size.0,
        size.1,
        if sharing.sends_sound() {
            "too"
        } else if choice.with_sound {
            "not captured"
        } else {
            "not asked for"
        }
    ));
    Ok(sharing)
}

/// The stream's first format, which says how large the screen or window
/// is; the share is published at that. A stream that ends or says nothing
/// in ten seconds is a share that did not start.
async fn first_format(heard: &mut UnboundedReceiver<Event>) -> Result<(u32, u32), MediaError> {
    let wait = async {
        loop {
            match heard.recv().await {
                Some(Event::Format { width, height }) if width > 0 && height > 0 => {
                    return Ok((width, height))
                }
                Some(Event::Gone(why)) => return Err(MediaError::Failed(why)),
                Some(_) => continue,
                None => return Err(MediaError::Failed("the stream's thread ended".into())),
            }
        }
    };
    glib::future_with_timeout(Duration::from_secs(10), wait)
        .await
        .unwrap_or_else(|_| {
            Err(MediaError::Failed(
                "the stream never said its format".into(),
            ))
        })
}

impl Sharing {
    /// Reads what the threads say, and the portal's session closing, until
    /// the share is over.
    fn follow(&self, mut heard: UnboundedReceiver<Event>) {
        let me = self.me.clone();
        let follower = glib::spawn_future_local(async move {
            while let Some(event) = heard.recv().await {
                let Some(sharing) = me.upgrade() else {
                    return;
                };
                match event {
                    Event::Format { width, height } => {
                        log(&format!("the shared screen is now {width}x{height}"));
                    }
                    Event::SoundGone(why) => {
                        log(&format!("the screen's sound stopped: {why}"));
                        sharing.sound_off().await;
                    }
                    Event::Gone(why) => {
                        sharing.end_by_itself(why).await;
                        return;
                    }
                }
            }
        });
        self.follower.replace(Some(follower));

        // The compositor closing the session is the system's Stop: GNOME's
        // indicator, KDE's, or the portal giving up. It is told to the
        // follower like any other end, so that only one place ends a share.
        let session = match self
            .running
            .borrow()
            .as_ref()
            .map(|running| &running.picture)
        {
            Some(Picture::Portal { session, .. }) => session.clone(),
            _ => return,
        };
        let events = self.events.clone();
        let closer = glib::spawn_future_local(async move {
            use futures_util::StreamExt as _;
            match session.receive_closed().await {
                Ok(closed) => {
                    let mut closed = std::pin::pin!(closed);
                    if closed.next().await.is_some() {
                        let _ = events.send(Event::Gone("the portal closed the session".into()));
                    }
                }
                Err(fault) => log(&format!("the portal's session cannot be followed: {fault}")),
            }
        });
        self.closer.replace(Some(closer));
    }

    /// Takes what is running out of the share, so that nothing that wakes
    /// later finds anything to carry on with; the follower is left running
    /// when it is the one ending the share.
    fn finish(&self, from_follower: bool) -> Option<Running> {
        let running = self.running.borrow_mut().take()?;
        if let Some(closer) = self.closer.borrow_mut().take() {
            closer.abort();
        }
        if !from_follower {
            if let Some(follower) = self.follower.borrow_mut().take() {
                follower.abort();
            }
        }
        Some(running)
    }

    async fn end_by_itself(&self, why: String) {
        let Some(running) = self.finish(true) else {
            return;
        };
        log(&format!("the share ended by itself: {why}"));
        teardown(running).await;
        self.ended_by_itself.set(true);
        let ended = self.ended.borrow_mut().take();
        if let Some(ended) = ended {
            ended();
        }
    }

    /// Starts the share's sound: the capture first, so that a computer
    /// whose sound cannot be captured publishes no silent track; then the
    /// track. False, and the share without its sound, where either fails.
    async fn sound_on(&self) -> bool {
        if self
            .running
            .borrow()
            .as_ref()
            .is_none_or(|running| running.sound.is_some())
        {
            return false;
        }
        let hearing = match Hearing::start(self.source, self.events.clone()) {
            Ok(hearing) => hearing,
            Err(fault) => {
                log(&format!("the screen goes without its sound: {fault}"));
                return false;
            }
        };
        let input = match self
            .call
            .clone()
            .publish_audio(SoundKind::ScreenSound)
            .await
        {
            Ok(input) => input,
            Err(fault) => {
                log(&format!("the screen's sound was not published: {fault}"));
                join(vec![hearing.stop()]).await;
                return false;
            }
        };
        let refused = {
            let mut running = self.running.borrow_mut();
            match running.as_mut() {
                Some(running) if running.sound.is_none() => {
                    hearing.feed(Some(input.clone()));
                    running.sound = Some((hearing, input));
                    None
                }
                // Stopped, or given its sound by another change, while this
                // was being published.
                _ => Some((hearing, input)),
            }
        };
        let Some((hearing, input)) = refused else {
            return true;
        };
        join(vec![hearing.stop()]).await;
        quietly("the sound", input.unpublish()).await;
        false
    }

    async fn sound_off(&self) {
        let sound = self
            .running
            .borrow_mut()
            .as_mut()
            .and_then(|running| running.sound.take());
        if let Some((hearing, input)) = sound {
            join(vec![hearing.stop()]).await;
            quietly("the sound", input.unpublish()).await;
        }
    }

    async fn change_to(&self, choice: ShareChoice) -> Result<(), MediaError> {
        let gone = || MediaError::Failed("the share stopped".into());
        let (current, content) = {
            let running = self.running.borrow();
            let running = running.as_ref().ok_or_else(gone)?;
            (
                running.choice,
                running.slot.size().unwrap_or(running.content),
            )
        };
        if (current.fps, current.quality) != (choice.fps, choice.quality) {
            let size = fit(content, choice.quality, choice.fps);
            let kind = VideoKind::Screen {
                fps: choice.fps,
                quality: choice.quality,
            };
            let fresh = self
                .call
                .clone()
                .publish_video(kind, size.0, size.1)
                .await
                .map_err(|fault| MediaError::Call(fault.to_string()))?;
            let old = {
                let mut running = self.running.borrow_mut();
                running.as_mut().map(|running| {
                    running.pump.feed(fresh.clone(), choice.fps, size);
                    if let Picture::Portal { capture, .. } = &running.picture {
                        capture.set_rate(choice.fps);
                    }
                    running.choice.fps = choice.fps;
                    running.choice.quality = choice.quality;
                    std::mem::replace(&mut running.video, fresh.clone())
                })
            };
            let Some(old) = old else {
                // Stopped while the new picture was being published: it is
                // nobody's.
                quietly("the new picture", fresh.unpublish()).await;
                return Err(gone());
            };
            quietly("the old picture", old.unpublish()).await;
            log(&format!(
                "the share is now at {:?} and {} fps, {}x{} sent",
                choice.quality, choice.fps, size.0, size.1
            ));
        }
        if choice.with_sound != current.with_sound {
            if choice.with_sound {
                self.sound_on().await;
            } else {
                self.sound_off().await;
            }
            if let Some(running) = self.running.borrow_mut().as_mut() {
                running.choice.with_sound = choice.with_sound;
            }
        }
        Ok(())
    }
}

impl ScreenShare for Sharing {
    fn stop(&self) -> Local<()> {
        let running = self.finish(false);
        Box::pin(async move {
            if let Some(running) = running {
                log("stopping the share");
                teardown(running).await;
            }
        })
    }

    fn on_ended(&self, ended: Box<dyn Fn()>) {
        if self.ended_by_itself.get() {
            ended();
        } else {
            self.ended.replace(Some(ended));
        }
    }

    fn sends_sound(&self) -> bool {
        self.running
            .borrow()
            .as_ref()
            .is_some_and(|running| running.sound.is_some())
    }

    fn change(&self, choice: ShareChoice) -> Local<Result<(), MediaError>> {
        let me = self.me.upgrade();
        Box::pin(async move {
            let Some(me) = me else {
                return Err(MediaError::Failed("the share stopped".into()));
            };
            // One at a time (Windows's `_changes`): a second change while the
            // first is on the network would publish a third picture.
            if me.changing.replace(true) {
                return Err(MediaError::Busy);
            }
            let changed = me.change_to(choice).await;
            me.changing.set(false);
            changed
        })
    }
}

impl Drop for Sharing {
    /// A share let go of without being stopped (a room thrown away in the
    /// middle of a call) still takes its tracks down and closes the
    /// portal's session.
    fn drop(&mut self) {
        if let Some(running) = self.finish(false) {
            glib::spawn_future_local(teardown(running));
        }
    }
}

/// Takes a share apart: nothing more pushed, the capture stopped, the
/// tracks taken down, the portal's session closed, and the threads waited
/// for off the main thread.
async fn teardown(running: Running) {
    let Running {
        picture,
        video,
        pump,
        sound,
        ..
    } = running;
    let pushed = pump.pushed();
    let mut threads = vec![pump.stop()];
    let mut heard = None;
    if let Some((hearing, input)) = sound {
        heard = Some(hearing.pushed());
        threads.push(hearing.stop());
        quietly("the sound", input.unpublish()).await;
    }
    quietly("the picture", video.unpublish()).await;
    close_picture(picture, &mut threads).await;
    join(threads).await;
    log(&format!(
        "the share is over: {pushed} pictures sent{}",
        heard
            .map(|chunks| format!(", {chunks} pieces of sound"))
            .unwrap_or_default()
    ));
}

async fn close_picture(picture: Picture, threads: &mut Vec<Option<JoinHandle<()>>>) {
    match picture {
        Picture::Portal {
            capture,
            portal,
            session,
        } => {
            threads.push(capture.stop());
            if let Err(fault) = session.close().await {
                log(&format!("the portal's session did not close: {fault}"));
            }
            drop(portal);
        }
        Picture::Synthetic(generator) => threads.push(generator.stop()),
    }
}

/// Waits for threads to end, on GIO's pool rather than the main thread.
async fn join(threads: Vec<Option<JoinHandle<()>>>) {
    let threads: Vec<JoinHandle<()>> = threads.into_iter().flatten().collect();
    if threads.is_empty() {
        return;
    }
    let _ = gio::spawn_blocking(move || {
        for thread in threads {
            let _ = thread.join();
        }
    })
    .await;
}

/// A track taken down, its failure only logged: a share is stopped however
/// the network answers.
async fn quietly<F>(what: &str, unpublished: F)
where
    F: std::future::Future<Output = Result<(), tomoshibi_core::call::CallError>>,
{
    if let Err(fault) = unpublished.await {
        log(&format!("{what} was not taken down: {fault}"));
    }
}
