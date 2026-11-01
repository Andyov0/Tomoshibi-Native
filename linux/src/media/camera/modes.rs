//! Which of a camera's modes to capture in: the web asks for 1280x720 at
//! thirty (room.ts `videoCaptureDefaults`, the core's `camera_capture`), and
//! a camera offers a list of formats, sizes and rates to pick from.
//!
//! The rate comes first, the size second, the format last: a USB camera
//! offers 720p as YUYV at ten frames a second and as MJPEG at thirty, and a
//! picture at ten a second is the one people notice. Then the smallest size
//! that covers what is wanted (a larger one is sent as it is, as a browser
//! would be given it), or else the largest there is below it. Among equals,
//! a raw format before MJPEG, which costs a decode per frame.

use super::frame::PixelFormat;

/// One way a camera can capture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mode {
    pub format: PixelFormat,
    pub width: u32,
    pub height: u32,
    /// The highest rate it offers at this size.
    pub fps: f32,
}

/// The size and rate asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wanted {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

fn format_rank(format: PixelFormat) -> u8 {
    match format {
        PixelFormat::Nv12 => 0,
        PixelFormat::I420 => 1,
        PixelFormat::Yuyv => 2,
        PixelFormat::Mjpeg => 3,
    }
}

/// How far a size is from what is wanted: a size that covers it costs its
/// area over the wanted one (one for an exact fit), anything smaller costs
/// more than any that covers, less the larger it is.
fn size_cost(mode: &Mode, wanted: &Wanted) -> f64 {
    let area = f64::from(mode.width) * f64::from(mode.height);
    let want = f64::from(wanted.width) * f64::from(wanted.height);
    if mode.width >= wanted.width && mode.height >= wanted.height {
        area / want
    } else {
        1000.0 + want / area.max(1.0)
    }
}

/// The mode to capture in, if any.
pub fn choose(modes: &[Mode], wanted: Wanted) -> Option<Mode> {
    modes
        .iter()
        .copied()
        .filter(|m| m.width >= 2 && m.height >= 2)
        .min_by(|a, b| {
            let rate_ok = |m: &Mode| m.fps + 0.5 >= wanted.fps as f32;
            rate_ok(b)
                .cmp(&rate_ok(a))
                .then(size_cost(a, &wanted).total_cmp(&size_cost(b, &wanted)))
                .then(format_rank(a.format).cmp(&format_rank(b.format)))
                .then(b.fps.total_cmp(&a.fps))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HD: Wanted = Wanted {
        width: 1280,
        height: 720,
        fps: 30,
    };

    fn mode(format: PixelFormat, width: u32, height: u32, fps: f32) -> Mode {
        Mode {
            format,
            width,
            height,
            fps,
        }
    }

    #[test]
    fn a_usb_camera_sends_720p_as_mjpeg_at_thirty_rather_than_yuyv_at_ten() {
        let modes = [
            mode(PixelFormat::Yuyv, 640, 480, 30.0),
            mode(PixelFormat::Yuyv, 1280, 720, 10.0),
            mode(PixelFormat::Mjpeg, 640, 480, 30.0),
            mode(PixelFormat::Mjpeg, 1280, 720, 30.0),
            mode(PixelFormat::Mjpeg, 1920, 1080, 30.0),
        ];
        assert_eq!(
            choose(&modes, HD),
            Some(mode(PixelFormat::Mjpeg, 1280, 720, 30.0))
        );
    }

    #[test]
    fn a_raw_format_wins_where_it_is_as_good() {
        let modes = [
            mode(PixelFormat::Mjpeg, 1280, 720, 30.0),
            mode(PixelFormat::Nv12, 1280, 720, 30.0),
        ];
        assert_eq!(choose(&modes, HD).unwrap().format, PixelFormat::Nv12);
    }

    #[test]
    fn the_smallest_size_that_covers_or_else_the_largest_below() {
        let modes = [
            mode(PixelFormat::Nv12, 1920, 1080, 30.0),
            mode(PixelFormat::Nv12, 3840, 2160, 30.0),
        ];
        assert_eq!(choose(&modes, HD).unwrap().width, 1920);
        let modes = [
            mode(PixelFormat::Nv12, 320, 240, 30.0),
            mode(PixelFormat::Nv12, 640, 480, 30.0),
        ];
        assert_eq!(choose(&modes, HD).unwrap().width, 640);
    }

    #[test]
    fn a_rate_below_the_one_wanted_is_taken_only_when_nothing_else_is_there() {
        let modes = [mode(PixelFormat::Yuyv, 1280, 720, 10.0)];
        assert_eq!(choose(&modes, HD).unwrap().fps, 10.0);
        assert_eq!(choose(&[], HD), None);
    }
}
