# Patched LiveKit crates

Copies of three crates from crates.io, at exactly the versions `Cargo.toml`
pins, each change marked `Tomoshibi:`. Upgrading means copying the new release
over the old one and carrying every change forward.

- `livekit-signaling` 0.1.6 — `validate()` never sends its request. After a
  failed WebSocket the SDK would make an HTTPS GET with the token as a bearer,
  which a silent relay in mainland China answers like a website, and that is
  what gets the relay's name blocked. The web client suppresses the same
  request in `web/src/live/novalidate.ts`.
- `livekit` 0.9.5 — a two-channel audio source is announced as stereo
  (`AddTrackRequest.stereo` and `TF_STEREO`), as the web client announces it.
  Without that the media server's answer carries no `stereo=1` and the encoder
  downmixes, so a shared screen's sound and the music reached browsers in mono.
- `livekit` 0.9.5 — the `x-google-start-bitrate` hint is written only on the
  m-sections that send video (`rtc_engine/peer_transport.rs`). In single PC
  mode the offer that publishes the first camera also holds every remote video
  received; the hint on those changed their receive codecs, libwebrtc rebuilt
  each receive stream, and a packet in the gap left one remote camera with no
  sink for the rest of the call, about one launch in four. `rtc_engine::
  munge_start_bitrate` exposes the rewrite to `core/tests/start_bitrate.rs`,
  since the vendored crate's own tests cannot run outside the workspace.
- `libwebrtc` 0.3.52 — a receiver's minimum playout delay is reachable
  (`RtpReceiver::set_jitter_buffer_minimum_delay`): the web holds every shared
  screen and its sound for 200 ms (`web/src/live/delay.ts`), and the binding
  beneath had it with nothing above it calling it.
- `libwebrtc` 0.3.52 — on Android, `NativeBuffer::from_android_buffer` takes
  over a frame buffer made in C++ from a Java one (`core/src/android_frame.cc`),
  which is how a shared screen reaches the hardware encoders as a texture.
  webrtc-sys can make a native buffer from a CVPixelBuffer or a DMA-BUF and
  from nothing an Android app holds; without this every frame was copied
  through memory four times on its way in.

All three are Apache-2.0, as each `Cargo.toml` declares; the published crates carry no separate licence file.
