<p align="center">
  <img src="docs/logo.svg" width="104" alt="Tomoshibi">
</p>

<h1 align="center">Tomoshibi Native</h1>

<p align="center">
  The native apps for <a href="https://github.com/Andyov0/Tomoshibi-Server">Tomoshibi Server</a>:
  video meetings on the Mac, the iPhone and iPad, Android and Windows.
</p>

<p align="center">
  <img alt="macOS 14+" src="https://img.shields.io/badge/macOS-14%2B-111111?style=flat-square&logo=apple&logoColor=white">
  <img alt="iOS and iPadOS 17+" src="https://img.shields.io/badge/iOS%20%C2%B7%20iPadOS-17%2B-111111?style=flat-square&logo=apple&logoColor=white">
  <img alt="Android 10+" src="https://img.shields.io/badge/Android-10%2B-3DDC84?style=flat-square&logo=android&logoColor=white">
  <img alt="Windows 10 and 11" src="https://img.shields.io/badge/Windows-10%20%C2%B7%2011-0078D4?style=flat-square&logo=windows&logoColor=white">
  <br>
  <img alt="Rust" src="https://img.shields.io/badge/core-Rust-B7410E?style=flat-square&logo=rust&logoColor=white">
  <img alt="SwiftUI" src="https://img.shields.io/badge/SwiftUI-F05138?style=flat-square&logo=swift&logoColor=white">
  <img alt="Jetpack Compose" src="https://img.shields.io/badge/Jetpack%20Compose-4285F4?style=flat-square&logo=jetpackcompose&logoColor=white">
  <img alt="WinUI 3" src="https://img.shields.io/badge/WinUI%203-512BD4?style=flat-square&logo=dotnet&logoColor=white">
  <img alt="License AGPL-3.0" src="https://img.shields.io/badge/license-AGPL--3.0-FFA23B?style=flat-square">
</p>

---

Tomoshibi is a video meeting server in one binary, and its web client is enough
for most calls. These apps are for everything a browser cannot do, while staying
indistinguishable from a browser to everybody else in the room: what they
publish, what it is named and what they say over the data channel are the web
client's own.

## What they add

- **The whole screen, with its sound.** Share an iPhone's or iPad's entire
  screen, any window or display of a Mac with its audio, or a Windows or
  Android screen.
- **A call that keeps going.** Stay in a call with the iPhone locked, in
  Android's picture in picture, or with the Mac's window closed.
- **Sound as sound.** A fuller voice, music in stereo, lossless streams, and
  routing to the right speaker or headset.
- **Straight from a link.** Invitations and meeting links open in the app.
- **The nearest relay.** Every relay is timed, and the call goes where it is
  quickest.
- **Watch together and the song desk,** kept in step with the browsers in the
  room.
- **Made for each device.** Glass on the Mac and the iPhone, foldables open and
  half open, phones, tablets and desktop windows alike.
- **Updates in place.** The Mac, Windows and Android apps update themselves
  from the deployment; the iPhone's come through TestFlight.

## Platforms

| | Built with | Notes |
|---|---|---|
| **macOS** | SwiftUI | Signed and notarised disk image |
| **iOS, iPadOS** | SwiftUI | Distributed through TestFlight |
| **Android** | Kotlin, Jetpack Compose | Phones, tablets and foldables |
| **Windows** | WinUI 3, C# | Windows 10 (2004) and 11, arm64 and x64 |

One media and protocol core, written in Rust on
[LiveKit's Rust SDK](https://github.com/livekit/rust-sdks), runs under all four,
so the protocol is written once.

## Requirements

A deployment running [Tomoshibi Server](https://github.com/Andyov0/Tomoshibi-Server).
The apps rely on what it adds to upstream
[Tomoshibi](https://github.com/5t-RawBeRry/Tomoshibi) (relays, invitations and
arranged meetings, the apps' own updates, and more) and do not work with
upstream on its own.

## Repository

| | |
|---|---|
| `core/` | The Rust core and its bindings |
| `apple/` | macOS, iOS and iPadOS |
| `android/` | Android |
| `windows/` | Windows |
| `interop/` | Checks that put a browser in a room with the core |

## Licence

AGPL-3.0-or-later, as Tomoshibi is.
