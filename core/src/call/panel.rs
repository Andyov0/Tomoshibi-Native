//! Windows: remote pictures drawn by the core into swap chains that the app
//! shows in SwapChainPanels.
//!
//! The Windows decoder (WebRTC's software H.264, built into the prebuilt
//! library) gives an I420 picture in memory, as Android's does. Handing that
//! to C# would mean every frame of every picture crossing the binding as a
//! byte array, copied on the way out of Rust and again into a bitmap the
//! XAML compositor then uploads: a 1080p frame is 3 MB, thirty times a second,
//! per picture, and the conversion to BGRA would be C#'s to write. So this is
//! Android's answer (window.rs) in Direct3D: the core makes a composition
//! swap chain on its own device, the app sets it on a SwapChainPanel
//! (`ISwapChainPanelNative::SetSwapChain`, on the UI thread), and each frame
//! is converted with libyuv straight into the swap chain's back buffer and
//! presented. The compositor scales it to the panel by the swap chain's
//! matrix transform.
//!
//! Unlike a TextureView, a SwapChainPanel cannot be turned or cropped by its
//! layout, so the turn a phone's camera asks for is made here, and so is the
//! crop when the picture fills its panel: the app says where the picture is
//! to sit (`place`), in the panel's device-independent pixels.

use std::sync::{Arc, Mutex, OnceLock};

use livekit::webrtc::{
    native::yuv_helper,
    video_frame::{VideoBuffer, VideoRotation},
};
use windows::{
    core::Interface,
    Win32::{
        Foundation::HMODULE,
        Graphics::{
            Direct3D::{D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP},
            Direct3D11::{
                D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                D3D11_SDK_VERSION,
            },
            Dxgi::{
                Common::{DXGI_ALPHA_MODE_IGNORE, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
                IDXGIDevice, IDXGIFactory2, IDXGISwapChain1, IDXGISwapChain2, DXGI_MATRIX_3X2_F,
                DXGI_PRESENT_DO_NOT_WAIT, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG,
                DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL, DXGI_USAGE_RENDER_TARGET_OUTPUT,
            },
        },
    },
};

use super::remote::PictureShape;

/// Two buffers: one being shown while the next is written. A flip-model
/// swap chain, which is the only kind a composition swap chain can be.
const BUFFERS: u32 = 2;

/// The process's one Direct3D device. Its immediate context may be used by
/// one thread at a time, and every picture's frames arrive on a thread of
/// their own, so the context is only reached through this lock.
struct Gpu {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    factory: IDXGIFactory2,
}

fn gpu() -> Option<&'static Mutex<Gpu>> {
    static GPU: OnceLock<Option<Mutex<Gpu>>> = OnceLock::new();
    GPU.get_or_init(|| {
        // The hardware first; WARP (Direct3D on the processor) where there is
        // none, as in a virtual machine without a display driver.
        [D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP].into_iter().find_map(|driver| make_gpu(driver).map(Mutex::new))
    })
    .as_ref()
}

fn make_gpu(driver: D3D_DRIVER_TYPE) -> Option<Gpu> {
    let mut device = None;
    let mut context = None;
    // SAFETY: plain out-parameters for a device made with no adapter.
    unsafe {
        D3D11CreateDevice(
            None,
            driver,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )
        .ok()?;
    }
    let (device, context) = (device?, context?);
    // SAFETY: COM calls on the device just made.
    let factory = unsafe { device.cast::<IDXGIDevice>().ok()?.GetAdapter().ok()?.GetParent::<IDXGIFactory2>().ok()? };
    Some(Gpu { device, context, factory })
}

/// Where the app has put the picture: its panel's size in device-independent
/// pixels, and whether the picture fills it (cut to its shape) or fits
/// inside it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Place {
    width: f32,
    height: f32,
    fill: bool,
}

/// One panel's swap chain.
pub(crate) struct Panel {
    chain: IDXGISwapChain1,
    shape: Arc<dyn PictureShape>,
    place: Option<Place>,
    /// The swap chain's buffers' size, which is the part of the picture shown.
    buffers: (u32, u32),
    transform: Option<[f32; 3]>,
    told: (u32, u32, u32),
    frames: u64,
}

impl Panel {
    /// A swap chain for a panel the app is about to show, or None when no
    /// Direct3D device could be made at all.
    pub(crate) fn new(shape: Arc<dyn PictureShape>) -> Option<Panel> {
        let gpu = gpu()?.lock().unwrap();
        // The size is a placeholder until the first frame says what it is;
        // a composition swap chain cannot be made empty.
        let description = DXGI_SWAP_CHAIN_DESC1 {
            Width: 16,
            Height: 16,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            Stereo: false.into(),
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: BUFFERS,
            Scaling: DXGI_SCALING_STRETCH,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            AlphaMode: DXGI_ALPHA_MODE_IGNORE,
            Flags: 0,
        };
        // SAFETY: a description that lives across the call, on the device.
        let chain = unsafe { gpu.factory.CreateSwapChainForComposition(&gpu.device, &description, None).ok()? };
        Some(Panel { chain, shape, place: None, buffers: (16, 16), transform: None, told: (0, 0, u32::MAX), frames: 0 })
    }

    /// What identifies the panel to the app: the swap chain's address.
    pub(crate) fn id(&self) -> u64 {
        self.chain.as_raw() as u64
    }

    /// A reference to the swap chain that is the app's, for
    /// `ISwapChainPanelNative::SetSwapChain`; the app releases it after.
    pub(crate) fn hand_over(&self) -> u64 {
        self.chain.clone().into_raw() as u64
    }

    /// Draws one upright BGRA picture of `width` by `height`, as decoded at
    /// `decoded` and turned by `rotation` degrees. Returns who is to hear
    /// that the picture's shape changed, to be told once every lock is let
    /// go: the app's answer may well be to place the panel again.
    fn draw(
        &mut self,
        gpu: &Gpu,
        pixels: &[u8],
        width: u32,
        height: u32,
        decoded: (u32, u32),
        rotation: u32,
    ) -> Option<Arc<dyn PictureShape>> {
        let changed = (self.told != (decoded.0, decoded.1, rotation)).then(|| self.shape.clone());
        self.told = (decoded.0, decoded.1, rotation);
        let (x, y, shown_w, shown_h) = crop(width, height, self.place);
        if self.buffers != (shown_w, shown_h) {
            // SAFETY: no buffer of the chain is held outside a draw.
            if unsafe { self.chain.ResizeBuffers(BUFFERS, shown_w, shown_h, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SWAP_CHAIN_FLAG(0)) }
                .is_err()
            {
                return changed;
            }
            self.buffers = (shown_w, shown_h);
        }
        let transform = transform(shown_w, shown_h, self.place);
        if self.transform != Some(transform) {
            let [scale, dx, dy] = transform;
            let matrix = DXGI_MATRIX_3X2_F { _11: scale, _12: 0.0, _21: 0.0, _22: scale, _31: dx, _32: dy };
            // SAFETY: a composition swap chain is an IDXGISwapChain2 from
            // Windows 8.1 on; the matrix lives across the call.
            if let Ok(chain) = self.chain.cast::<IDXGISwapChain2>() {
                if unsafe { chain.SetMatrixTransform(&matrix) }.is_ok() {
                    self.transform = Some(transform);
                }
            }
        }
        // SAFETY: the back buffer is a B8G8R8A8 texture of `shown_w` by
        // `shown_h`, and the source rows start at the crop's corner of a
        // picture `width` pixels wide with `shown_h` rows below it.
        unsafe {
            let Ok(back) = self.chain.GetBuffer::<ID3D11Texture2D>(0) else { return changed };
            let start = pixels.as_ptr().add(((y * width + x) * 4) as usize);
            gpu.context.UpdateSubresource(&back, 0, None, start.cast(), width * 4, 0);
            drop(back);
            // Not waiting for the display: a frame the compositor has not
            // taken yet is dropped rather than holding the decoder's thread.
            let _ = self.chain.Present(0, DXGI_PRESENT_DO_NOT_WAIT);
        }
        self.frames += 1;
        changed
    }
}

/// The part of an upright picture shown in a panel, as its corner and size:
/// all of it when it fits, or the middle of it cut to the panel's shape when
/// it fills.
fn crop(width: u32, height: u32, place: Option<Place>) -> (u32, u32, u32, u32) {
    let Some(place) = place.filter(|p| p.fill && p.width > 0.0 && p.height > 0.0) else {
        return (0, 0, width, height);
    };
    let panel = place.width / place.height;
    if width as f32 / height as f32 > panel {
        let shown = ((height as f32 * panel).round() as u32).clamp(1, width);
        ((width - shown) / 2, 0, shown, height)
    } else {
        let shown = ((width as f32 / panel).round() as u32).clamp(1, height);
        (0, (height - shown) / 2, width, shown)
    }
}

/// The scale and offset that put `width` by `height` buffer pixels in the
/// middle of the panel, as large as fits. A SwapChainPanel shows one buffer
/// pixel as one device-independent pixel until it is told otherwise.
fn transform(width: u32, height: u32, place: Option<Place>) -> [f32; 3] {
    let Some(place) = place.filter(|p| p.width > 0.0 && p.height > 0.0) else { return [1.0, 0.0, 0.0] };
    let scale = (place.width / width as f32).min(place.height / height as f32);
    [scale, (place.width - width as f32 * scale) / 2.0, (place.height - height as f32 * scale) / 2.0]
}

/// Every panel one picture is drawn into, and the picture converted once
/// for all of them.
#[derive(Default)]
pub(crate) struct Panels {
    panels: Mutex<Vec<Panel>>,
    scratch: Mutex<(Vec<u8>, Vec<u8>)>,
}

impl Panels {
    pub(crate) fn add(&self, panel: Panel) {
        let mut panels = self.panels.lock().unwrap();
        panels.retain(|p| p.id() != panel.id());
        panels.push(panel);
    }

    /// Whether that was the last.
    pub(crate) fn remove(&self, id: u64) -> bool {
        let mut panels = self.panels.lock().unwrap();
        panels.retain(|p| p.id() != id);
        panels.is_empty()
    }

    pub(crate) fn place(&self, id: u64, width: f32, height: f32, fill: bool) {
        if let Some(panel) = self.panels.lock().unwrap().iter_mut().find(|p| p.id() == id) {
            panel.place = Some(Place { width, height, fill });
        }
    }

    pub(crate) fn frames(&self, id: u64) -> u64 {
        self.panels.lock().unwrap().iter().find(|p| p.id() == id).map_or(0, |p| p.frames)
    }

    pub(crate) fn draw(&self, buffer: &Box<dyn VideoBuffer>, rotation: VideoRotation) {
        let Some(gpu) = gpu() else { return };
        let rotation = match rotation {
            VideoRotation::VideoRotation0 => 0,
            VideoRotation::VideoRotation90 => 90,
            VideoRotation::VideoRotation180 => 180,
            VideoRotation::VideoRotation270 => 270,
        };
        let i420 = buffer.to_i420();
        let (y, u, v) = i420.data();
        let (sy, su, sv) = i420.strides();
        let (width, height) = (buffer.width(), buffer.height());
        if width == 0 || height == 0 {
            return;
        }
        let mut scratch = self.scratch.lock().unwrap();
        let (decoded, turned) = &mut *scratch;
        decoded.resize((width * height * 4) as usize, 0);
        // libyuv's ARGB is a 32-bit word, so its bytes are B, G, R, A in
        // memory: DXGI's B8G8R8A8.
        yuv_helper::i420_to_argb(y, sy, u, su, v, sv, decoded, width * 4, width as i32, height as i32);
        let (pixels, shown_w, shown_h) = match rotation {
            0 => (&decoded[..], width, height),
            _ => {
                turn(decoded, turned, width, height, rotation);
                let (w, h) = if rotation == 180 { (width, height) } else { (height, width) };
                (&turned[..], w, h)
            }
        };
        let changed: Vec<_> = {
            let gpu = gpu.lock().unwrap();
            let mut panels = self.panels.lock().unwrap();
            panels.iter_mut().filter_map(|p| p.draw(&gpu, pixels, shown_w, shown_h, (width, height), rotation)).collect()
        };
        drop(scratch);
        for shape in changed {
            shape.changed(width, height, rotation);
        }
    }
}

/// Turns a BGRA picture clockwise by `rotation` degrees into `out`.
fn turn(source: &[u8], out: &mut Vec<u8>, width: u32, height: u32, rotation: u32) {
    let (w, h) = (width as usize, height as usize);
    out.resize(w * h * 4, 0);
    let pixel = |x: usize, y: usize| &source[(y * w + x) * 4..(y * w + x) * 4 + 4];
    for y in 0..h {
        for x in 0..w {
            // Where the pixel at (x, y) lands, and the row length there.
            let (tx, ty, row) = match rotation {
                90 => (h - 1 - y, x, h),
                180 => (w - 1 - x, h - 1 - y, w),
                _ => (y, w - 1 - x, h),
            };
            out[(ty * row + tx) * 4..(ty * row + tx) * 4 + 4].copy_from_slice(pixel(x, y));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_picture_filling_a_square_loses_its_sides() {
        let place = Some(Place { width: 100.0, height: 100.0, fill: true });
        assert_eq!(crop(1280, 720, place), (280, 0, 720, 720));
        let [scale, dx, dy] = transform(720, 720, place);
        assert_eq!(scale, 100.0 / 720.0);
        assert!(dx.abs() < 1e-4 && dy.abs() < 1e-4, "offset {dx}, {dy}");
    }

    #[test]
    fn a_picture_the_panel_has_not_placed_is_shown_whole_and_unscaled() {
        assert_eq!(crop(1280, 720, None), (0, 0, 1280, 720));
        assert_eq!(transform(1280, 720, None), [1.0, 0.0, 0.0]);
    }

    #[test]
    fn a_wide_picture_fitting_a_square_is_letterboxed() {
        let place = Some(Place { width: 100.0, height: 100.0, fill: false });
        assert_eq!(crop(1280, 720, place), (0, 0, 1280, 720));
        let [scale, dx, dy] = transform(1280, 720, place);
        assert_eq!((scale, dx), (100.0 / 1280.0, 0.0));
        assert!((dy - (100.0 - 720.0 * scale) / 2.0).abs() < 1e-4);
    }

    #[test]
    fn turning_a_quarter_puts_the_left_column_on_top() {
        // Two by one: a red pixel on the left, a blue one on the right.
        let source = [0, 0, 255, 255, 255, 0, 0, 255];
        let mut out = Vec::new();
        turn(&source, &mut out, 2, 1, 90);
        // Turned clockwise the picture is one by two, red above blue.
        assert_eq!(out, vec![0, 0, 255, 255, 255, 0, 0, 255]);
        turn(&source, &mut out, 2, 1, 270);
        assert_eq!(out, vec![255, 0, 0, 255, 0, 0, 255, 255]);
    }
}
