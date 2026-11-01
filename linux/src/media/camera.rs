//! The camera, through the xdg-desktop-portal's Camera interface and
//! PipeWire, with V4L2 directly where there is no portal.
//!
//! What this has to do (core-map section 3): open the chosen camera at
//! 1280x720 and 30 fps as Windows does (Devices.cs), push each frame into
//! the camera's `VideoInput` with `push_planes` (NV12 as y plus an
//! interleaved uv plane with a chroma step of 2, or I420 with a step of 1),
//! hand the same frames to every preview sink as `RemoteFrame`s with `i420`
//! filled, and blur the background behind a switch with the MediaPipe
//! selfie model (windows/Blur/selfie_segmenter.onnx; the math shared in
//! Tomoshibi.Basics/BlurMath.cs).
//!
//! Owned by the media module. Nothing here yet: `media::standard()` is
//! `Quiet` until this implements `Camera`.
