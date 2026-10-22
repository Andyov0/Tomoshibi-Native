# Tomoshibi Native

Native clients for the Tomoshibi deployment at meet.example.com, which runs
[Tomoshibi Server](https://github.com/Andyov0/Tomoshibi-Server), a modified
version of [Tomoshibi](https://github.com/5t-RawBeRry/Tomoshibi), the video
meeting server in one binary.

They are not clients for upstream Tomoshibi. They rely on what the
deployment's server adds — relays and picking one, invitations and scheduled
meetings, accounts' pictures, the apps' own updates, and more — and the
address of the deployment is written into them.

The web client that server ships is the reference: a native participant has
to be indistinguishable from a browser to everybody else in the room, and every
rule about what is published, what it is named and what goes over the data
channel comes from there.

What a native client is for is everything a browser cannot do: share the whole
screen of an iPhone, stay in a call with the screen locked, share any window of
a Mac together with its sound, send lossless audio from any machine, and open
straight from an invitation.

## Layout

- `core/` — the media and protocol core, in Rust, on LiveKit's Rust SDK. One
  core for every platform, so the protocol is written once. The apps reach it
  through generated bindings.
- `apple/` — macOS, iOS and iPadOS, in SwiftUI.
- `android/` — Android, in Kotlin with Jetpack Compose, phones and foldables.
- `windows/` — Windows 10 and 11, in WinUI 3 and C#; `windows/README.md` says
  how it is built and tested.
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
