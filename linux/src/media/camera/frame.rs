//! A camera's frames, in whatever layout the device gives them, made into
//! the one the rest of the camera works in: I420 in three packed planes, at
//! an even width and height, which is what `VideoInput::push_planes` takes
//! with a chroma step of one, what the blur works on (BlurMath's planes),
//! and what a preview sink is handed as `RemoteFrame::i420`.
//!
//! The layouts are the ones a webcam offers at 1280x720 (V4L2 and PipeWire
//! alike): NV12 and planar I420 from most built-in cameras, YUYV from
//! nearly every USB one (which at 720p usually manages ten frames a second
//! at most over USB 2), and MJPEG, which is how those same cameras send 720p
//! at thirty. The colours are passed on as the device gives them, BT.601 as
//! cameras deliver it, without a change of range, as Windows does
//! (Planes.cs) and as libyuv's MJPGToI420 does for a browser.

use zune_jpeg::{
    zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions},
    JpegDecoder,
};

/// How a device lays its frames out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    /// Planar 4:2:0: luma, then U, then V at half the width and height.
    I420,
    /// Luma, then U and V interleaved at half the height.
    Nv12,
    /// Packed 4:2:2: Y0 U Y1 V for each two pixels.
    Yuyv,
    /// A JPEG per frame.
    Mjpeg,
}

impl PixelFormat {
    /// The V4L2 fourcc.
    pub fn fourcc(self) -> [u8; 4] {
        match self {
            PixelFormat::I420 => *b"YU12",
            PixelFormat::Nv12 => *b"NV12",
            PixelFormat::Yuyv => *b"YUYV",
            PixelFormat::Mjpeg => *b"MJPG",
        }
    }

    pub fn of_fourcc(fourcc: &[u8; 4]) -> Option<PixelFormat> {
        match fourcc {
            b"YU12" | b"I420" => Some(PixelFormat::I420),
            b"NV12" => Some(PixelFormat::Nv12),
            b"YUYV" | b"YUY2" => Some(PixelFormat::Yuyv),
            b"MJPG" | b"JPEG" => Some(PixelFormat::Mjpeg),
            _ => None,
        }
    }
}

/// A frame as the device gave it: its planes, each with the bytes from one
/// row to the next. One buffer holds every plane for the layouts V4L2 gives
/// (the chroma follows the luma); PipeWire may hand them over apart.
pub struct Raw<'a> {
    pub format: PixelFormat,
    pub width: u32,
    pub height: u32,
    pub planes: Vec<(&'a [u8], u32)>,
}

/// An I420 picture, its planes packed one after the other without padding.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Planes {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Planes {
    pub fn new(width: u32, height: u32) -> Planes {
        let mut planes = Planes::default();
        planes.reshape(width, height);
        planes
    }

    /// The size of a chroma plane: half each way.
    pub fn chroma(&self) -> (usize, usize) {
        ((self.width / 2) as usize, (self.height / 2) as usize)
    }

    fn reshape(&mut self, width: u32, height: u32) {
        self.width = width & !1;
        self.height = height & !1;
        let luma = (self.width * self.height) as usize;
        self.data.resize(luma + luma / 2, 0);
    }

    /// The three planes, apart.
    pub fn split(&self) -> (&[u8], &[u8], &[u8]) {
        let luma = (self.width * self.height) as usize;
        let (y, chroma) = self.data.split_at(luma);
        let (u, v) = chroma.split_at(luma / 4);
        (y, u, v)
    }

    pub fn split_mut(&mut self) -> (&mut [u8], &mut [u8], &mut [u8]) {
        let luma = (self.width * self.height) as usize;
        let (y, chroma) = self.data.split_at_mut(luma);
        let (u, v) = chroma.split_at_mut(luma / 4);
        (y, u, v)
    }
}

/// Why a frame could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// Shorter than its size says, or with no planes.
    Short,
    /// The JPEG would not decode.
    Jpeg(String),
}

/// Reads `raw` into `out`, which takes its size (an odd row or column is
/// left out).
pub fn convert(raw: &Raw, out: &mut Planes) -> Result<(), FrameError> {
    match raw.format {
        PixelFormat::Mjpeg => mjpeg(raw, out),
        PixelFormat::I420 => i420(raw, out),
        PixelFormat::Nv12 => nv12(raw, out),
        PixelFormat::Yuyv => yuyv(raw, out),
    }
}

fn plane<'a>(raw: &Raw<'a>, index: usize) -> Option<(&'a [u8], usize)> {
    raw.planes
        .get(index)
        .map(|(data, stride)| (*data, *stride as usize))
}

fn copy_rows(
    from: &[u8],
    stride: usize,
    width: usize,
    rows: usize,
    to: &mut [u8],
) -> Result<(), FrameError> {
    if rows == 0 || from.len() < (rows - 1) * stride + width || stride < width {
        return Err(FrameError::Short);
    }
    for row in 0..rows {
        to[row * width..(row + 1) * width]
            .copy_from_slice(&from[row * stride..row * stride + width]);
    }
    Ok(())
}

fn i420(raw: &Raw, out: &mut Planes) -> Result<(), FrameError> {
    let (first, stride) = plane(raw, 0).ok_or(FrameError::Short)?;
    // V4L2's YU12 in one buffer: the chroma at half the luma's stride, after
    // the whole luma plane of the frame's own height.
    let rows = raw.height as usize;
    let (u_plane, u_stride, v_plane, v_stride) = match (plane(raw, 1), plane(raw, 2)) {
        (Some((u, us)), Some((v, vs))) => (u, us, v, vs),
        _ => {
            let chroma_stride = stride / 2;
            let u_at = stride * rows;
            let v_at = u_at + chroma_stride * rows.div_ceil(2);
            if first.len() < v_at {
                return Err(FrameError::Short);
            }
            (&first[u_at..], chroma_stride, &first[v_at..], chroma_stride)
        }
    };
    out.reshape(raw.width, raw.height);
    let (w, h) = (out.width as usize, out.height as usize);
    let (cw, ch) = out.chroma();
    let (y, u, v) = out.split_mut();
    copy_rows(first, stride, w, h, y)?;
    copy_rows(u_plane, u_stride, cw, ch, u)?;
    copy_rows(v_plane, v_stride, cw, ch, v)
}

fn nv12(raw: &Raw, out: &mut Planes) -> Result<(), FrameError> {
    let (first, stride) = plane(raw, 0).ok_or(FrameError::Short)?;
    let (uv_plane, uv_stride) = match plane(raw, 1) {
        Some(second) => second,
        None => {
            let at = stride * raw.height as usize;
            if first.len() < at {
                return Err(FrameError::Short);
            }
            (&first[at..], stride)
        }
    };
    out.reshape(raw.width, raw.height);
    let (w, h) = (out.width as usize, out.height as usize);
    let (cw, ch) = out.chroma();
    if uv_plane.len() < (ch.max(1) - 1) * uv_stride + cw * 2 {
        return Err(FrameError::Short);
    }
    let (y, u, v) = out.split_mut();
    copy_rows(first, stride, w, h, y)?;
    for row in 0..ch {
        let line = &uv_plane[row * uv_stride..row * uv_stride + cw * 2];
        for column in 0..cw {
            u[row * cw + column] = line[column * 2];
            v[row * cw + column] = line[column * 2 + 1];
        }
    }
    Ok(())
}

fn yuyv(raw: &Raw, out: &mut Planes) -> Result<(), FrameError> {
    let (data, stride) = plane(raw, 0).ok_or(FrameError::Short)?;
    out.reshape(raw.width, raw.height);
    let (w, h) = (out.width as usize, out.height as usize);
    let (cw, _) = out.chroma();
    if h == 0 || stride < w * 2 || data.len() < (h - 1) * stride + w * 2 {
        return Err(FrameError::Short);
    }
    let (y, u, v) = out.split_mut();
    for pair in 0..h / 2 {
        let top = &data[pair * 2 * stride..pair * 2 * stride + w * 2];
        let bottom = &data[(pair * 2 + 1) * stride..(pair * 2 + 1) * stride + w * 2];
        for column in 0..cw {
            let (t, b) = (
                &top[column * 4..column * 4 + 4],
                &bottom[column * 4..column * 4 + 4],
            );
            y[pair * 2 * w + column * 2] = t[0];
            y[pair * 2 * w + column * 2 + 1] = t[2];
            y[(pair * 2 + 1) * w + column * 2] = b[0];
            y[(pair * 2 + 1) * w + column * 2 + 1] = b[2];
            // 4:2:2 to 4:2:0: the two rows' chroma averaged.
            u[pair * cw + column] = (u16::from(t[1]) + u16::from(b[1])).div_ceil(2) as u8;
            v[pair * cw + column] = (u16::from(t[3]) + u16::from(b[3])).div_ceil(2) as u8;
        }
    }
    Ok(())
}

fn mjpeg(raw: &Raw, out: &mut Planes) -> Result<(), FrameError> {
    let (data, _) = plane(raw, 0).ok_or(FrameError::Short)?;
    // YCbCr straight out of the decoder, no conversion to RGB and back:
    // the decoder upsamples the chroma to every pixel, which is taken down
    // to 4:2:0 again here.
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::YCbCr);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(data), options);
    let pixels = decoder
        .decode()
        .map_err(|e| FrameError::Jpeg(format!("{e:?}")))?;
    let info = decoder.info().ok_or(FrameError::Short)?;
    let (width, height) = (info.width as usize, info.height as usize);
    if pixels.len() < width * height * 3 {
        return Err(FrameError::Short);
    }
    out.reshape(width as u32, height as u32);
    let (w, h) = (out.width as usize, out.height as usize);
    let (cw, ch) = out.chroma();
    let (y, u, v) = out.split_mut();
    for row in 0..h {
        for column in 0..w {
            y[row * w + column] = pixels[(row * width + column) * 3];
        }
    }
    for row in 0..ch {
        for column in 0..cw {
            let mut cb = 0u16;
            let mut cr = 0u16;
            for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                let at = ((row * 2 + dy) * width + column * 2 + dx) * 3;
                cb += u16::from(pixels[at + 1]);
                cr += u16::from(pixels[at + 2]);
            }
            u[row * cw + column] = ((cb + 2) / 4) as u8;
            v[row * cw + column] = ((cr + 2) / 4) as u8;
        }
    }
    Ok(())
}

/// The planes as NV12 in one buffer at `stride`: what a generated test
/// picture is turned into before it is read back like a device's.
pub fn to_nv12(planes: &Planes, stride: usize) -> Vec<u8> {
    let (w, h) = (planes.width as usize, planes.height as usize);
    let (cw, ch) = planes.chroma();
    let (y, u, v) = planes.split();
    let mut out = vec![0u8; stride * h + stride * ch];
    for row in 0..h {
        out[row * stride..row * stride + w].copy_from_slice(&y[row * w..(row + 1) * w]);
    }
    for row in 0..ch {
        let at = stride * h + row * stride;
        for column in 0..cw {
            out[at + column * 2] = u[row * cw + column];
            out[at + column * 2 + 1] = v[row * cw + column];
        }
    }
    out
}

/// The planes as YUYV at `stride`, each chroma sample used for both rows.
pub fn to_yuyv(planes: &Planes, stride: usize) -> Vec<u8> {
    let (w, h) = (planes.width as usize, planes.height as usize);
    let (cw, _) = planes.chroma();
    let (y, u, v) = planes.split();
    let mut out = vec![0u8; stride * h];
    for row in 0..h {
        for column in 0..cw {
            let at = row * stride + column * 4;
            let c = (row / 2) * cw + column;
            out[at] = y[row * w + column * 2];
            out[at + 1] = u[c];
            out[at + 2] = y[row * w + column * 2 + 1];
            out[at + 3] = v[c];
        }
    }
    out
}

/// The planes as V4L2's single-buffer I420 at a luma `stride`.
pub fn to_i420(planes: &Planes, stride: usize) -> Vec<u8> {
    let (w, h) = (planes.width as usize, planes.height as usize);
    let (cw, ch) = planes.chroma();
    let (y, u, v) = planes.split();
    let cs = stride / 2;
    let mut out = vec![0u8; stride * h + 2 * cs * ch];
    for row in 0..h {
        out[row * stride..row * stride + w].copy_from_slice(&y[row * w..(row + 1) * w]);
    }
    for (index, plane) in [u, v].into_iter().enumerate() {
        let base = stride * h + index * cs * ch;
        for row in 0..ch {
            out[base + row * cs..base + row * cs + cw]
                .copy_from_slice(&plane[row * cw..(row + 1) * cw]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture whose every sample says where it is, so that a plane read
    /// from the wrong place, or a row from the wrong row, shows.
    fn sample(width: u32, height: u32) -> Planes {
        let mut planes = Planes::new(width, height);
        let (w, _) = (planes.width as usize, planes.height as usize);
        let (cw, _) = planes.chroma();
        let (y, u, v) = planes.split_mut();
        for (i, value) in y.iter_mut().enumerate() {
            *value = ((i % w) * 3 + (i / w) * 7) as u8;
        }
        for (i, value) in u.iter_mut().enumerate() {
            *value = (40 + (i % cw) * 5 + (i / cw) * 11) as u8;
        }
        for (i, value) in v.iter_mut().enumerate() {
            *value = (200 - (i % cw) - (i / cw) * 2) as u8;
        }
        planes
    }

    fn read(format: PixelFormat, width: u32, height: u32, data: &[u8], stride: u32) -> Planes {
        let mut out = Planes::default();
        convert(
            &Raw {
                format,
                width,
                height,
                planes: vec![(data, stride)],
            },
            &mut out,
        )
        .unwrap();
        out
    }

    #[test]
    fn nv12_with_padded_rows_reads_back_exactly() {
        let original = sample(64, 36);
        let data = to_nv12(&original, 80);
        assert_eq!(read(PixelFormat::Nv12, 64, 36, &data, 80), original);
    }

    #[test]
    fn i420_in_one_buffer_reads_back_exactly() {
        let original = sample(64, 36);
        let data = to_i420(&original, 96);
        assert_eq!(read(PixelFormat::I420, 64, 36, &data, 96), original);
    }

    #[test]
    fn i420_in_three_planes_reads_back_exactly() {
        let original = sample(32, 16);
        let (y, u, v) = original.split();
        let mut out = Planes::default();
        let raw = Raw {
            format: PixelFormat::I420,
            width: 32,
            height: 16,
            planes: vec![(y, 32), (u, 16), (v, 16)],
        };
        convert(&raw, &mut out).unwrap();
        assert_eq!(out, original);
    }

    #[test]
    fn yuyv_keeps_the_luma_and_averages_the_chroma_of_two_rows() {
        // The 4:2:0 chroma written to both rows of 4:2:2 comes back as it
        // was; a chroma that differs between the two rows is averaged.
        let original = sample(64, 36);
        let mut data = to_yuyv(&original, 140);
        assert_eq!(read(PixelFormat::Yuyv, 64, 36, &data, 140), original);
        // The second row's first U up by 10: the average moves by 5.
        data[140 + 1] = data[140 + 1].wrapping_add(10);
        let out = read(PixelFormat::Yuyv, 64, 36, &data, 140);
        assert_eq!(out.split().1[0], original.split().1[0] + 5);
    }

    #[test]
    fn an_odd_size_loses_its_last_row_and_column() {
        let mut data = vec![0u8; 65 * 2 * 37];
        data[..4].copy_from_slice(&[10, 20, 30, 40]);
        let out = read(PixelFormat::Yuyv, 65, 37, &data, 130);
        assert_eq!((out.width, out.height), (64, 36));
        assert_eq!(out.split().0[..2], [10, 30]);
    }

    #[test]
    fn a_short_frame_is_refused_not_read_past() {
        let original = sample(64, 36);
        let data = to_nv12(&original, 64);
        let mut out = Planes::default();
        let short = Raw {
            format: PixelFormat::Nv12,
            width: 64,
            height: 36,
            planes: vec![(&data[..data.len() - 1], 64)],
        };
        assert_eq!(convert(&short, &mut out), Err(FrameError::Short));
        let raw = Raw {
            format: PixelFormat::Yuyv,
            width: 64,
            height: 36,
            planes: vec![(&data[..100], 128)],
        };
        assert_eq!(convert(&raw, &mut out), Err(FrameError::Short));
        let none = Raw {
            format: PixelFormat::I420,
            width: 64,
            height: 36,
            planes: vec![],
        };
        assert_eq!(convert(&none, &mut out), Err(FrameError::Short));
    }

    /// A 64x48 JPEG (4:2:0, made by macOS's sips from a picture of four
    /// coloured quarters: red, green, blue and white) decodes to the
    /// quarters' colours. Broken (the chroma planes swapped, or the decoder
    /// handing back RGB), red and blue trade places and this fails.
    #[test]
    fn mjpeg_decodes_to_the_colours_it_shows() {
        let jpeg = include_bytes!("testdata/quarters.jpg");
        let out = read(PixelFormat::Mjpeg, 0, 0, jpeg, 0);
        assert_eq!((out.width, out.height), (64, 48));
        let (y, u, v) = out.split();
        let (cw, w) = (32usize, 64usize);
        let at = |x: usize, yy: usize| {
            (
                y[yy * w + x],
                u[(yy / 2) * cw + x / 2],
                v[(yy / 2) * cw + x / 2],
            )
        };
        // BT.601 full range: red Y 76 U 85 V 255; green 150/44/21;
        // blue 29/255/107; white 255/128/128. JPEG is lossy, so near.
        let near = |(y, u, v): (u8, u8, u8), (ey, eu, ev): (i32, i32, i32)| {
            (i32::from(y) - ey).abs() < 14
                && (i32::from(u) - eu).abs() < 14
                && (i32::from(v) - ev).abs() < 14
        };
        assert!(near(at(8, 8), (76, 85, 255)), "red: {:?}", at(8, 8));
        assert!(near(at(56, 8), (150, 44, 21)), "green: {:?}", at(56, 8));
        assert!(near(at(8, 40), (29, 255, 107)), "blue: {:?}", at(8, 40));
        assert!(near(at(56, 40), (255, 128, 128)), "white: {:?}", at(56, 40));
    }

    #[test]
    fn a_broken_jpeg_is_an_error() {
        let jpeg = include_bytes!("testdata/quarters.jpg");
        let mut out = Planes::default();
        let raw = Raw {
            format: PixelFormat::Mjpeg,
            width: 0,
            height: 0,
            planes: vec![(&jpeg[..40], 0)],
        };
        assert!(matches!(
            convert(&raw, &mut out),
            Err(FrameError::Jpeg(_)) | Err(FrameError::Short)
        ));
    }

    #[test]
    fn fourccs_name_the_formats() {
        for format in [
            PixelFormat::I420,
            PixelFormat::Nv12,
            PixelFormat::Yuyv,
            PixelFormat::Mjpeg,
        ] {
            assert_eq!(PixelFormat::of_fourcc(&format.fourcc()), Some(format));
        }
        assert_eq!(PixelFormat::of_fourcc(b"YUY2"), Some(PixelFormat::Yuyv));
        assert_eq!(PixelFormat::of_fourcc(b"RGB3"), None);
    }
}
