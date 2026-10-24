//! The core's own C++, which is Android's alone: src/android_frame.cc, the
//! one step between a Java frame buffer and the core that only C++ can take
//! (it says what and why). Every other target builds nothing here.
//!
//! It is compiled against the prebuilt WebRTC webrtc-sys links, with the
//! defines that WebRTC was built with (read from its webrtc.ninja, as
//! webrtc-sys reads them for its own C++), so that every type it shares with
//! the library is laid out as the library lays it out.

use std::env;

fn main() {
    println!("cargo:rerun-if-changed=src/android_frame.cc");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("android") {
        return;
    }
    // Takes the download lock and returns at once where webrtc-sys's own
    // build script has already fetched it.
    webrtc_sys_build::download_webrtc().expect("the prebuilt WebRTC could not be fetched");
    let webrtc = webrtc_sys_build::webrtc_dir().join("include");
    let toolchain = webrtc_sys_build::android_ndk_toolchain().expect("no Android NDK");
    let sysroot = toolchain.join("sysroot").canonicalize().expect("the NDK has no sysroot");

    let mut build = cc::Build::new();
    build
        .cpp(true)
        // The C++ library is webrtc-sys's, linked statically (c++_static);
        // cc's default for Android, c++_shared, made the core ask for a
        // libc++_shared.so the app does not carry, and it would not load.
        .cpp_link_stdlib(None)
        .file("src/android_frame.cc")
        .include(&webrtc)
        .include(webrtc.join("third_party/abseil-cpp"))
        .include(webrtc.join("third_party/libyuv/include"))
        .flag("-std=c++20")
        .flag(format!("-isysroot{}", sysroot.display()).as_str())
        .warnings(false);
    for (name, value) in webrtc_sys_build::webrtc_defines() {
        build.define(name.as_str(), value.as_deref());
    }
    build.compile("tomoshibi-android-frame");
}
