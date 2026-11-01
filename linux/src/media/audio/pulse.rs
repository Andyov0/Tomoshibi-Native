//! PulseAudio, which PipeWire serves as well through pipewire-pulse: the
//! device lists, watched for changes, and the two kinds of stream.
//!
//! The lists are kept by a thread of their own running a PulseAudio main
//! loop, subscribed to the server's sinks, sources and defaults: a headset
//! plugged in or a default changed in the system's settings reaches the
//! devices panel at once, as `devicechange` does in a browser. Everything of
//! PulseAudio's lives on that thread; what leaves it is a plain copy of the
//! lists. A server that goes away (pipewire-pulse restarted, a session with
//! no sound server at all) empties the lists and is looked for again, more
//! slowly each time up to ten seconds apart.
//!
//! The streams use the simple API, each on its own thread blocking on the
//! device, which is what paces both: the capture's reads return every 10 ms
//! of the microphone's clock, and the playback's writes wait for the
//! speakers'. 48 kHz signed 16-bit throughout, the rate and the format the
//! core sends and receives, so PulseAudio does any conversion the hardware
//! needs.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use libpulse_binding as pulse;
use libpulse_simple_binding::Simple;
use pulse::{
    callbacks::ListResult,
    context::{subscribe::InterestMaskSet, Context, FlagSet, State},
    def::BufferAttr,
    mainloop::standard::{IterateResult, Mainloop},
    proplist::{properties, Proplist},
    sample::{Format, Spec},
    stream::Direction,
};

use super::mixer::{FRAME, RATE};
use crate::app::log;

/// A source or a sink as the lists keep it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// PulseAudio's name for it, which is stable across runs and is what a
    /// stream is opened on.
    pub name: String,
    /// What the system calls it.
    pub description: String,
}

/// The server's devices as last read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lists {
    /// Whether a server answered at all.
    pub reachable: bool,
    /// Microphones: every source that is not a sink's monitor, which is
    /// what a browser lists as audio inputs.
    pub sources: Vec<Endpoint>,
    pub sinks: Vec<Endpoint>,
    pub default_source: Option<String>,
    pub default_sink: Option<String>,
}

/// Starts the thread that keeps `lists` and calls `changed` (on that
/// thread) whenever they change.
pub fn watch(lists: Arc<Mutex<Lists>>, changed: impl Fn() + Send + 'static) {
    let spawned = thread::Builder::new()
        .name("tomoshibi pulse watch".into())
        .spawn(move || {
            let mut wait = Duration::from_millis(500);
            let mut said = false;
            loop {
                let publish = |fresh: Lists| {
                    let mut kept = lists.lock().unwrap_or_else(|p| p.into_inner());
                    if *kept != fresh {
                        *kept = fresh;
                        drop(kept);
                        changed();
                    }
                };
                match session(&publish) {
                    Ok(()) => wait = Duration::from_millis(500),
                    Err(fault) => {
                        if !said {
                            log(&format!(
                                "sound: no PulseAudio server ({fault}); looking again"
                            ));
                            said = true;
                        }
                    }
                }
                publish(Lists::default());
                thread::sleep(wait);
                wait = (wait * 2).min(Duration::from_secs(10));
            }
        });
    if let Err(fault) = spawned {
        log(&format!("sound: the device watch did not start: {fault}"));
    }
}

/// The application's name and icon as PulseAudio and PipeWire show them in
/// a volume mixer.
fn identify(proplist: &mut Proplist) {
    let _ = proplist.set_str(properties::APPLICATION_NAME, "Tomoshibi");
    let _ = proplist.set_str(properties::APPLICATION_ID, crate::APP_ID);
    let _ = proplist.set_str(properties::APPLICATION_ICON_NAME, crate::APP_ID);
}

/// One connection to the server, kept until it fails; the lists are read
/// at the start and again after every change the server reports.
fn session(publish: &dyn Fn(Lists)) -> Result<(), String> {
    let mut mainloop = Mainloop::new().ok_or("no main loop")?;
    let mut proplist = Proplist::new().ok_or("no property list")?;
    identify(&mut proplist);
    let mut context = Context::new_with_proplist(&mainloop, "Tomoshibi devices", &proplist)
        .ok_or("no context")?;
    // Never start a server: one that is not running is not this app's to
    // start, and an autospawned PulseAudio under PipeWire fights it.
    context
        .connect(None, FlagSet::NOAUTOSPAWN, None)
        .map_err(|e| format!("{e}"))?;
    loop {
        if let IterateResult::Quit(_) | IterateResult::Err(_) = mainloop.iterate(true) {
            return Err("the main loop stopped".into());
        }
        match context.get_state() {
            State::Ready => break,
            State::Failed | State::Terminated => return Err(format!("{}", context.errno())),
            _ => {}
        }
    }

    let dirty = Rc::new(Cell::new(true));
    {
        let dirty = dirty.clone();
        context.set_subscribe_callback(Some(Box::new(move |_, _, _| dirty.set(true))));
    }
    context.subscribe(
        InterestMaskSet::SINK | InterestMaskSet::SOURCE | InterestMaskSet::SERVER,
        |_| {},
    );

    // The three answers of one reading, and how many are still out; and a
    // reading finished inside PulseAudio's callback, published by the loop
    // outside any callback.
    let reading: Rc<RefCell<(Lists, u32)>> = Rc::new(RefCell::new((Lists::default(), 0)));
    let finished_lists: Rc<RefCell<Option<Lists>>> = Rc::new(RefCell::new(None));
    loop {
        if dirty.get() && reading.borrow().1 == 0 {
            dirty.set(false);
            *reading.borrow_mut() = (
                Lists {
                    reachable: true,
                    ..Lists::default()
                },
                3,
            );
            let introspect = context.introspect();
            let done = {
                let (reading, finished_lists) = (reading.clone(), finished_lists.clone());
                move || {
                    let mut reading = reading.borrow_mut();
                    reading.1 -= 1;
                    if reading.1 == 0 {
                        let mut lists = reading.0.clone();
                        lists
                            .sources
                            .sort_by(|a, b| a.description.cmp(&b.description));
                        lists
                            .sinks
                            .sort_by(|a, b| a.description.cmp(&b.description));
                        *finished_lists.borrow_mut() = Some(lists);
                    }
                }
            };
            let finished = Rc::new(done);
            {
                let (reading, finished) = (reading.clone(), finished.clone());
                introspect.get_server_info(move |info| {
                    let mut reading = reading.borrow_mut();
                    reading.0.default_sink = info.default_sink_name.as_ref().map(|n| n.to_string());
                    reading.0.default_source =
                        info.default_source_name.as_ref().map(|n| n.to_string());
                    drop(reading);
                    finished();
                });
            }
            {
                let (reading, finished) = (reading.clone(), finished.clone());
                introspect.get_sink_info_list(move |item| match item {
                    ListResult::Item(sink) => {
                        if let Some(name) = sink.name.as_ref() {
                            let description = sink
                                .description
                                .as_ref()
                                .map(|d| d.to_string())
                                .unwrap_or_else(|| name.to_string());
                            reading.borrow_mut().0.sinks.push(Endpoint {
                                name: name.to_string(),
                                description,
                            });
                        }
                    }
                    ListResult::End | ListResult::Error => finished(),
                });
            }
            {
                let (reading, finished) = (reading.clone(), finished.clone());
                introspect.get_source_info_list(move |item| match item {
                    ListResult::Item(source) => {
                        if source.monitor_of_sink.is_some() {
                            return;
                        }
                        if let Some(name) = source.name.as_ref() {
                            let description = source
                                .description
                                .as_ref()
                                .map(|d| d.to_string())
                                .unwrap_or_else(|| name.to_string());
                            reading.borrow_mut().0.sources.push(Endpoint {
                                name: name.to_string(),
                                description,
                            });
                        }
                    }
                    ListResult::End | ListResult::Error => finished(),
                });
            }
        }
        let finished = finished_lists.borrow_mut().take();
        if let Some(lists) = finished {
            publish(lists);
        }
        if let IterateResult::Quit(_) | IterateResult::Err(_) = mainloop.iterate(true) {
            return Err("the main loop stopped".into());
        }
        if !matches!(context.get_state(), State::Ready) {
            return Ok(());
        }
    }
}

/// The microphone: 48 kHz mono, read ten milliseconds at a time.
pub fn record(device: Option<&str>) -> Result<Simple, String> {
    let spec = Spec {
        format: Format::S16le,
        channels: 1,
        rate: RATE,
    };
    let bytes = (FRAME * 2) as u32;
    // A fragment of one frame, so that each read returns as soon as the
    // frame is there rather than when the server's default of two seconds
    // would have filled.
    let attr = BufferAttr {
        maxlength: u32::MAX,
        tlength: u32::MAX,
        prebuf: u32::MAX,
        minreq: u32::MAX,
        fragsize: bytes,
    };
    Simple::new(
        None,
        "Tomoshibi",
        Direction::Record,
        device,
        "Microphone",
        &spec,
        None,
        Some(&attr),
    )
    .map_err(|e| format!("{e} (error {})", e.0))
}

/// How much the playback keeps queued at the server: enough for a busy
/// machine's scheduling not to run it dry, little enough that what is said
/// is heard about as soon as in a browser.
pub const PLAYBACK_LATENCY_MS: u32 = 50;

/// The speakers: 48 kHz stereo, written ten milliseconds at a time.
pub fn playback(device: Option<&str>) -> Result<Simple, String> {
    let spec = Spec {
        format: Format::S16le,
        channels: 2,
        rate: RATE,
    };
    let per_ms = RATE / 1000 * 2 * 2;
    let attr = BufferAttr {
        maxlength: u32::MAX,
        tlength: PLAYBACK_LATENCY_MS * per_ms,
        prebuf: u32::MAX,
        minreq: 10 * per_ms,
        fragsize: u32::MAX,
    };
    Simple::new(
        None,
        "Tomoshibi",
        Direction::Playback,
        device,
        "Call",
        &spec,
        None,
        Some(&attr),
    )
    .map_err(|e| format!("{e} (error {})", e.0))
}
