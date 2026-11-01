//! A shared screen's pixels as PipeWire hands them over, made into the rows
//! the core's `VideoInput::push_bgra` takes, and the sizes a share is sent
//! at.
//!
//! Every portal backend offers four bytes a pixel in one byte order or
//! another (mutter and xdg-desktop-portal-wlr BGRx, KWin RGBx or BGRA, a
//! wlroots compositor on some GPUs xBGR). The core converts with libyuv's
//! ARGBToI420, which reads blue, green, red in that order and ignores the
//! fourth byte, so BGRx and BGRA are copied row by row and the others have
//! their bytes moved. A screen sent with red and blue swapped looks almost
//! right (a blue sky shared as orange is the tell), which is why the rig's
//! test reads colour bars back from the browser.

use tomoshibi_protocol::share::{share_capture_request, ShareQuality};

/// The four-byte layouts taken, named as SPA names them: by the bytes'
/// order in memory, the `x` or `A` byte padding or alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// B, G, R, x (also BGRA): what push_bgra takes as it is.
    Bgrx,
    /// R, G, B, x (also RGBA).
    Rgbx,
    /// x, R, G, B (also ARGB).
    Xrgb,
    /// x, B, G, R (also ABGR).
    Xbgr,
}

impl Layout {
    /// Where blue, green and red are among a pixel's four bytes.
    fn offsets(self) -> (usize, usize, usize) {
        match self {
            Layout::Bgrx => (0, 1, 2),
            Layout::Rgbx => (2, 1, 0),
            Layout::Xrgb => (3, 2, 1),
            Layout::Xbgr => (1, 2, 3),
        }
    }
}

/// A part of a picture, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn whole(width: u32, height: u32) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    /// This part where it lies wholly within a picture of `width` by
    /// `height`; nothing otherwise. A window's stream says which part of
    /// each buffer is the window (SPA_META_VideoCrop), and a crop that has
    /// not caught up with a resize can reach past the buffer's edge.
    pub fn within(self, width: u32, height: u32) -> Option<Rect> {
        let fits = self.width > 0
            && self.height > 0
            && self
                .x
                .checked_add(self.width)
                .is_some_and(|right| right <= width)
            && self
                .y
                .checked_add(self.height)
                .is_some_and(|bottom| bottom <= height);
        fits.then_some(self)
    }
}

/// Copies `part` of a picture laid out as `layout`, `stride` bytes from one
/// row to the next, into `out` as tightly packed rows of blue, green, red
/// and a fourth byte, `part.width * 4` bytes a row. False, and `out`
/// untouched, where the picture is shorter than the stride and the part
/// say it is: a buffer whose chunk is smaller than its format promised is
/// a frame not sent, rather than a read past its end.
pub fn to_bgra(
    layout: Layout,
    picture: &[u8],
    stride: usize,
    part: Rect,
    out: &mut Vec<u8>,
) -> bool {
    let (x, y, width, height) = (
        part.x as usize,
        part.y as usize,
        part.width as usize,
        part.height as usize,
    );
    if width == 0 || height == 0 || stride < (x + width) * 4 {
        return false;
    }
    let last_row_end = (y + height - 1) * stride + (x + width) * 4;
    if picture.len() < last_row_end {
        return false;
    }
    let row_bytes = width * 4;
    out.clear();
    out.resize(row_bytes * height, 0);
    let (b, g, r) = layout.offsets();
    for (row, to) in out.chunks_exact_mut(row_bytes).enumerate() {
        let from = &picture[(y + row) * stride + x * 4..][..row_bytes];
        if layout == Layout::Bgrx {
            to.copy_from_slice(from);
            continue;
        }
        for (pixel, source) in to
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(from.as_chunks::<4>().0)
        {
            pixel[0] = source[b];
            pixel[1] = source[g];
            pixel[2] = source[r];
            pixel[3] = 255;
        }
    }
    true
}

/// The size a share is published at for content of this size
/// (windows/Tomoshibi/Media/ScreenCapture.cs `Fit`, Android's
/// ScreenCapture.kt `fit`): the content's own shape, its shorter side
/// brought down to what the setting asks the capture for (the protocol
/// crate's `share_capture_request`, which follows room.ts), on even
/// numbers, as an encoder wants them. Never made larger: a 1080p screen
/// shared at the 1440p setting goes at 1080p.
pub fn fit(content: (u32, u32), quality: ShareQuality, fps: u32) -> (u32, u32) {
    let ask = share_capture_request(fps, quality);
    shorter_side_within(content, ask.width.min(ask.height))
}

/// The size asked for, kept to the shape of what is captured now
/// (ScreenCapture.cs `Fitted`): a window dragged to another shape is sent
/// at its new shape within the same shorter side, never stretched to the
/// old one.
pub fn fitted(asked: (u32, u32), content: (u32, u32)) -> (u32, u32) {
    if asked.0 == 0 || asked.1 == 0 {
        return even(content);
    }
    shorter_side_within(content, asked.0.min(asked.1))
}

fn shorter_side_within(content: (u32, u32), limit: u32) -> (u32, u32) {
    let (width, height) = content;
    let shorter = width.min(height);
    if shorter <= limit || shorter == 0 {
        return even(content);
    }
    let scaled = |side: u32| (u64::from(side) * u64::from(limit) / u64::from(shorter)) as u32;
    even((scaled(width), scaled(height)))
}

fn even((width, height): (u32, u32)) -> (u32, u32) {
    ((width & !1).max(2), (height & !1).max(2))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One pixel of each layout holding blue 10, green 20, red 30, and the
    /// fourth byte 99.
    fn pixel(layout: Layout) -> [u8; 4] {
        match layout {
            Layout::Bgrx => [10, 20, 30, 99],
            Layout::Rgbx => [30, 20, 10, 99],
            Layout::Xrgb => [99, 30, 20, 10],
            Layout::Xbgr => [99, 10, 20, 30],
        }
    }

    #[test]
    fn every_layout_comes_out_blue_green_red() {
        for layout in [Layout::Bgrx, Layout::Rgbx, Layout::Xrgb, Layout::Xbgr] {
            let picture: Vec<u8> = pixel(layout).repeat(6);
            let mut out = Vec::new();
            assert!(to_bgra(layout, &picture, 12, Rect::whole(3, 2), &mut out));
            assert_eq!(out.len(), 24, "{layout:?}");
            for got in out.as_chunks::<4>().0 {
                assert_eq!(&got[..3], &[10, 20, 30], "{layout:?}");
            }
        }
    }

    #[test]
    fn padding_at_the_end_of_each_row_is_left_behind() {
        // Two pixels a row, sixteen bytes a row: eight bytes of padding.
        let mut picture = vec![0u8; 32];
        for row in 0..2 {
            for column in 0..2 {
                let at = row * 16 + column * 4;
                picture[at..at + 4].copy_from_slice(&[row as u8, column as u8, 7, 0]);
            }
            picture[row * 16 + 8..row * 16 + 16].fill(0xEE);
        }
        let mut out = Vec::new();
        assert!(to_bgra(
            Layout::Bgrx,
            &picture,
            16,
            Rect::whole(2, 2),
            &mut out
        ));
        assert_eq!(out, [0, 0, 7, 0, 0, 1, 7, 0, 1, 0, 7, 0, 1, 1, 7, 0]);
    }

    #[test]
    fn a_crop_takes_its_part_only() {
        // Four by three, each pixel's blue its column and green its row.
        let mut picture = vec![0u8; 4 * 4 * 3];
        for row in 0..3 {
            for column in 0..4 {
                let at = row * 16 + column * 4;
                picture[at..at + 4].copy_from_slice(&[column as u8, row as u8, 0, 0]);
            }
        }
        let part = Rect {
            x: 1,
            y: 1,
            width: 2,
            height: 2,
        };
        let mut out = Vec::new();
        assert!(to_bgra(Layout::Bgrx, &picture, 16, part, &mut out));
        let read: Vec<(u8, u8)> = out
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| (p[0], p[1]))
            .collect();
        assert_eq!(read, [(1, 1), (2, 1), (1, 2), (2, 2)]);
    }

    #[test]
    fn a_picture_shorter_than_it_says_is_not_read() {
        let picture = vec![0u8; 4 * 4 * 2 - 1];
        let mut out = vec![1, 2, 3];
        assert!(!to_bgra(
            Layout::Rgbx,
            &picture,
            16,
            Rect::whole(4, 2),
            &mut out
        ));
        assert_eq!(out, [1, 2, 3]);
        assert!(!to_bgra(
            Layout::Bgrx,
            &picture,
            8,
            Rect::whole(4, 1),
            &mut out
        ));
    }

    #[test]
    fn a_crop_past_the_edge_is_not_taken() {
        let crop = Rect {
            x: 100,
            y: 0,
            width: 1000,
            height: 50,
        };
        assert_eq!(crop.within(1100, 50), Some(crop));
        assert_eq!(crop.within(1099, 50), None);
        assert_eq!(Rect::whole(0, 10).within(10, 10), None);
    }

    #[test]
    fn a_share_keeps_its_shape_and_never_grows() {
        // Automatic asks for 2560x1440: a 4K screen comes down to 1440 lines.
        assert_eq!(fit((3840, 2160), ShareQuality::Auto, 60), (2560, 1440));
        // A 1080p screen is not made larger.
        assert_eq!(fit((1920, 1080), ShareQuality::P1440, 30), (1920, 1080));
        // A tall window keeps its shape, its shorter side brought down.
        assert_eq!(fit((1600, 2400), ShareQuality::P1080, 30), (1080, 1620));
        // Odd sides are made even.
        assert_eq!(fit((1281, 721), ShareQuality::Auto, 30), (1280, 720));
    }

    #[test]
    fn a_resized_window_is_sent_at_its_new_shape() {
        assert_eq!(fitted((1920, 1080), (1000, 3000)), (1000, 3000));
        assert_eq!(fitted((1920, 1080), (4000, 2000)), (2160, 1080));
        assert_eq!(fitted((0, 0), (641, 481)), (640, 480));
    }
}
