# Tomoshibi Native

Native clients for [Tomoshibi](https://github.com/5t-RawBeRry/Tomoshibi), the
video meeting server in one binary. The web client that server ships is the
reference: a native participant has to be indistinguishable from a browser to
everybody else in the room, and every rule about what is published, what it is
named and what goes over the data channel comes from there.

What a native client is for is everything a browser cannot do: share the whole
screen of an iPhone, stay in a call with the screen locked, share any window of
a Mac together with its sound, send lossless audio from any machine, and open
straight from an invitation.

## Layout

- `core/` — the media and protocol core, in Rust, on LiveKit's Rust SDK. One
  core for every platform, so the protocol is written once. The apps reach it
  through generated bindings.
- `apple/` — macOS and iOS, in Swift (to come).
- `interop/` — scripts that put a browser in a room with the core and report
  what it actually received. The unit tests cannot hear a mono track that was
  supposed to be stereo; these can.
- `docs/PLAN.md` — the plan and the record of what has been verified.

## Building the core

```bash
cd core
cargo build --example probe
```

The first build downloads LiveKit's prebuilt WebRTC. `core/.cargo/config.toml`
carries two things every Apple build needs, and why.

Three LiveKit crates are patched; `core/vendor/README.md` says what and why.

## Talking to a deployment

```bash
./target/debug/examples/probe http://127.0.0.1:8080 some-room 60
```

joins a room as a participant with a synthetic voice, screen sound, music and
camera, mints an invitation and prints it. `interop/web-peer.mjs` then follows
it in with a headless browser (Chrome for Testing, with `--mute-audio`) and
reports what arrived.

## Licence

AGPL-3.0-or-later, as Tomoshibi is: the apps carry the web client's phrases.
