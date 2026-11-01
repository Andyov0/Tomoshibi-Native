//! What a test sends in place of the camera (`TOMOSHIBI_SOURCE`, debug
//! builds only): a generated moving picture, or a Y4M file looped. There is
//! no camera in a container, and a test must never capture the room of
//! whoever runs it.
//!
//! The generated picture is made in the layout a device would send
//! (`pattern:nv12`, `pattern:yuyv`, `pattern:i420`), with padded rows, and
//! read back through the same conversion a device's frames go through
//! (frame.rs), so that a call carrying it has been through the camera's
//! own path from the device's bytes on. Eight bars of colour, and a white
//! square that crosses them once every two seconds, so that a frozen
//! picture is told from a moving one in a browser's screenshot.

use std::{
    fs::File,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use tokio::sync::oneshot;

use super::{
    feed::Feed,
    frame::{self, PixelFormat, Planes, Raw},
};
use crate::{app::log, media::MediaError};

/// The bars' colours as Y, U and V: white, yellow, cyan, green, magenta,
/// red, blue, black (BT.601).
const BARS: [(u8, u8, u8); 8] = [
    (235, 128, 128),
    (210, 16, 146),
    (170, 166, 16),
    (145, 54, 34),
    (106, 202, 222),
    (81, 90, 240),
    (41, 240, 110),
    (16, 128, 128),
];

/// The generated picture at a moment: bars and the square at `frame`.
pub fn picture(width: u32, height: u32, frame: u64, out: &mut Planes) {
    *out = Planes::new(width, height);
    let (w, h) = (out.width as usize, out.height as usize);
    let (cw, ch) = out.chroma();
    let (y, u, v) = out.split_mut();
    for row in 0..h {
        for column in 0..w {
            y[row * w + column] = BARS[column * 8 / w].0;
        }
    }
    for row in 0..ch {
        for column in 0..cw {
            let bar = BARS[column * 2 * 8 / w];
            u[row * cw + column] = bar.1;
            v[row * cw + column] = bar.2;
        }
    }
    // The square, a sixth of the height, across the middle once every
    // sixty frames.
    let side = (h / 6).max(2) & !1;
    let left = ((frame % 60) as usize * (w - side) / 59) & !1;
    let top = ((h - side) / 2) & !1;
    for row in top..top + side {
        for column in left..left + side {
            y[row * w + column] = 235;
        }
    }
    for row in top / 2..(top + side) / 2 {
        for column in left / 2..(left + side) / 2 {
            u[row * cw + column] = 128;
            v[row * cw + column] = 128;
        }
    }
}

/// The picture as a device would send it, and the stride its rows are at.
pub fn as_device(planes: &Planes, format: PixelFormat) -> (Vec<u8>, u32) {
    let w = planes.width as usize;
    match format {
        PixelFormat::Yuyv => {
            let stride = w * 2 + 32;
            (frame::to_yuyv(planes, stride), stride as u32)
        }
        PixelFormat::I420 => {
            let stride = w + 64;
            (frame::to_i420(planes, stride), stride as u32)
        }
        // NV12, and MJPEG, which a test cannot make without an encoder.
        _ => {
            let stride = w + 64;
            (frame::to_nv12(planes, stride), stride as u32)
        }
    }
}

/// What the test sends.
#[derive(Debug, Clone, PartialEq)]
pub enum TestPicture {
    Pattern(PixelFormat),
    Y4m(PathBuf),
}

/// Starts the test camera's thread: 1280x720 at thirty, or the file's own
/// size and rate.
pub fn start(
    picture: TestPicture,
    feed: Arc<Feed>,
    stop: Arc<AtomicBool>,
    opened: oneshot::Sender<Result<(), MediaError>>,
) {
    let spawned = thread::Builder::new()
        .name("tomoshibi test camera".into())
        .spawn(move || match picture {
            TestPicture::Pattern(format) => {
                let _ = opened.send(Ok(()));
                pattern(format, &feed, &stop);
            }
            TestPicture::Y4m(path) => match Y4m::open(&path) {
                Ok(file) => {
                    let _ = opened.send(Ok(()));
                    file.play(&feed, &stop);
                }
                Err(fault) => {
                    let _ = opened.send(Err(MediaError::Failed(fault)));
                }
            },
        });
    if let Err(fault) = spawned {
        log(&format!(
            "camera: the test camera's thread did not start: {fault}"
        ));
    }
}

/// Waits for the next frame's moment, `interval` after the last.
fn pace(next: &mut Instant, interval: Duration) {
    *next += interval;
    let now = Instant::now();
    if *next > now {
        thread::sleep(*next - now);
    } else if now - *next > Duration::from_millis(500) {
        *next = now;
    }
}

fn pattern(format: PixelFormat, feed: &Feed, stop: &AtomicBool) {
    let mut planes = Planes::default();
    let mut next = Instant::now();
    let mut count = 0u64;
    while !stop.load(Ordering::Relaxed) {
        picture(1280, 720, count, &mut planes);
        let (bytes, stride) = as_device(&planes, format);
        let raw = Raw {
            format,
            width: 1280,
            height: 720,
            planes: vec![(&bytes, stride)],
        };
        feed.deliver(&raw, feed.now_us());
        count += 1;
        pace(&mut next, Duration::from_micros(33_333));
    }
}

/// A YUV4MPEG2 file of 4:2:0 frames.
struct Y4m {
    reader: BufReader<File>,
    width: u32,
    height: u32,
    interval: Duration,
    /// Where the first frame starts.
    first: u64,
}

impl Y4m {
    fn open(path: &Path) -> Result<Y4m, String> {
        let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut reader = BufReader::new(file);
        let mut header = String::new();
        reader.read_line(&mut header).map_err(|e| e.to_string())?;
        let (width, height, fps) = parse_header(&header)?;
        let first = reader.stream_position().map_err(|e| e.to_string())?;
        Ok(Y4m {
            reader,
            width,
            height,
            interval: Duration::from_secs_f64(1.0 / fps.max(1.0)),
            first,
        })
    }

    fn play(mut self, feed: &Feed, stop: &AtomicBool) {
        let size = (self.width * self.height) as usize;
        let chroma = (self.width.div_ceil(2) * self.height.div_ceil(2)) as usize;
        let mut data = vec![0u8; size + 2 * chroma];
        let mut line = String::new();
        let mut next = Instant::now();
        while !stop.load(Ordering::Relaxed) {
            line.clear();
            match self.reader.read_line(&mut line) {
                Ok(0) => {
                    // The end: round again.
                    if self.reader.seek(SeekFrom::Start(self.first)).is_err() {
                        return;
                    }
                    continue;
                }
                Ok(_) if line.starts_with("FRAME") => {}
                _ => return,
            }
            if self.reader.read_exact(&mut data).is_err() {
                if self.reader.seek(SeekFrom::Start(self.first)).is_err() {
                    return;
                }
                continue;
            }
            let (y, rest) = data.split_at(size);
            let (u, v) = rest.split_at(chroma);
            let half = self.width.div_ceil(2);
            let raw = Raw {
                format: PixelFormat::I420,
                width: self.width,
                height: self.height,
                planes: vec![(y, self.width), (u, half), (v, half)],
            };
            feed.deliver(&raw, feed.now_us());
            pace(&mut next, self.interval);
        }
    }
}

/// The size and rate of a Y4M header; only 4:2:0 is taken.
fn parse_header(header: &str) -> Result<(u32, u32, f64), String> {
    let mut parts = header.split_whitespace();
    if parts.next() != Some("YUV4MPEG2") {
        return Err("not a Y4M file".into());
    }
    let (mut width, mut height, mut fps) = (0, 0, 30.0);
    for part in parts {
        let (tag, value) = part.split_at(1);
        match tag {
            "W" => width = value.parse().map_err(|_| "a bad width")?,
            "H" => height = value.parse().map_err(|_| "a bad height")?,
            "F" => {
                if let Some((n, d)) = value.split_once(':') {
                    let (n, d): (f64, f64) = (n.parse().unwrap_or(30.0), d.parse().unwrap_or(1.0));
                    if d > 0.0 {
                        fps = n / d;
                    }
                }
            }
            "C" if !value.starts_with("420") => {
                return Err(format!("only 4:2:0 is taken, not {value}"))
            }
            _ => {}
        }
    }
    if width < 2 || height < 2 {
        return Err("no size".into());
    }
    Ok((width, height, fps))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pattern_reads_back_the_same_from_every_device_layout() {
        let mut planes = Planes::default();
        picture(320, 180, 7, &mut planes);
        for format in [PixelFormat::Nv12, PixelFormat::Yuyv, PixelFormat::I420] {
            let (bytes, stride) = as_device(&planes, format);
            let mut back = Planes::default();
            frame::convert(
                &Raw {
                    format,
                    width: 320,
                    height: 180,
                    planes: vec![(&bytes, stride)],
                },
                &mut back,
            )
            .unwrap();
            assert_eq!(back, planes, "{format:?}");
        }
    }

    #[test]
    fn the_square_moves() {
        let (mut a, mut b) = (Planes::default(), Planes::default());
        picture(320, 180, 0, &mut a);
        picture(320, 180, 30, &mut b);
        assert_ne!(a, b);
        picture(320, 180, 60, &mut b);
        assert_eq!(a, b, "once round in sixty frames");
    }

    #[test]
    fn a_y4m_header_gives_the_size_and_rate() {
        assert_eq!(
            parse_header("YUV4MPEG2 W1280 H720 F30000:1001 Ip A1:1 C420jpeg\n")
                .unwrap()
                .0,
            1280
        );
        let (_, h, fps) = parse_header("YUV4MPEG2 W640 H360 F25:1\n").unwrap();
        assert_eq!(h, 360);
        assert!((fps - 25.0).abs() < 1e-9);
        assert!(parse_header("YUV4MPEG2 W640 H360 C444\n").is_err());
        assert!(parse_header("RIFF").is_err());
    }
}
