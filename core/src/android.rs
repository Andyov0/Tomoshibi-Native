//! The few calls an Android app makes through JNI rather than through the
//! UniFFI bindings, because they carry Java objects, which JNA cannot.
//!
//! Kotlin declares them on `app.tomoshibi.core.CoreNative`, and loads the
//! library with `System.loadLibrary` before the bindings' JNA does: JNA opens
//! it with dlopen, which never gives the JVM's class loader to WebRTC, and
//! `initialize` must run on a thread that has it, or WebRTC's Java codec
//! factories (`livekit.org.webrtc.*`, from libwebrtc.jar) are not found and
//! no picture is encoded or decoded at all.

use std::{ffi::c_void, sync::Arc};

use jni::{
    objects::{JClass, JObject},
    sys::{jboolean, jint, jlong, JNI_FALSE, JNI_TRUE},
    JNIEnv,
};
use livekit::webrtc::video_frame::{native::NativeBuffer, VideoBuffer};

use crate::call::VideoInput;

#[link(name = "android")]
extern "C" {
    fn ANativeWindow_fromSurface(env: *mut jni::sys::JNIEnv, surface: jni::sys::jobject) -> *mut c_void;
    fn ANativeWindow_release(window: *mut c_void);
}

// src/android_frame.cc, compiled by build.rs.
extern "C" {
    fn tomoshibi_frame_buffer_from_java(env: *mut jni::sys::JNIEnv, buffer: jni::sys::jobject) -> *mut c_void;
}

/// Hands WebRTC the JVM and the application's context, once, before the
/// first call is joined.
#[no_mangle]
pub extern "system" fn Java_app_tomoshibi_core_CoreNative_initialize(env: JNIEnv, _: JClass, context: JObject) -> jboolean {
    let Ok(vm) = env.get_java_vm() else { return JNI_FALSE };
    if livekit::webrtc::android::initialize_android_context(&vm, &context) {
        JNI_TRUE
    } else {
        JNI_FALSE
    }
}

/// The ANativeWindow behind a Surface, acquired once for the core: passed to
/// `Call.attachVideoWindow`, which takes the reference over, or else given back
/// with `releaseWindow`. Zero when the surface has no window.
#[no_mangle]
pub extern "system" fn Java_app_tomoshibi_core_CoreNative_windowFromSurface(env: JNIEnv, _: JClass, surface: JObject) -> jlong {
    // SAFETY: a live JNIEnv for this thread and a Surface the caller holds.
    unsafe { ANativeWindow_fromSurface(env.get_raw(), surface.as_raw()) as jlong }
}

#[no_mangle]
pub extern "system" fn Java_app_tomoshibi_core_CoreNative_releaseWindow(_: JNIEnv, _: JClass, window: jlong) {
    if window != 0 {
        // SAFETY: a reference from windowFromSurface that nothing took over.
        unsafe { ANativeWindow_release(window as *mut c_void) };
    }
}

/// One frame for the input behind `input` (`VideoInput.frameHandle`):
/// `buffer` is a livekit.org.webrtc.VideoFrame.Buffer, in practice the shared
/// screen's texture, `width` by `height`, which WebRTC holds by reference
/// until every encoder has drawn it and then releases. False where no native
/// buffer could be made of it, or one of another size, for the caller to send
/// the frame some other way: a shared screen whose frames silently went
/// nowhere would be a black tile nobody is told about.
#[no_mangle]
pub extern "system" fn Java_app_tomoshibi_core_CoreNative_pushFrame(
    env: JNIEnv,
    _: JClass,
    input: jlong,
    buffer: JObject,
    width: jint,
    height: jint,
    timestamp_us: jlong,
) -> jboolean {
    if input == 0 || buffer.as_raw().is_null() {
        return JNI_FALSE;
    }
    // SAFETY: a handle from frame_handle that has not been given back, which
    // holds the input alive.
    let input = unsafe { &*(input as *const VideoInput) };
    // SAFETY: a live JNIEnv for this thread and a buffer the caller holds.
    let raw = unsafe { tomoshibi_frame_buffer_from_java(env.get_raw(), buffer.as_raw()) };
    // SAFETY: null or an owned buffer from android_frame.cc, taken over once.
    let Some(native) = (unsafe { NativeBuffer::from_android_buffer(raw) }) else { return JNI_FALSE };
    if native.width() != width as u32 || native.height() != height as u32 {
        return JNI_FALSE;
    }
    input.push_native(native, timestamp_us);
    JNI_TRUE
}

/// Gives back a handle from `VideoInput.frameHandle`.
#[no_mangle]
pub extern "system" fn Java_app_tomoshibi_core_CoreNative_releaseFrameInput(_: JNIEnv, _: JClass, input: jlong) {
    if input != 0 {
        // SAFETY: a handle from frame_handle, given back once.
        unsafe { drop(Arc::from_raw(input as *const VideoInput)) };
    }
}
