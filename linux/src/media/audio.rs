//! The microphone and the speakers, through PulseAudio, which PipeWire
//! serves as well (pipewire-pulse), so one client covers both kinds of
//! system.
//!
//! What this has to do (core-map section 2): capture the microphone at
//! 48 kHz mono and push it into the voice's `AudioInput` in any chunk size
//! (the core re-cuts it into 10 ms frames); play every remote sound the room
//! hands to `Media::play` through one mixer fed by `Call::attach_pcm` (10 ms
//! frames at 48 kHz, one channel for a voice, two for screen sound and
//! music) and by the lossless side (`Call::set_lossless_sink`); cancel the
//! echo of that playback in the capture, since Linux has no system canceller
//! to lean on, with WebRTC's audio processing fed the playback as its
//! reverse stream; and list the sources and sinks for the devices panel.
//!
//! Owned by the media module. Nothing here yet: `media::standard()` is
//! `Quiet` until this implements `Microphone` and the playback half of
//! `Media`.
