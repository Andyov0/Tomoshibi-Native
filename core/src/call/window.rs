//! Android: remote pictures drawn by the core straight into the app's
//! surfaces.
//!
//! On Apple the decoder's CVPixelBuffer is handed to the app and a display
//! layer shows it without a copy. Android's decoder (MediaCodec through
//! WebRTC's Java factories, set up without an EGL context in webrtc-sys
//! android.cpp) gives an I420 picture in memory instead, and handing that to
//! Kotlin would mean every frame of every picture crossing JNA as a byte
//! array, copied twice on the way: a 1080p frame is 3 MB, thirty times a
//! second, per picture. So the app hands the core an ANativeWindow made from
//! the Surface of its view (`CoreNative.windowFromSurface`), and the core
//! converts each frame into the window's buffer with libyuv and posts it; the
//! compositor scales it to the view. Turning the picture upright is left to
//! the view, which hears the picture's shape whenever it changes.

use std::{
    ffi::c_void,
    sync::{Arc, Mutex},
};

use livekit::webrtc::{
    native::yuv_helper,
    video_frame::{VideoBuffer, VideoRotation},
};

use super::remote::PictureShape;

#[repr(C)]
struct NativeWindowBuffer {
    width: i32,
    height: i32,
    stride: i32,
    format: i32,
    bits: *mut c_void,
    reserved: [u32; 6],
}

#[link(name = "android")]
extern "C" {
    fn ANativeWindow_release(window: *mut c_void);
    fn ANativeWindow_setBuffersGeometry(window: *mut c_void, width: i32, height: i32, format: i32) -> i32;
    fn ANativeWindow_lock(window: *mut c_void, buffer: *mut NativeWindowBuffer, dirty: *mut c_void) -> i32;
    fn ANativeWindow_unlockAndPost(window: *mut c_void) -> i32;
}

/// AHARDWAREBUFFER_FORMAT_R8G8B8A8_UNORM: bytes R, G, B, A, which is what
/// libyuv calls ABGR (it names a 32-bit word, least significant byte last).
const RGBA_8888: i32 = 1;

/// One surface, owning the reference `windowFromSurface` acquired.
pub(crate) struct Window {
    pointer: *mut c_void,
    shape: Arc<dyn PictureShape>,
    drawn: (i32, i32, u32),
}

// SAFETY: an ANativeWindow may be used from any thread; this one is only ever
// used under the `Windows` lock.
unsafe impl Send for Window {}

impl Window {
    /// Takes over a window the app acquired, releasing it when dropped.
    pub(crate) fn new(pointer: u64, shape: Arc<dyn PictureShape>) -> Window {
        Window { pointer: pointer as *mut c_void, shape, drawn: (0, 0, u32::MAX) }
    }

    fn draw(&mut self, y: &[u8], u: &[u8], v: &[u8], strides: (u32, u32, u32), width: i32, height: i32, rotation: u32) {
        if self.drawn != (width, height, rotation) {
            // SAFETY: the window is held for as long as this is alive.
            if unsafe { ANativeWindow_setBuffersGeometry(self.pointer, width, height, RGBA_8888) } != 0 {
                return;
            }
            self.drawn = (width, height, rotation);
            self.shape.changed(width as u32, height as u32, rotation);
        }
        let mut buffer = NativeWindowBuffer {
            width: 0,
            height: 0,
            stride: 0,
            format: 0,
            bits: std::ptr::null_mut(),
            reserved: [0; 6],
        };
        // A surface that has gone (the view was taken off the screen before
        // it detached) refuses the lock; nothing is drawn and nothing breaks,
        // since the window itself is still held.
        if unsafe { ANativeWindow_lock(self.pointer, &mut buffer, std::ptr::null_mut()) } != 0 {
            return;
        }
        if buffer.width == width && buffer.height == height && !buffer.bits.is_null() {
            let stride = buffer.stride as u32 * 4;
            // SAFETY: a locked RGBA_8888 buffer is `stride` pixels by `height`
            // rows of four bytes each.
            let destination = unsafe { std::slice::from_raw_parts_mut(buffer.bits as *mut u8, (stride * height as u32) as usize) };
            yuv_helper::i420_to_abgr(y, strides.0, u, strides.1, v, strides.2, destination, stride, width, height);
        }
        unsafe { ANativeWindow_unlockAndPost(self.pointer) };
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        // SAFETY: the reference acquired by windowFromSurface, released once.
        unsafe { ANativeWindow_release(self.pointer) };
    }
}

/// Every surface one picture is drawn into.
#[derive(Default)]
pub(crate) struct Windows {
    windows: Mutex<Vec<Window>>,
}

impl Windows {
    pub(crate) fn add(&self, window: Window) {
        let mut windows = self.windows.lock().unwrap();
        // The same window attached again replaces itself rather than being
        // drawn twice and released twice.
        windows.retain(|w| w.pointer != window.pointer);
        windows.push(window);
    }

    /// Whether that was the last.
    pub(crate) fn remove(&self, pointer: u64) -> bool {
        let mut windows = self.windows.lock().unwrap();
        windows.retain(|w| w.pointer as u64 != pointer);
        windows.is_empty()
    }

    pub(crate) fn draw(&self, buffer: &Box<dyn VideoBuffer>, rotation: VideoRotation) {
        let rotation = match rotation {
            VideoRotation::VideoRotation0 => 0,
            VideoRotation::VideoRotation90 => 90,
            VideoRotation::VideoRotation180 => 180,
            VideoRotation::VideoRotation270 => 270,
        };
        let i420 = buffer.to_i420();
        let (y, u, v) = i420.data();
        let strides = i420.strides();
        let (width, height) = (buffer.width() as i32, buffer.height() as i32);
        for window in self.windows.lock().unwrap().iter_mut() {
            window.draw(y, u, v, strides, width, height, rotation);
        }
    }
}
