//! The picture of a shared screen: the PipeWire stream the portal opened,
//! read on a thread of its own into the share's [`FrameSlot`].
//!
//! The stream is connected through the file descriptor the portal hands
//! over (`OpenPipeWireRemote`), which is a PipeWire connection that can see
//! only the nodes of this session; the node is the one the portal named.
//!
//! Shared memory only (MemFd and MemPtr), not DMA-BUF. A DMA-BUF the
//! compositor fills is GPU memory: one with a tiled modifier can only be
//! read by importing it into EGL and drawing it out, and a linear one,
//! though it can be mapped, is read through an uncached mapping that on a
//! discrete GPU is slower than the compositor's own read-back into shared
//! memory, which is what every portal backend (mutter, KWin,
//! xdg-desktop-portal-wlr) does when the client offers nothing else. The
//! place DMA-BUF would pay is handing the buffer to a hardware encoder
//! without a copy (libwebrtc's `NativeBuffer::from_dmabuf`), which the core
//! does not wrap.
//!
//! The format asked for is the WebRTC screencast's (its
//! `screencast_stream_utils.cc`): raw video in any four-byte RGB order,
//! any size, a variable frame rate (0/1) no faster than the share's
//! (`maxFramerate`), so that a compositor that can pace itself does not
//! render frames the pump would drop.

use std::{
    cell::Cell,
    io::Cursor,
    os::fd::OwnedFd,
    rc::Rc,
    sync::{mpsc, Arc},
    thread::JoinHandle,
    time::Duration,
};

use pipewire as pw;
use pw::{
    properties::properties,
    spa::{
        self,
        buffer::meta::{MetaHeader, MetaHeaderFlags, MetaVideoCrop},
        param::{
            format::{FormatProperties, MediaSubtype, MediaType},
            video::{VideoFormat, VideoInfoRaw},
            ParamType,
        },
        pod::{serialize::PodSerializer, ChoiceValue, Object, Pod, Property, PropertyFlags, Value},
        utils::{Choice, ChoiceEnum, ChoiceFlags, Fraction, Id, Rectangle, SpaTypes},
    },
    stream::{StreamFlags, StreamRc, StreamState},
};
use tokio::sync::mpsc::UnboundedSender;

use super::{
    pixels::{to_bgra, Layout, Rect},
    pump::{Frame, FrameSlot},
    Event,
};
use crate::{app::log, media::MediaError};

/// What the main thread can ask of the capture's loop.
enum Ask {
    /// A new top rate: the format is offered again with it as the ceiling.
    Rate(u32),
    Quit,
}

/// The capture of one share's stream, running until stopped or the stream
/// goes away.
pub struct VideoCapture {
    asks: pw::channel::Sender<Ask>,
    thread: Option<JoinHandle<()>>,
}

impl VideoCapture {
    /// Connects to `node` over the portal's `remote` and starts copying
    /// its frames into `slot`; `events` hears the format and the end. Fails
    /// where PipeWire will not take the connection or the stream.
    pub fn start(
        remote: OwnedFd,
        node: u32,
        fps: u32,
        size_hint: Option<(u32, u32)>,
        slot: Arc<FrameSlot>,
        events: UnboundedSender<Event>,
    ) -> Result<VideoCapture, MediaError> {
        let (asks, asked) = pw::channel::channel::<Ask>();
        let (started, starting) = mpsc::channel::<Result<(), String>>();
        let thread = std::thread::Builder::new()
            .name("tomoshibi-screen-capture".into())
            .spawn(move || {
                if let Err(fault) = run(remote, node, fps, size_hint, slot, events, asked, &started)
                {
                    let _ = started.send(Err(fault));
                }
            })
            .map_err(|fault| MediaError::Failed(format!("no capture thread: {fault}")))?;
        match starting.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(())) => Ok(VideoCapture {
                asks,
                thread: Some(thread),
            }),
            Ok(Err(fault)) => Err(MediaError::Failed(format!("the screen's stream: {fault}"))),
            Err(_) => {
                let _ = asks.send(Ask::Quit);
                Err(MediaError::Failed(
                    "the screen's stream did not start".into(),
                ))
            }
        }
    }

    /// Offers the format again with this rate as its ceiling, for a share
    /// changed to another rate.
    pub fn set_rate(&self, fps: u32) {
        let _ = self.asks.send(Ask::Rate(fps));
    }

    /// Disconnects the stream and ends the loop; returns the thread to
    /// wait for.
    pub fn stop(mut self) -> Option<JoinHandle<()>> {
        let _ = self.asks.send(Ask::Quit);
        self.thread.take()
    }
}

impl Drop for VideoCapture {
    fn drop(&mut self) {
        let _ = self.asks.send(Ask::Quit);
    }
}

/// What the stream's callbacks keep between them.
struct Negotiated {
    layout: Option<Layout>,
    width: u32,
    height: u32,
    /// The stream has run; an unconnected one after that has gone away.
    streamed: bool,
    slot: Arc<FrameSlot>,
    events: UnboundedSender<Event>,
    /// Frames not taken, by reason, for the log.
    skipped: u64,
}

/// The four-byte layout a negotiated format is, or nothing for one this
/// does not take (the offer names only these, so nothing else should come).
fn layout_of(format: VideoFormat) -> Option<Layout> {
    match format {
        VideoFormat::BGRx | VideoFormat::BGRA => Some(Layout::Bgrx),
        VideoFormat::RGBx | VideoFormat::RGBA => Some(Layout::Rgbx),
        VideoFormat::xRGB | VideoFormat::ARGB => Some(Layout::Xrgb),
        VideoFormat::xBGR | VideoFormat::ABGR => Some(Layout::Xbgr),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn run(
    remote: OwnedFd,
    node: u32,
    fps: u32,
    size_hint: Option<(u32, u32)>,
    slot: Arc<FrameSlot>,
    events: UnboundedSender<Event>,
    asked: pw::channel::Receiver<Ask>,
    started: &mpsc::Sender<Result<(), String>>,
) -> Result<(), String> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(|e| format!("main loop: {e}"))?;
    let context =
        pw::context::ContextRc::new(&mainloop, None).map_err(|e| format!("context: {e}"))?;
    let core = context
        .connect_fd_rc(remote, None)
        .map_err(|e| format!("the portal's remote: {e}"))?;

    // A core error (the remote closed under us) ends the share like the
    // stream's own.
    let _core_listener = core
        .add_listener_local()
        .error({
            let events = events.clone();
            move |id, _seq, res, message| {
                log(&format!(
                    "the screen's PipeWire remote: error {res} on {id}: {message}"
                ));
                if id == pw::core::PW_ID_CORE {
                    let _ = events.send(Event::Gone(format!("PipeWire: {message}")));
                }
            }
        })
        .register();

    let stream = StreamRc::new(
        core.clone(),
        "tomoshibi-screen",
        properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )
    .map_err(|e| format!("stream: {e}"))?;

    let state = Negotiated {
        layout: None,
        width: 0,
        height: 0,
        streamed: false,
        slot,
        events: events.clone(),
        skipped: 0,
    };
    let _listener = stream
        .add_local_listener_with_user_data(state)
        .state_changed(|_, state, old, new| {
            log(&format!("the screen's stream: {old:?} -> {new:?}"));
            match new {
                StreamState::Streaming => state.streamed = true,
                StreamState::Error(message) => {
                    let _ = state
                        .events
                        .send(Event::Gone(format!("the stream failed: {message}")));
                }
                StreamState::Unconnected if state.streamed => {
                    let _ = state
                        .events
                        .send(Event::Gone("the stream went away".into()));
                }
                _ => {}
            }
        })
        .param_changed(|stream, state, id, param| {
            if id != ParamType::Format.as_raw() {
                return;
            }
            let Some(param) = param else {
                return;
            };
            if !matches!(
                spa::param::format_utils::parse_format(param),
                Ok((MediaType::Video, MediaSubtype::Raw))
            ) {
                return;
            }
            let mut info = VideoInfoRaw::new();
            if info.parse(param).is_err() {
                return;
            }
            state.layout = layout_of(info.format());
            state.width = info.size().width;
            state.height = info.size().height;
            log(&format!(
                "the screen's stream: {:?} {}x{}, at most {}/{} a second",
                info.format(),
                state.width,
                state.height,
                info.max_framerate().num,
                info.max_framerate().denom
            ));
            let Some(_) = state.layout else {
                let _ = state.events.send(Event::Gone(format!(
                    "the stream came as {:?}, which is not taken",
                    info.format()
                )));
                return;
            };
            let _ = state.events.send(Event::Format {
                width: state.width,
                height: state.height,
            });
            let params = [
                buffers(),
                meta(
                    spa::sys::SPA_META_Header,
                    std::mem::size_of::<spa::sys::spa_meta_header>(),
                ),
                meta(
                    spa::sys::SPA_META_VideoCrop,
                    std::mem::size_of::<spa::sys::spa_meta_region>(),
                ),
            ];
            let mut pods: Vec<&Pod> = params
                .iter()
                .filter_map(|bytes| Pod::from_bytes(bytes))
                .collect();
            if let Err(fault) = stream.update_params(&mut pods) {
                log(&format!(
                    "the screen's stream refused its buffer parameters: {fault}"
                ));
            }
        })
        .process(take)
        .register()
        .map_err(|e| format!("listener: {e}"))?;

    let offer = enum_format(fps, size_hint);
    let mut params = [Pod::from_bytes(&offer).ok_or("the offered format")?];
    stream
        .connect(
            spa::utils::Direction::Input,
            Some(node),
            StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS,
            &mut params,
        )
        .map_err(|e| format!("connect to node {node}: {e}"))?;
    let _ = started.send(Ok(()));

    let last_rate = Rc::new(Cell::new(fps));
    let _asked = asked.attach(mainloop.loop_(), {
        let (mainloop, stream, last_rate) = (mainloop.clone(), stream.clone(), last_rate.clone());
        move |ask| match ask {
            Ask::Quit => mainloop.quit(),
            Ask::Rate(fps) if fps != last_rate.get() => {
                last_rate.set(fps);
                let offer = enum_format(fps, None);
                if let Some(pod) = Pod::from_bytes(&offer) {
                    if let Err(fault) = stream.update_params(&mut [pod]) {
                        log(&format!("the screen's stream kept its rate: {fault}"));
                    }
                }
            }
            Ask::Rate(_) => {}
        }
    });
    mainloop.run();
    let _ = stream.disconnect();
    Ok(())
}

/// Takes the newest buffer the stream has, copies the picture into the
/// slot, and gives every buffer back.
fn take(stream: &pw::stream::Stream, state: &mut Negotiated) {
    // Only the newest is worth anything; the ones before it go straight
    // back (each is queued again as it is dropped).
    let mut newest = None;
    while let Some(buffer) = stream.dequeue_buffer() {
        newest = Some(buffer);
    }
    let Some(mut buffer) = newest else {
        return;
    };
    let Some(layout) = state.layout else {
        return;
    };
    if buffer
        .find_meta::<MetaHeader>()
        .is_some_and(|header| header.flags().contains(MetaHeaderFlags::CORRUPTED))
    {
        state.skip("marked corrupted");
        return;
    }
    let (width, height) = (state.width, state.height);
    let part = buffer
        .find_meta::<MetaVideoCrop>()
        .map(|crop| crop.meta_region())
        .filter(|region| region.is_valid())
        .and_then(|region| {
            Rect {
                x: u32::try_from(region.position().x).ok()?,
                y: u32::try_from(region.position().y).ok()?,
                width: region.size().width,
                height: region.size().height,
            }
            .within(width, height)
        })
        .unwrap_or(Rect::whole(width, height));
    let datas = buffer.datas_mut();
    let Some(data) = datas.first_mut() else {
        return;
    };
    let (offset, size, stride) = {
        let chunk = data.chunk();
        (
            chunk.offset() as usize,
            chunk.size() as usize,
            chunk.stride(),
        )
    };
    // A buffer with nothing in it: mutter sends one when only the pointer
    // moved and the pointer is not in the picture.
    if size == 0 {
        return;
    }
    let stride = if stride > 0 {
        stride as usize
    } else {
        width as usize * 4
    };
    let Some(bytes) = data.data() else {
        state.skip("not mapped");
        return;
    };
    let Some(picture) = bytes.get(offset..offset.saturating_add(size)) else {
        state.skip("a chunk past the buffer's end");
        return;
    };
    let mut bgra = Vec::new();
    if !to_bgra(layout, picture, stride, part, &mut bgra) {
        state.skip("shorter than its format");
        return;
    }
    state.slot.put(Frame {
        bgra,
        width: part.width,
        height: part.height,
    });
}

impl Negotiated {
    fn skip(&mut self, why: &str) {
        self.skipped += 1;
        if self.skipped == 1 || self.skipped.is_multiple_of(100) {
            log(&format!(
                "a screen frame was not taken ({why}), {} so far",
                self.skipped
            ));
        }
    }
}

fn serialized(object: Object) -> Vec<u8> {
    PodSerializer::serialize(Cursor::new(Vec::new()), &Value::Object(object))
        .map(|(bytes, _)| bytes.into_inner())
        .unwrap_or_default()
}

/// The formats offered: any four-byte RGB order, any size (the portal's own
/// for the stream first, where it said one), a variable rate no faster than
/// `fps`.
fn enum_format(fps: u32, size_hint: Option<(u32, u32)>) -> Vec<u8> {
    let fps = fps.max(1);
    let (width, height) = size_hint.unwrap_or((1920, 1080));
    let id = |format: VideoFormat| Id(format.as_raw());
    let formats = [
        VideoFormat::BGRx,
        VideoFormat::BGRA,
        VideoFormat::RGBx,
        VideoFormat::RGBA,
        VideoFormat::xRGB,
        VideoFormat::ARGB,
        VideoFormat::xBGR,
        VideoFormat::ABGR,
    ];
    let property = |key: FormatProperties, value: Value| Property {
        key: key.as_raw(),
        flags: PropertyFlags::empty(),
        value,
    };
    serialized(Object {
        type_: SpaTypes::ObjectParamFormat.as_raw(),
        id: ParamType::EnumFormat.as_raw(),
        properties: vec![
            property(
                FormatProperties::MediaType,
                Value::Id(Id(MediaType::Video.as_raw())),
            ),
            property(
                FormatProperties::MediaSubtype,
                Value::Id(Id(MediaSubtype::Raw.as_raw())),
            ),
            property(
                FormatProperties::VideoFormat,
                Value::Choice(ChoiceValue::Id(Choice(
                    ChoiceFlags::empty(),
                    ChoiceEnum::Enum {
                        default: id(VideoFormat::BGRx),
                        alternatives: formats.iter().map(|f| id(*f)).collect(),
                    },
                ))),
            ),
            property(
                FormatProperties::VideoSize,
                Value::Choice(ChoiceValue::Rectangle(Choice(
                    ChoiceFlags::empty(),
                    ChoiceEnum::Range {
                        default: Rectangle {
                            width: width.max(1),
                            height: height.max(1),
                        },
                        min: Rectangle {
                            width: 1,
                            height: 1,
                        },
                        max: Rectangle {
                            width: 16384,
                            height: 16384,
                        },
                    },
                ))),
            ),
            property(
                FormatProperties::VideoFramerate,
                Value::Fraction(Fraction { num: 0, denom: 1 }),
            ),
            property(
                FormatProperties::VideoMaxFramerate,
                Value::Choice(ChoiceValue::Fraction(Choice(
                    ChoiceFlags::empty(),
                    ChoiceEnum::Range {
                        default: Fraction { num: fps, denom: 1 },
                        min: Fraction { num: 1, denom: 1 },
                        max: Fraction { num: fps, denom: 1 },
                    },
                ))),
            ),
        ],
    })
}

/// The buffers taken: shared memory, mapped by PipeWire (MAP_BUFFERS).
fn buffers() -> Vec<u8> {
    let types = (1i32 << spa::sys::SPA_DATA_MemFd) | (1i32 << spa::sys::SPA_DATA_MemPtr);
    serialized(Object {
        type_: SpaTypes::ObjectParamBuffers.as_raw(),
        id: ParamType::Buffers.as_raw(),
        properties: vec![Property {
            key: spa::sys::SPA_PARAM_BUFFERS_dataType,
            flags: PropertyFlags::empty(),
            value: Value::Choice(ChoiceValue::Int(Choice(
                ChoiceFlags::empty(),
                ChoiceEnum::Flags {
                    default: types,
                    flags: vec![types],
                },
            ))),
        }],
    })
}

/// A kind of metadata asked for on every buffer: the header (whose flags
/// mark a corrupted frame) and the crop (the part of a buffer that is the
/// shared window).
fn meta(kind: u32, size: usize) -> Vec<u8> {
    serialized(Object {
        type_: SpaTypes::ObjectParamMeta.as_raw(),
        id: ParamType::Meta.as_raw(),
        properties: vec![
            Property {
                key: spa::sys::SPA_PARAM_META_type,
                flags: PropertyFlags::empty(),
                value: Value::Id(Id(kind)),
            },
            Property {
                key: spa::sys::SPA_PARAM_META_size,
                flags: PropertyFlags::empty(),
                value: Value::Int(size as i32),
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_offer_is_a_pod_pipewire_can_read() {
        for bytes in [
            enum_format(30, Some((2560, 1440))),
            buffers(),
            meta(spa::sys::SPA_META_Header, 32),
        ] {
            assert!(!bytes.is_empty());
            let pod = Pod::from_bytes(&bytes).expect("a pod");
            assert!(pod.is_object());
        }
    }

    #[test]
    fn every_offered_format_has_a_layout() {
        for format in [
            VideoFormat::BGRx,
            VideoFormat::BGRA,
            VideoFormat::RGBx,
            VideoFormat::RGBA,
            VideoFormat::xRGB,
            VideoFormat::ARGB,
            VideoFormat::xBGR,
            VideoFormat::ABGR,
        ] {
            assert!(layout_of(format).is_some(), "{format:?}");
        }
        assert_eq!(layout_of(VideoFormat::I420), None);
    }
}
