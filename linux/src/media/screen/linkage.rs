//! PipeWire made safe to call in a binary that also links WebRTC.
//!
//! The prebuilt WebRTC the core links carries its own way of reaching
//! PipeWire, for its desktop and camera capturers: stubs for 37 PipeWire
//! functions (pw_init, pw_stream_new, pw_stream_connect,
//! pw_context_connect_fd, pw_thread_loop_new and the rest), compiled as weak
//! definitions under the real names in its `pipewire_stubs.o`, each a jump
//! through a pointer that WebRTC's loader fills from the library it opens.
//! webrtc-sys's desktop-capturer bridge pulls that object into the link
//! whenever the code-generation unit holding the bridge is linked, which
//! depends on nothing more than how rustc partitioned the crate; and once
//! it is in, its definitions win over libpipewire's, so every call
//! pipewire-rs makes to one of the 37 lands in a stub. Nothing in this app
//! runs WebRTC's capturers, so nobody fills the pointers, and the first call
//! (pw_init) jumped to address zero: sharing the screen crashed the app the
//! moment the stream was connected. Naming libpipewire on the link line did
//! not help, since a definition in the executable beats one in a shared
//! library whatever the order.
//!
//! So the stubs are filled: libpipewire is opened (it is already loaded, a
//! dependency of the binary, so this only hands back its handle) and
//! WebRTC's own loader, `modules_portal::InitializePipewire(void*)`, looks
//! each of the 37 up in it, after which a stub is a jump into the library.
//! Calls to functions WebRTC has no stub for go to the library directly.
//! Referring to the loader also makes the link take the stubs' object every
//! time, rather than when the partitioning happens to, so the binary is the
//! same shape in every build.
//!
//! Every module of the app that uses PipeWire calls [`init_pipewire`] before
//! anything else of PipeWire, in place of `pipewire::init()`: the camera and
//! the sound as well as the screen. Check a binary with
//! `nm <binary> | grep -c ' t pw_'`: a non-zero count is the stubs linked
//! in, which is fine once this has run and fatal before.

use std::{ffi::c_void, sync::OnceLock};

use crate::app::log;

unsafe extern "C" {
    /// WebRTC's loader for its PipeWire stubs (webrtc's generated
    /// pipewire_stubs.cc, namespace `modules_portal`): fills every stub's
    /// pointer from `module`, a dlopen handle; false where a function was
    /// not found in it. The same symbol in the x86_64 and aarch64 prebuilts.
    #[link_name = "_ZN14modules_portal18InitializePipewireEPv"]
    fn webrtc_initialize_pipewire(module: *mut c_void) -> bool;
}

/// Makes PipeWire callable, once per process; false where there is no
/// libpipewire to open. Call before any other PipeWire function, from any
/// thread.
pub fn init_pipewire() -> bool {
    static READY: OnceLock<bool> = OnceLock::new();
    *READY.get_or_init(|| {
        // SAFETY: a constant, NUL-terminated name; the handle is kept for
        // the life of the process and never closed.
        let module = unsafe {
            libc::dlopen(
                c"libpipewire-0.3.so.0".as_ptr(),
                libc::RTLD_NOW | libc::RTLD_LOCAL,
            )
        };
        if module.is_null() {
            log("no libpipewire-0.3 to open");
            return false;
        }
        // SAFETY: WebRTC's loader only looks names up in the handle given
        // and stores the addresses in its own pointers.
        if !unsafe { webrtc_initialize_pipewire(module) } {
            // A function WebRTC knows that this libpipewire lacks (one newer
            // than the system's): its stub stays empty, and nothing here
            // calls it, since the stubs are of what WebRTC calls.
            log("libpipewire lacks a function WebRTC's stubs name");
        }
        pipewire::init();
        true
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stubs, once filled, reach the library: pw_init through its stub
    /// and the library's version through a call WebRTC does not stub.
    /// Without `init_pipewire` the first PipeWire call of this test
    /// binary would jump to address zero.
    #[test]
    fn pipewire_can_be_called_once_the_stubs_are_filled() {
        assert!(init_pipewire());
        assert!(init_pipewire());
        let version = unsafe { std::ffi::CStr::from_ptr(pipewire::sys::pw_get_library_version()) };
        assert!(
            version.to_string_lossy().starts_with(['0', '1']),
            "{version:?}"
        );
        let main_loop = pipewire::main_loop::MainLoopRc::new(None);
        assert!(main_loop.is_ok());
    }
}
