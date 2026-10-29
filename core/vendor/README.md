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

- `livekit` 0.9.5 — a full reconnect publishes every local track again
  (`room/mod.rs` `handle_restarted`) and skips one that was unpublished after
  its list was taken, where it used to unwrap the missing track: the task
  panicked, `Reconnected` never came and the call said it was reconnecting
  for good. Taking a track back during a reconnect is exactly what the core's
  `withdraw` (call/publish.rs) allows.

- `livekit` 0.9.5 — the reconnect schedule is the web's
  (`rtc_engine/reconnect_strategy.rs`): livekit-client's
  `DefaultReconnectPolicy` as the deployment's web client is built with it,
  ten attempts after waits of 0, 0.3, 1.2, 2.7, 4.8 and five times 7 s, with
  up to a second more at random from the third on, the wait taken before each
  attempt and none after the last. The SDK's own (full-jitter backoff capped
  at 7 s, waited after every attempt) gave up in about 22 s, and a browser on
  the same network lasted twice that. `rtc_engine::reconnect_wait` and the
  constants are public for `core/tests/reconnect_schedule.rs`.
- `livekit` 0.9.5 — `Room::retry_now` and `Room::network_changed`, reached
  from the apps through `Call::retry_now` and `Call::network_changed`: while
  reconnecting, the attempt waiting its turn is made now (the engine's
  `retry_now_notify`, which until now only the server could ring); while
  connected, a network change starts a resume at once, the path a server's
  `Leave{Resume}` takes, as the web resumes when the browser comes back online.
  A permit left on that notify by an earlier episode is cleared at the start of
  each, so it cannot cut a later wait short.

All three are Apache-2.0, as each `Cargo.toml` declares; the published crates carry no separate licence file.
