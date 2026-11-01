//! Cameras through PipeWire: on the connection the camera portal opens
//! (`org.freedesktop.portal.Camera.OpenPipeWireRemote`), which shows this
//! app the camera nodes alone, or, where no portal answers, on an ordinary
//! connection to the session's PipeWire. Cameras that only libcamera can
//! drive (the image signal processors of recent laptops) exist nowhere
//! else; a V4L2 node of theirs gives raw sensor data nobody can show.
//!
//! One thread holds the connection and runs PipeWire's loop: the registry
//! listener keeps the list of camera nodes (a node is a camera when it is a
//! video source whose role is Camera, or that a camera monitor made from a
//! V4L2 or libcamera device, which leaves out the screen casts that are
//! video sources too), and streams are opened and closed on it by command
//! (`pipewire::channel`). Each stream asks for NV12, YUY2 or I420 at
//! 1280x720 and thirty frames a second, or MJPEG at the same, and its frames
//! go into the camera's feed on this thread, read before the buffer is given
//! back.

use std::{
    cell::RefCell,
    collections::HashMap,
    os::fd::OwnedFd,
    rc::Rc,
    sync::{Arc, Mutex},
    thread,
};

use pipewire as pw;
use pw::{properties::properties, spa};
use spa::{
    param::{
        format::{FormatProperties, MediaSubtype, MediaType},
        format_utils::parse_format,
        video::{VideoFormat, VideoInfoRaw},
        ParamType,
    },
    pod::{serialize::PodSerializer, Pod},
    utils::{Direction, Fraction, Rectangle, SpaTypes},
};
use tokio::sync::oneshot;

use super::{
    feed::Feed,
    frame::{PixelFormat, Raw},
    modes::Wanted,
};
use crate::{app::log, media::MediaError};

/// A camera node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// `node.name`, which a camera keeps across runs.
    pub name: String,
    /// `object.serial`, which a stream is pointed at.
    pub serial: String,
    pub label: String,
}

enum Command {
    Open {
        token: u64,
        node: Node,
        wanted: Wanted,
        feed: Arc<Feed>,
        opened: oneshot::Sender<Result<(), MediaError>>,
        ended: Box<dyn FnOnce() + Send>,
    },
    Close {
        token: u64,
    },
}

/// The connection, from the main thread's side.
pub struct Session {
    commands: pw::channel::Sender<Command>,
    pub nodes: Arc<Mutex<Vec<Node>>>,
}

impl Session {
    /// Opens a stream from `node` into `feed`, under `token`.
    pub fn open(
        &self,
        token: u64,
        node: Node,
        wanted: Wanted,
        feed: Arc<Feed>,
        opened: oneshot::Sender<Result<(), MediaError>>,
        ended: Box<dyn FnOnce() + Send>,
    ) -> bool {
        self.commands
            .send(Command::Open {
                token,
                node,
                wanted,
                feed,
                opened,
                ended,
            })
            .is_ok()
    }

    pub fn close(&self, token: u64) {
        let _ = self.commands.send(Command::Close { token });
    }
}

/// Connects (through the portal's `fd`, or directly when there is none) on
/// a thread of its own; `connected` is told whether it worked, and
/// `changed` is called on that thread whenever the cameras change.
pub fn start(
    fd: Option<OwnedFd>,
    changed: impl Fn() + Send + 'static,
    connected: oneshot::Sender<Result<Session, String>>,
) {
    let spawned = thread::Builder::new()
        .name("tomoshibi pipewire camera".into())
        .spawn(move || {
            if let Err(fault) = run(fd, changed, connected) {
                log(&format!("camera: PipeWire: {fault}"));
            }
        });
    if let Err(fault) = spawned {
        log(&format!(
            "camera: the PipeWire thread did not start: {fault}"
        ));
    }
}

/// Whether a node is a camera.
fn is_camera(props: &spa::utils::dict::DictRef, through_portal: bool) -> bool {
    if props.get("media.class") != Some("Video/Source") {
        return false;
    }
    through_portal
        || props.get("media.role") == Some("Camera")
        || matches!(props.get("device.api"), Some("v4l2") | Some("libcamera"))
}

fn run(
    fd: Option<OwnedFd>,
    changed: impl Fn() + Send + 'static,
    connected: oneshot::Sender<Result<Session, String>>,
) -> Result<(), String> {
    pw::init();
    let through_portal = fd.is_some();
    let made = (|| {
        let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(|e| e.to_string())?;
        let context = pw::context::ContextRc::new(&mainloop, None).map_err(|e| e.to_string())?;
        let core = match fd {
            Some(fd) => context.connect_fd_rc(fd, None),
            None => context.connect_rc(None),
        }
        .map_err(|e| format!("no connection: {e}"))?;
        let registry = core.get_registry_rc().map_err(|e| e.to_string())?;
        Ok::<_, String>((mainloop, context, core, registry))
    })();
    let (mainloop, _context, core, registry) = match made {
        Ok(made) => made,
        Err(fault) => {
            let _ = connected.send(Err(fault.clone()));
            return Err(fault);
        }
    };

    let nodes: Arc<Mutex<Vec<Node>>> = Arc::new(Mutex::new(Vec::new()));
    let (commands, receiver) = pw::channel::channel::<Command>();
    let _ = connected.send(Ok(Session {
        commands,
        nodes: nodes.clone(),
    }));

    // Every camera by its global id, and the list published from it.
    let known: Rc<RefCell<HashMap<u32, Node>>> = Rc::new(RefCell::new(HashMap::new()));
    let changed = Rc::new(changed);
    let publish = {
        let (known, nodes, changed) = (known.clone(), nodes.clone(), changed.clone());
        Rc::new(move || {
            let mut list: Vec<Node> = known.borrow().values().cloned().collect();
            list.sort_by(|a, b| a.label.cmp(&b.label));
            *nodes.lock().unwrap_or_else(|p| p.into_inner()) = list;
            changed();
        })
    };
    let _registry_listener = registry
        .add_listener_local()
        .global({
            let (known, publish) = (known.clone(), publish.clone());
            move |global| {
                if global.type_ != pw::types::ObjectType::Node {
                    return;
                }
                let Some(props) = global.props.as_ref() else {
                    return;
                };
                if !is_camera(props, through_portal) {
                    return;
                }
                let name = props.get("node.name").unwrap_or_default().to_owned();
                let label = props
                    .get("node.description")
                    .or_else(|| props.get("node.nick"))
                    .unwrap_or(&name)
                    .to_owned();
                let serial = props.get("object.serial").unwrap_or_default().to_owned();
                known.borrow_mut().insert(
                    global.id,
                    Node {
                        name,
                        serial,
                        label,
                    },
                );
                publish();
            }
        })
        .global_remove({
            let (known, publish) = (known.clone(), publish.clone());
            move |id| {
                if known.borrow_mut().remove(&id).is_some() {
                    publish();
                }
            }
        })
        .register();

    // The open streams by token, with their listeners, which must live as
    // long as they do.
    type Open = (pw::stream::StreamRc, pw::stream::StreamListener<Negotiated>);
    let streams: Rc<RefCell<HashMap<u64, Open>>> = Rc::new(RefCell::new(HashMap::new()));
    let _receiver = receiver.attach(mainloop.loop_(), {
        let (core, streams) = (core.clone(), streams.clone());
        move |command| match command {
            Command::Open {
                token,
                node,
                wanted,
                feed,
                opened,
                ended,
            } => match open(&core, &node, wanted, feed, ended) {
                Ok(open) => {
                    log(&format!(
                        "camera: {} through PipeWire{}",
                        node.label,
                        if through_portal {
                            " (the portal's)"
                        } else {
                            ""
                        }
                    ));
                    streams.borrow_mut().insert(token, open);
                    let _ = opened.send(Ok(()));
                }
                Err(fault) => {
                    let _ = opened.send(Err(MediaError::Failed(fault)));
                }
            },
            Command::Close { token } => {
                if let Some((stream, listener)) = streams.borrow_mut().remove(&token) {
                    let _ = stream.disconnect();
                    drop(listener);
                }
            }
        }
    });

    mainloop.run();
    Ok(())
}

/// What a stream has agreed to send.
struct Negotiated {
    format: Option<PixelFormat>,
    width: u32,
    height: u32,
    feed: Arc<Feed>,
    ended: Option<Box<dyn FnOnce() + Send>>,
    streaming: bool,
}

/// The format a stream offers to take: raw in the layouts read here, or
/// MJPEG, each at the size and rate wanted by default and anything else
/// the camera has otherwise.
fn format_param(subtype: MediaSubtype, wanted: Wanted) -> Result<Vec<u8>, String> {
    let size = spa::pod::property!(
        FormatProperties::VideoSize,
        Choice,
        Range,
        Rectangle,
        Rectangle {
            width: wanted.width,
            height: wanted.height
        },
        Rectangle {
            width: 1,
            height: 1
        },
        Rectangle {
            width: 8192,
            height: 8192
        }
    );
    let rate = spa::pod::property!(
        FormatProperties::VideoFramerate,
        Choice,
        Range,
        Fraction,
        Fraction {
            num: wanted.fps,
            denom: 1
        },
        Fraction { num: 0, denom: 1 },
        Fraction {
            num: 1000,
            denom: 1
        }
    );
    let object = if subtype == MediaSubtype::Raw {
        spa::pod::object!(
            SpaTypes::ObjectParamFormat,
            ParamType::EnumFormat,
            spa::pod::property!(FormatProperties::MediaType, Id, MediaType::Video),
            spa::pod::property!(FormatProperties::MediaSubtype, Id, MediaSubtype::Raw),
            spa::pod::property!(
                FormatProperties::VideoFormat,
                Choice,
                Enum,
                Id,
                VideoFormat::NV12,
                VideoFormat::NV12,
                VideoFormat::YUY2,
                VideoFormat::I420
            ),
            size,
            rate,
        )
    } else {
        spa::pod::object!(
            SpaTypes::ObjectParamFormat,
            ParamType::EnumFormat,
            spa::pod::property!(FormatProperties::MediaType, Id, MediaType::Video),
            spa::pod::property!(FormatProperties::MediaSubtype, Id, MediaSubtype::Mjpg),
            size,
            rate,
        )
    };
    PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(object),
    )
    .map(|(cursor, _)| cursor.into_inner())
    .map_err(|e| format!("{e:?}"))
}

fn open(
    core: &pw::core::CoreRc,
    node: &Node,
    wanted: Wanted,
    feed: Arc<Feed>,
    ended: Box<dyn FnOnce() + Send>,
) -> Result<(pw::stream::StreamRc, pw::stream::StreamListener<Negotiated>), String> {
    let stream = pw::stream::StreamRc::new(
        core.clone(),
        "Tomoshibi camera",
        properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Camera",
            // PW_KEY_TARGET_OBJECT, which the bindings name only behind a
            // version feature; every PipeWire since 0.3.44 reads it.
            "target.object" => node.serial.as_str(),
        },
    )
    .map_err(|e| e.to_string())?;
    let negotiated = Negotiated {
        format: None,
        width: 0,
        height: 0,
        feed,
        ended: Some(ended),
        streaming: false,
    };
    let listener = stream
        .add_local_listener_with_user_data(negotiated)
        .state_changed(|_, negotiated, _, new| match new {
            pw::stream::StreamState::Streaming => negotiated.streaming = true,
            pw::stream::StreamState::Error(fault) => {
                log(&format!("camera: the PipeWire stream failed: {fault}"));
                if let Some(ended) = negotiated.ended.take() {
                    ended();
                }
            }
            pw::stream::StreamState::Unconnected if negotiated.streaming => {
                if let Some(ended) = negotiated.ended.take() {
                    ended();
                }
            }
            _ => {}
        })
        .param_changed(|_, negotiated, id, param| {
            let Some(param) = param else { return };
            if id != ParamType::Format.as_raw() {
                return;
            }
            let Ok((media_type, subtype)) = parse_format(param) else {
                return;
            };
            if media_type != MediaType::Video {
                return;
            }
            if subtype == MediaSubtype::Mjpg {
                // The size is read from each JPEG.
                negotiated.format = Some(PixelFormat::Mjpeg);
                return;
            }
            let mut info = VideoInfoRaw::new();
            if info.parse(param).is_err() {
                return;
            }
            negotiated.format = match info.format() {
                VideoFormat::NV12 => Some(PixelFormat::Nv12),
                VideoFormat::YUY2 => Some(PixelFormat::Yuyv),
                VideoFormat::I420 => Some(PixelFormat::I420),
                _ => None,
            };
            negotiated.width = info.size().width;
            negotiated.height = info.size().height;
            log(&format!(
                "camera: PipeWire sends {:?} at {}x{}",
                negotiated.format, negotiated.width, negotiated.height
            ));
        })
        .process(|stream, negotiated| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let Some(format) = negotiated.format else {
                return;
            };
            let datas = buffer.datas_mut();
            let mut planes: Vec<(&[u8], u32)> = Vec::with_capacity(datas.len());
            for data in datas.iter_mut() {
                let (offset, size, stride) = {
                    let chunk = data.chunk();
                    (
                        chunk.offset() as usize,
                        chunk.size() as usize,
                        chunk.stride().max(0) as u32,
                    )
                };
                let Some(bytes) = data.data() else { continue };
                // Read only, for as long as the buffer is held.
                let bytes: &[u8] = bytes;
                let end = (offset + size).min(bytes.len());
                if offset >= end {
                    continue;
                }
                planes.push((&bytes[offset..end], stride));
            }
            if planes.is_empty() {
                return;
            }
            let raw = Raw {
                format,
                width: negotiated.width,
                height: negotiated.height,
                planes,
            };
            negotiated.feed.deliver(&raw, negotiated.feed.now_us());
        })
        .register()
        .map_err(|e| e.to_string())?;
    let raw = format_param(MediaSubtype::Raw, wanted)?;
    let mjpg = format_param(MediaSubtype::Mjpg, wanted)?;
    let mut params = [
        Pod::from_bytes(&raw).ok_or("a bad format")?,
        Pod::from_bytes(&mjpg).ok_or("a bad format")?,
    ];
    stream
        .connect(
            Direction::Input,
            None,
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut params,
        )
        .map_err(|e| e.to_string())?;
    Ok((stream, listener))
}
