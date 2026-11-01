//! Cameras through V4L2 directly: where there is no camera portal and no
//! PipeWire, and for the cameras PipeWire does not list. Every USB camera
//! and most built-in ones are a /dev/video node.
//!
//! A node is listed when it captures video in a format the camera module
//! reads (some nodes of a camera carry only metadata, and some capture only
//! formats nothing here reads). Its id is the stable name udev gives it
//! under /dev/v4l/by-id (or by-path), so that a camera chosen last time is
//! found again after the nodes are numbered differently, and its label is
//! the driver's name for the card.
//!
//! Capture is memory-mapped streaming of four buffers, each frame read into
//! the feed on this thread before its buffer is given back; a camera
//! unplugged ends the stream, and the feed's owner is told.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use tokio::sync::oneshot;
use v4l::{
    buffer::Type,
    capability::Flags,
    format::FourCC,
    frameinterval::FrameIntervalEnum,
    framesize::FrameSizeEnum,
    io::traits::{CaptureStream, Stream as _},
    prelude::*,
    video::{capture::Parameters, Capture},
};

use super::{
    feed::Feed,
    frame::{PixelFormat, Raw},
    modes::{self, Mode, Wanted},
};
use crate::{app::log, media::MediaError};

/// A camera node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// The stable path when there is one, else the node itself.
    pub id: String,
    pub path: PathBuf,
    pub label: String,
}

/// The stable names udev gives the video nodes, by where they point.
fn stable_names() -> Vec<(PathBuf, PathBuf)> {
    let mut names = Vec::new();
    for directory in ["/dev/v4l/by-id", "/dev/v4l/by-path"] {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            if let Ok(target) = fs::canonicalize(entry.path()) {
                names.push((target, entry.path()));
            }
        }
    }
    names
}

/// The camera nodes there are now.
pub fn nodes() -> Vec<Node> {
    let stable = stable_names();
    let mut found: Vec<(usize, Node)> = Vec::new();
    for node in v4l::context::enum_devices() {
        let path = node.path().to_path_buf();
        let Ok(device) = Device::with_path(&path) else {
            continue;
        };
        let Ok(caps) = device.query_caps() else {
            continue;
        };
        if !caps.capabilities.contains(Flags::VIDEO_CAPTURE)
            || !caps.capabilities.contains(Flags::STREAMING)
        {
            continue;
        }
        let readable = device
            .enum_formats()
            .map(|formats| {
                formats
                    .iter()
                    .any(|f| PixelFormat::of_fourcc(&f.fourcc.repr).is_some())
            })
            .unwrap_or(false);
        if !readable {
            continue;
        }
        let canonical = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        let id = stable
            .iter()
            .find(|(target, _)| *target == canonical)
            .map(|(_, name)| name.display().to_string())
            .unwrap_or_else(|| path.display().to_string());
        let label = if caps.card.trim().is_empty() {
            path.display().to_string()
        } else {
            caps.card.trim().to_owned()
        };
        found.push((node.index(), Node { id, path, label }));
    }
    found.sort_by_key(|(index, _)| *index);
    found.into_iter().map(|(_, node)| node).collect()
}

/// Every mode a node offers in a format read here, with the best rate at
/// each size.
fn modes_of(device: &Device) -> Vec<Mode> {
    let mut modes = Vec::new();
    let Ok(formats) = device.enum_formats() else {
        return modes;
    };
    for description in formats {
        let Some(format) = PixelFormat::of_fourcc(&description.fourcc.repr) else {
            continue;
        };
        let Ok(sizes) = device.enum_framesizes(description.fourcc) else {
            continue;
        };
        for size in sizes {
            let discrete: Vec<(u32, u32)> = match size.size {
                FrameSizeEnum::Discrete(d) => vec![(d.width, d.height)],
                // A range: the size wanted where it lies inside, else its ends.
                FrameSizeEnum::Stepwise(s) => {
                    let mut sizes = vec![(s.min_width, s.min_height), (s.max_width, s.max_height)];
                    if (s.min_width..=s.max_width).contains(&1280)
                        && (s.min_height..=s.max_height).contains(&720)
                    {
                        sizes.push((1280, 720));
                    }
                    sizes
                }
            };
            for (width, height) in discrete {
                let fps = device
                    .enum_frameintervals(description.fourcc, width, height)
                    .ok()
                    .and_then(|intervals| {
                        intervals
                            .iter()
                            .map(|interval| match &interval.interval {
                                FrameIntervalEnum::Discrete(f) => {
                                    f.denominator as f32 / f.numerator.max(1) as f32
                                }
                                FrameIntervalEnum::Stepwise(s) => {
                                    s.min.denominator as f32 / s.min.numerator.max(1) as f32
                                }
                            })
                            .reduce(f32::max)
                    })
                    .unwrap_or(30.0);
                modes.push(Mode {
                    format,
                    width,
                    height,
                    fps,
                });
            }
        }
    }
    modes
}

/// Opens `node` and captures it into `feed` on a thread of its own until
/// `stop`; `opened` is told how the open went, and `ended` is called if the
/// camera stops by itself (unplugged).
pub fn start(
    node: Node,
    wanted: Wanted,
    feed: Arc<Feed>,
    stop: Arc<AtomicBool>,
    opened: oneshot::Sender<Result<(), MediaError>>,
    ended: impl FnOnce() + Send + 'static,
) {
    let spawned = thread::Builder::new()
        .name("tomoshibi camera".into())
        .spawn(move || capture(node, wanted, feed, stop, opened, ended));
    if let Err(fault) = spawned {
        log(&format!(
            "camera: the capture thread did not start: {fault}"
        ));
    }
}

fn refusal(fault: &std::io::Error) -> MediaError {
    match fault.raw_os_error() {
        Some(libc_codes::EBUSY) => MediaError::Busy,
        Some(libc_codes::EACCES) | Some(libc_codes::EPERM) => MediaError::Denied,
        Some(libc_codes::ENOENT) | Some(libc_codes::ENODEV) | Some(libc_codes::ENXIO) => {
            MediaError::NotFound
        }
        _ => MediaError::Failed(fault.to_string()),
    }
}

/// The few error numbers the camera tells apart, as Linux numbers them.
mod libc_codes {
    pub const EPERM: i32 = 1;
    pub const ENOENT: i32 = 2;
    pub const ENXIO: i32 = 6;
    pub const EACCES: i32 = 13;
    pub const EBUSY: i32 = 16;
    pub const ENODEV: i32 = 19;
}

fn capture(
    node: Node,
    wanted: Wanted,
    feed: Arc<Feed>,
    stop: Arc<AtomicBool>,
    opened: oneshot::Sender<Result<(), MediaError>>,
    ended: impl FnOnce(),
) {
    let device = match Device::with_path(&node.path) {
        Ok(device) => device,
        Err(fault) => {
            let _ = opened.send(Err(refusal(&fault)));
            return;
        }
    };
    let Some(mode) = modes::choose(&modes_of(&device), wanted) else {
        let _ = opened.send(Err(MediaError::Unsupported));
        return;
    };
    let asked = v4l::Format::new(mode.width, mode.height, FourCC::new(&mode.format.fourcc()));
    let format = match device.set_format(&asked) {
        Ok(format) => format,
        Err(fault) => {
            let _ = opened.send(Err(refusal(&fault)));
            return;
        }
    };
    let Some(format_read) = PixelFormat::of_fourcc(&format.fourcc.repr) else {
        let _ = opened.send(Err(MediaError::Unsupported));
        return;
    };
    let _ = device.set_params(&Parameters::with_fps(wanted.fps));
    let mut stream = match MmapStream::with_buffers(&device, Type::VideoCapture, 4) {
        Ok(stream) => stream,
        Err(fault) => {
            let _ = opened.send(Err(refusal(&fault)));
            return;
        }
    };
    // Woken twice a second at least, to see whether it is to stop.
    stream.set_timeout(Duration::from_millis(500));
    log(&format!(
        "camera: {} at {}x{} {} ({} asked)",
        node.label,
        format.width,
        format.height,
        String::from_utf8_lossy(&format.fourcc.repr),
        String::from_utf8_lossy(&mode.format.fourcc()),
    ));
    let _ = opened.send(Ok(()));
    let mut gone = false;
    while !stop.load(Ordering::Relaxed) {
        match stream.next() {
            Ok((data, meta)) => {
                let used = (meta.bytesused as usize).min(data.len());
                let bytes = if used > 0 { &data[..used] } else { data };
                let raw = Raw {
                    format: format_read,
                    width: format.width,
                    height: format.height,
                    planes: vec![(bytes, format.stride)],
                };
                feed.deliver(&raw, feed.now_us());
            }
            Err(fault) if fault.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(fault) => {
                log(&format!("camera: {} stopped: {fault}", node.label));
                gone = true;
                break;
            }
        }
    }
    // The stream's own drop stops it and panics on any failure but a
    // device that is gone; stopped here first, a failure is only logged.
    if let Err(fault) = stream.stop() {
        if fault.raw_os_error() != Some(libc_codes::ENODEV) {
            log(&format!(
                "camera: {} did not stop cleanly: {fault}",
                node.label
            ));
            std::mem::forget(stream);
        }
    }
    if gone {
        ended();
    }
}

/// The path a camera id names: a stable name or a node.
pub fn find(id: &str, nodes: &[Node]) -> Option<Node> {
    nodes
        .iter()
        .find(|n| n.id == id || n.path == Path::new(id))
        .cloned()
}
