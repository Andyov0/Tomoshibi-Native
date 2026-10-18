//! The few calls an Android app makes through JNI rather than through the
//! UniFFI bindings, because they carry Java objects, which JNA cannot.
//!
//! Kotlin declares them on `app.tomoshibi.core.CoreNative`, and loads the
//! library with `System.loadLibrary` before the bindings' JNA does: JNA opens
//! it with dlopen, which never gives the JVM's class loader to WebRTC, and
//! `initialize` must run on a thread that has it, or WebRTC's Java codec
//! factories (`livekit.org.webrtc.*`, from libwebrtc.jar) are not found and
//! no picture is encoded or decoded at all.

use std::ffi::c_void;

use jni::{
    objects::{JClass, JObject},
    sys::{jboolean, jlong, JNI_FALSE, JNI_TRUE},
    JNIEnv,
};

#[link(name = "android")]
extern "C" {
    fn ANativeWindow_fromSurface(env: *mut jni::sys::JNIEnv, surface: jni::sys::jobject) -> *mut c_void;
    fn ANativeWindow_release(window: *mut c_void);
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
