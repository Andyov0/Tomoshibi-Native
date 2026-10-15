# Patched LiveKit crates

Copies of two crates from crates.io, at exactly the versions `Cargo.toml` pins,
each changed in one place marked `Tomoshibi:`. Upgrading means copying the new
release over the old one and carrying both changes forward.

- `livekit-signaling` 0.1.6 — `validate()` never sends its request. After a
  failed WebSocket the SDK would make an HTTPS GET with the token as a bearer,
  which a silent relay in mainland China answers like a website, and that is
  what gets the relay's name blocked. The web client suppresses the same
  request in `web/src/live/novalidate.ts`.
- `livekit` 0.9.5 — a two-channel audio source is announced as stereo
  (`AddTrackRequest.stereo` and `TF_STEREO`), as the web client announces it.
  Without that the media server's answer carries no `stereo=1` and the encoder
  downmixes, so a shared screen's sound and the music reached browsers in mono.

Both are Apache-2.0, as each `Cargo.toml` declares; the published crates carry no separate licence file.
