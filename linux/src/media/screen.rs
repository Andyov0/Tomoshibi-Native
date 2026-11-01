//! Sharing the screen, through the xdg-desktop-portal's ScreenCast and
//! PipeWire.
//!
//! What this has to do (core-map section 5): ask the portal for a monitor or
//! a window, publish `VideoKind::Screen { fps, quality }` at the stream's
//! size, push each frame with `push_bgra` (the core converts and scales with
//! libyuv), send the last frame again every 500 ms while the screen is still
//! as Windows does (ScreenCapture.cs), since an encoder given nothing new
//! lets a still screen go soft; and, when asked, publish the screen's sound
//! (`SoundKind::ScreenSound`, 48 kHz stereo) from the default output's
//! monitor without this app's own playback where PipeWire allows it.
//!
//! Owned by the media module. Nothing here yet: `media::standard()` is
//! `Quiet` until this implements `ScreenShare`.
