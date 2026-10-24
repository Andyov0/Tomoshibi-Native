// A Java frame buffer (livekit.org.webrtc.VideoFrame.Buffer) as a buffer the
// core can send: the shared screen's frames, which reach the core as GPU
// textures rather than bytes (android/app ScreenCapture), go to the encoders
// as they are, and the hardware encoders draw them into their own input
// surfaces without anything of the picture passing through the processor.
//
// Only C++ can make one. WebRTC's JavaToNativeFrameBuffer wraps the Java
// buffer in an AndroidVideoBuffer, which holds a reference to it for as long
// as any part of WebRTC holds the frame and releases it after, which is when
// the capture may draw the next picture into the same texture. That is
// wrapped in turn in webrtc-sys's livekit_ffi::VideoFrameBuffer, the type its
// Rust side holds through a cxx UniquePtr (vendor/libwebrtc NativeBuffer
// from_android_buffer).
//
// webrtc-sys does not publish its headers to the crates that use it, so the
// one declaration needed here is repeated from its include/livekit/
// video_frame_buffer.h: a class with no virtual functions whose only member is
// the scoped_refptr, and its constructor, which webrtc-sys defines and links.
// It holds for the webrtc-sys Cargo.lock pins (0.3.49); after an upgrade,
// compare it with that header again. A layout that no longer matched would
// fail on the first frame shared, which the Rust side reads the size of
// before sending it (android.rs pushFrame), and not somewhere quieter.

#include <jni.h>

#include "api/scoped_refptr.h"
#include "api/video/video_frame_buffer.h"
#include "sdk/android/native_api/jni/scoped_java_ref.h"
#include "sdk/android/src/jni/video_frame.h"

namespace livekit_ffi {
class VideoFrameBuffer {
 public:
  explicit VideoFrameBuffer(webrtc::scoped_refptr<webrtc::VideoFrameBuffer> buffer);

 protected:
  webrtc::scoped_refptr<webrtc::VideoFrameBuffer> buffer_;
};
}  // namespace livekit_ffi

// An owned livekit_ffi::VideoFrameBuffer for the Java buffer, or null; the
// Java buffer is retained by it and released when WebRTC lets the frame go.
extern "C" void* tomoshibi_frame_buffer_from_java(JNIEnv* env, jobject buffer) {
  if (env == nullptr || buffer == nullptr) return nullptr;
  webrtc::scoped_refptr<webrtc::VideoFrameBuffer> native =
      webrtc::jni::JavaToNativeFrameBuffer(env, jni_zero::CreateLeaky(env, buffer));
  if (!native) return nullptr;
  return new livekit_ffi::VideoFrameBuffer(std::move(native));
}
