# Tomoshibi for Windows

A WinUI 3 app over the same Rust core as the Apple and Android apps, with
their screens: starting, sign-in, the lobby (a sidebar on a wide window, the
phone's lobby on a narrow one), starting, joining and scheduling meetings,
the join screen, the door and an arranged meeting's wait, the call, its
endings, and the settings, in the same words, colours and motion.

- `build-core.ps1` builds the core into `Tomoshibi\Native\win-<arch>\tomoshibi_core.dll`
  and generates the C# bindings into `Tomoshibi\Generated`. Neither is
  committed, as on the other platforms.
- `Tomoshibi.Basics\` holds the rules the three platforms share, ported from
  Android's `basics/` (which follows the Apple apps' TomoshibiBasics and the
  web): the room planner, the stage pin and the speaking order, the
  fill-or-whole framing rule, the corner's notice times, the chat box's
  limit, the address finder, the language rules and phrase table, the
  palette and accents, and the sentences the server's refusals read as.
  A plain .NET library, so that `Tomoshibi.Tests\` runs without the core:
  `dotnet test windows\Tomoshibi.Tests` holds them to Android's cases one for
  one, and checks that every `T("…")` in the Windows sources is a phrase of
  the shared catalog.
- `Tomoshibi\` is the app: `Model\` (AppModel, LobbyModel, CallModel, the
  stores), `Media\` (capture, playback, the synthetic sources), `UI\` (the
  kit, then the screens, the lobby, the room and the settings).

## The app

The model is Android's, decision for decision: who is signed in and what the
deployment allows decide between the lobby and the join form; only somebody
signed in or holding an invitation is taken into a room by its name; a call
the server moves is joined again by itself. A release build talks to the home
deployment alone and reads no switches; no screen shows a server's address.

The screens are built in code, as Compose builds Android's, and follow the
Windows way where the platforms differ: the content runs under a dark title
bar with the caption buttons in the apps' greys; the control layer (the
lobby's sidebar and join bar, the call's bar, the popovers, the pills over
the pictures) wears in-app acrylic and what is inside it is plain; fields,
switches, the calendar and time pickers and the device lists are Windows'
own controls in the apps' colours, the tally carried into their accent;
Escape closes what lies on top, then does what the screen's Back does. Every
view arrives on the settle curve (320 ms) and leaves on the leave curve
(200 ms), through composition animations on its own visual.

The window opens as a desktop window: the first time 1280 by 820
device-independent pixels at the monitor's scale (never more than nine
tenths of the work area), in the middle of it; after that where it was left,
as large, and maximised if it was. It can be made as small as 360 by 560.
The layout follows its width by the other apps' lines: from 600 the wide
layout (the sidebar lobby, the desktop room), below it the phone's; the
sidebar is 250 wide below 760 and 284 from it.

The call places every picture by the shared planner and frames a camera by
the shared rule: shown whole, a camera sits on a blurred, darkened glimpse of
itself that the core keeps of the last frame (`video_panel_glimpse`), taken
once a second and crossfaded, since a composition swap chain cannot be read
back by the app.

What is kept: the choices in `%LOCALAPPDATA%\Tomoshibi\preferences.json`
under the web's own keys; the session cookie and the join passphrase sealed
with DPAPI for the Windows user in `%LOCALAPPDATA%\Tomoshibi\secrets`. Not
the Credential Locker, which roams what it holds to the person's other PCs;
a deployment's session belongs to the device it was signed in on.

Sharing is Android's share menu on the bar's one share button: the picture's
size and frame rate, then sharing the screen, then sound on its own with the
lossless switch under it (turned off, with the reason, in a sealed call). A
screen comes from the system's picker; sound shared on its own, and a shared
screen's sound, is everything the computer plays except the call, since
Windows names no app's sound by its window, where Android's line is "Pick
the app that is playing" for its own picker. The sharer's own picture is a
card that says so with a stop; who has it on their stage is read from the
web's `watching` attribute, and this app writes the same attribute for the
screen on its own stage. A remote screen off the stage is a card rather than
a desktop shrunk into a cell; on the stage it has the stage's controls (the
same person's camera, fill the screen). The uplink card of a share that
cannot be sent at its size (strain.ts) stands above the notices.

The host panel is Android's: the invitation with its link and the quiet
stop, the door (also drawn inside its notice, so a knock is answered where
it is read), moving the call to another relay, the people with mute, make
host (faint beside a guest, and pressable so the refusal can be said) and
remove, and ending the meeting asked twice in the panel. The sound panel
sets each person's voice, screen and music apart, under the web's hearing
store; the picture menu mutes or unmutes somebody for this listener alone.
The panel's buttons are drawn for a pointer (28 across) rather than a
fingertip (48), and keep the keyboard where it was when the panel is drawn
again.

Watching together is Android's: the protocol crate's theatre and keeper on
the call's `watch` topic, the show first among the pictures whenever one is
believed, its panel (a link to paste, the queue, the media servers for
somebody given them) and its stage (the controls, which drive everybody;
"does not reach you" with the reason; the relay's stop and its try again).
A file or a playlist plays in Windows' media player, a playlist through
AdaptiveMediaSource held to the viewer's ceiling and to the size it is shown
at by its renditions' bandwidths; YouTube plays in its own player in a
WebView2 whose page is answered as if from the deployment's address, so it
is embedded from the origin the web embeds it from. Where the video is
fails, or does not begin within eight seconds, it plays from the relay. The
controls show while the pointer moves over the video and for three seconds
after, where Android shows them for a while after a touch.

The song desk is Android's: the protocol crate's desk on the call's
`jukebox` topic, made with the room so that music anybody plays is turned
down under speech from the start. A song is fetched with the app's session
and decoded by Media Foundation's transcoder into two files read as they
play, 48 kHz for the Opus track and the file's own rate in floats for the
lossless stream, so that a five-minute song is never held in memory; one
track is published for the whole of the music and fed one 10 ms block at a
time by a pacer on a thread of its own, woken by a high-resolution waitable
timer, since Windows' own sleep rounds up to its 15.6 ms tick. The holder
hears it through the call's speakers behind a volume of their own; whoever
listens sets it in the sound panel's music row, which the panel shares.

A link the app is opened with (an invitation, `?invite=<token>#/<room>`, or
a meeting's `#/<room>`, in the deployment's own form or as `tomoshibi://`) is
read in `Program.Main`, before XAML, and opened once the deployment has
answered, as the other apps open one. One app runs at a time for a person
(`AppInstance` keyed "tomoshibi"): a second launch with a link hands it to
the first, whose window comes forward and opens it, and ends. A test's
launches each run on their own unless they share a store (`TOMOSHIBI_KEEP`),
which is how the hand-over is tested. `tomoshibi://` means `https://` on the
same host, as on Apple and Android, so against the kit's `http://` deployment
such a link is refused as being for another server; the deployment's own
`http://` form is what the kit opens end to end. The system hands
`tomoshibi://` to the packaged app only (its manifest registers the scheme);
an unpackaged build reads a link from its command line. Windows' counterpart
of Android's verified app links and Apple's associated domains (an app URI
handler, which needs a `windows-app-web-link` file served by the site) is not
something the deployment serves, so an `https://` link opens in the browser,
whose page offers the app.

## The machine it is built on

Windows 11 on Arm (build 26200), in the owner's Parallels VM. Everything below
came from the vendor's own site, and is what was installed, how, in this order.

| What | Version | From | How |
|---|---|---|---|
| Git for Windows | 2.56.0.windows.2, arm64 | `github.com/git-for-windows/git/releases` (`Git-2.56.0.2-arm64.exe`) | `/VERYSILENT /NORESTART /NOCANCEL /SP- /SUPPRESSMSGBOXES` |
| .NET SDK | 10.0.402 (the current LTS), arm64 | `builds.dotnet.microsoft.com` (`dotnet-sdk-10.0.402-win-arm64.exe`), SHA-512 checked against the release metadata | `/install /quiet /norestart` |
| LLVM | 23.1.3, Windows on Arm | `github.com/llvm/llvm-project/releases` (`LLVM-23.1.3-woa64.msi`) | `msiexec /i … /qn /norestart` |
| Visual Studio Build Tools 2026 | 18.11 (MSVC 14.51.36231, Windows SDK 10.0.26100.0) | `aka.ms/vs/stable/vs_buildtools.exe` | `--quiet --wait --norestart --nocache --add Microsoft.VisualStudio.Workload.VCTools --add Microsoft.VisualStudio.Component.VC.Tools.ARM64 --add Microsoft.VisualStudio.Component.VC.Tools.x86.x64 --add Microsoft.VisualStudio.Component.Windows11SDK.26100` |
| Rust | rustup, stable 1.99.1 | `static.rust-lang.org/rustup/dist/aarch64-pc-windows-msvc/rustup-init.exe` | for the user, not system-wide: `-y --profile minimal --default-toolchain stable --default-host aarch64-pc-windows-msvc --target x86_64-pc-windows-msvc` |

The installers ran as SYSTEM (`prlctl exec` without `--current-user`), since
nobody is there to answer a UAC prompt; rustup ran as the user.

Not installed, because nothing needed it: C++ ATL (neither webrtc-sys's C++
nor the prebuilt WebRTC's objects ask for an ATL library), CMake, NASM, Perl.
LLVM is there for one reason: ring compiles its C with clang when it targets
Windows on Arm (its `build.rs`), and `build-core.ps1` puts it on the path.

Per project, not per machine: the NuGet packages `Microsoft.WindowsAppSDK`
2.5.1 and `Microsoft.Windows.SDK.BuildTools` 10.0.26100.9169 (restored by
`dotnet build`), and the C# binding generator, which `build-core.ps1` builds
into `windows\.tools` (see below).

Two settings in the VM user's global git config: `safe.directory` for the
repository on the Mac's share (a network drive records no owner, and git
refuses it otherwise), and `core.autocrlf false`.

## Building

Clone the repository onto a local disk; a build on the Mac's share is slow and
puts cargo's and MSBuild's outputs on the Mac. Then, in PowerShell:

```powershell
windows\build-core.ps1                          # debug, arm64
windows\build-core.ps1 -Profile local           # optimised, talks to the server it is given
windows\build-core.ps1 -Profile release -Arch x64

dotnet build windows\Tomoshibi\Tomoshibi.csproj -c Debug -r win-arm64 -p:Platform=ARM64
dotnet build windows\Tomoshibi\Tomoshibi.csproj -c Debug -r win-x64   -p:Platform=x64
```

The profiles are the other platforms': `release` admits only the home
deployment (`protocol/src/api/mod.rs`, `admitted`), so anything pointed at a
local deployment is `debug` or `local`, which is release with debug assertions.

The first core build downloads LiveKit's prebuilt WebRTC for the target
(about 110 MB) and takes five minutes on four Arm cores; later ones take
seconds. The app is at
`windows\Tomoshibi\bin\ARM64\Debug\net10.0-windows10.0.26100.0\win-arm64\Tomoshibi.exe`.

### What the Windows link needs

- **The static C runtime.** The prebuilt WebRTC's objects ask for libcmt and
  libcpmt and refuse to be linked with anything built against the DLL
  runtime (`/FAILIFMISMATCH:RuntimeLibrary=MT_StaticRelease`), so every
  Windows target is built with `+crt-static` (`core/.cargo/config.toml`).
- **The system libraries reach the DLL by themselves.** On Android,
  webrtc-sys's `rustc-link-arg`s never reached the final link and the JNI
  symbols had to be named again in `build-core.sh`. On Windows everything
  webrtc-sys asks for is a `rustc-link-lib` (`msdmo`, `wmcodecdspuuid`,
  `dmoguids`, `strmiids`, `winmm`, `secur32`, `crypt32`, `iphlpapi`, `ws2_32`,
  `d3d11`, `dxgi`, …), which cargo does carry to the final link, and the
  archive names the rest itself (`/DEFAULTLIB`: `avrt`, `dbghelp`, `user32`,
  …). The DLL linked on the first attempt for both architectures.
- **A DLL only for Windows.** `build-core.ps1` asks for a cdylib with
  `cargo rustc --crate-type cdylib`, as `android/build-core.sh` does, rather
  than in `Cargo.toml`, where every Apple target would link one too; and
  without the staticlib, which would copy all of WebRTC into a second archive.

### Arm64 and x64

LiveKit publishes its prebuilt WebRTC for `win-arm64` as well as `win-x64`
(release `webrtc-89d790b`), so the app is native on Arm. The x64 build is made
on the same Arm machine, cross-compiled, and runs there under emulation.

## The C# bindings

UniFFI has no C# generator of its own. NordSecurity's `uniffi-bindgen-cs` is
the one there is, and its latest release (v0.11.1) is built on UniFFI 0.31:
bindings from it refuse a 0.32 library at load, by its checksums. The options
were:

- **a fork of the generator on 0.32**, which exists as the open upstream pull
  request NordSecurity/uniffi-bindgen-cs#176 (175 binding tests passing, and
  reported working against matrix-rust-sdk on 0.32.1);
- **an older UniFFI for Windows alone**, which would mean two versions of the
  macros over one core and a core that is not the one the other apps run;
- **a hand-written C API with P/Invoke over it**, which would mean writing and
  keeping by hand what the generator writes for the other platforms: the core
  and the protocol crate export over two hundred items, async functions and
  callback interfaces among them.

The first is the least bad, and it is what `build-core.ps1` does: it builds
the generator from that pull request, pinned to one commit
(`dennisameling/uniffi-bindgen-cs@cf0aa7a8`), into `windows\.tools`, and runs
it over the DLL that was just built. Generated for this core it compiles
without a change: both crates, `async` methods as `Task`s, callback
interfaces, errors as exceptions. Move to the first release made on 0.32 when
NordSecurity publishes one.

## Media on Windows

- **Remote video is drawn by the core**, as on Android (`core/src/call/panel.rs`).
  The decoder hands over I420 in memory; giving that to C# would be every
  frame of every picture copied across the binding and converted to BGRA in
  managed code. Instead the core makes a composition swap chain on its own
  Direct3D 11 device, the app sets it on a `SwapChainPanel`
  (`ISwapChainPanelNative::SetSwapChain`), and each frame is converted with
  libyuv into the back buffer and presented; the swap chain's matrix
  transform scales it to the panel. A `SwapChainPanel` cannot be turned or
  cut by layout, so the turn a phone's camera asks for and the crop of a
  picture that fills its tile are made in the core, from the size the app
  reports (`place_video_panel`). `video_panel_frames` counts what was drawn.
- **Remote sound** is `attach_pcm`: 10 ms of 48 kHz PCM per sound, measured
  always and played through an audio graph (one frame input node per sound)
  unless the launch is muted. A muted launch never opens the graph. Speakers
  picked in a call take effect at once: the graph is made again on them and
  every sound moved across before the old one stops (`Playback.UseOutput`).
  `TOMOSHIBI_MUTE=speakers` opens the graph on the device with its output at
  nought, so that a test can change the speakers with nothing reaching them;
  the log then says which speakers play and how many quanta they rendered.
  Speakers that stop under the call (unplugged, or the system's own changing
  under a graph that does not follow) move the call to the system's, as the
  web's SDK moves its output; the choice remembered is kept for the next call.
- **Lossless sound** from others (the web's `lossless` data streams) comes
  from the core whole and is played by the app as a pulled sound: a cushion,
  a windowed-sinc resampler to the graph's rate and an eased gain
  (`Media\LosslessPlayback.cs`, the rules in `Tomoshibi.Basics\Sound.cs`).
- **A shared screen** is `Windows.Graphics.Capture` on a Direct3D 11 device,
  copied to a staging texture and handed over as BGRA (`push_bgra`, which
  converts and scales with libyuv in the core); in a test a generated screen
  (`Media\ScreenCapture.cs`).
- **Shared sound** is process loopback with the call's own process excluded
  (`VAD\Process_Loopback`, `Interop\Loopback.cs`), sent as the screen's
  sound or, on its own, as a track or as lossless data; in a test a generated
  chord (`Media\SoundCapture.cs`).
- **Local video** goes in through `push_planes`: a generated picture
  (`Media\Synthetic.cs`) or the camera through `Windows.Media.Capture`
  (`Media\Devices.cs`), opened at the format closest to what the web sends
  from a computer, 1280x720 at thirty (share.rs `camera_capture`; the choice
  in `Tomoshibi.Basics\Devices.cs`, `CameraFormats`), read as NV12 at the size
  sent, which the frame reader scales to, and paced to thirty by Android's
  rule (`FramePacer`). The track is published at the size the camera came
  back at, as the web's encodings follow its capture's.
- **Local sound** goes in through `push_pcm`: a generated 440 Hz tone at
  -20 dBFS, or the microphone through WASAPI (`Interop\WasapiCapture.cs`),
  48 kHz mono in shared mode. A call's voice is a communications stream,
  which is where Windows applies the endpoint's echo canceller, noise
  suppression and gain control; original sound is raw where the microphone
  offers it and ordinary where not (Android's VOICE_COMMUNICATION,
  UNPROCESSED and MIC). The echo canceller is told which speakers the call
  plays to (`IAcousticEchoCancellationControl`, Windows 11 22H2 and later),
  and the log names the effects the stream runs (`IAudioEffectsManager`).
- **Devices come and go** (`Media\DeviceList.cs`): Windows' lists are
  watched, and the devices panel and the settings follow them. A camera or a
  microphone unplugged under a call carries on, on the same track, on the
  first camera still there or the system's microphone, as the web's SDK
  restarts an ended track; with none left the call turns it off and says so
  (`DeviceRules` in `Tomoshibi.Basics`). The settings' audio and video page
  opens the camera and the microphone while it shows, for the picture and
  the level, as Android's does.
- **Background blur** finds the person with the web's own segmenter,
  MediaPipe's selfie model (`web/public/segment`), converted to ONNX by
  `Blur/convert.py` (one MediaPipe operator rewritten as TensorFlow Lite's
  own, then tf2onnx; its run against MediaPipe is written at the top of the
  script) and run by `Windows.AI.MachineLearning`, which every Windows 10
  and 11 has, on the processor: 7 ms a frame in the VM, its mask within
  0.00002 of MediaPipe's on MediaPipe's test portrait
  (`TOMOSHIBI_DEBUG_SEGMENT`). The blur and the blend are Android's
  (`Tomoshibi.Basics\BlurMath.cs`): the frame shrunk to a quarter, blurred
  there with the web's radius scaled to the picture, and laid under the
  person through the mask, the segmenter running on a thread of its own so
  that a frame never waits for it. About 11 ms for a 1280x720 frame on the
  capture thread in a release build in the VM. Applied before the frame is
  sent and before the picture of yourself is drawn, the join screen's
  included, as the web's preview is; the camera's frames go through it as
  I420 after they are paced to the rate sent (`Media\CameraBlur.cs`), and
  a test's generated and made-up cameras too.

Synthetic is the default for every test, so that no test can capture
whoever sits at a machine whose devices are another's (a VM shares its
host's); a person's own launch uses the real devices. The real camera and
microphone paths are not yet checked on real hardware: the VM's only
devices are its host's. What is checked is everything above them, with a
test's made-up devices (`TOMOSHIBI_DEVICES`), each made-up camera a picture
of its own hue and each microphone a tone of its own, so that the browser
in the room says which one is being sent.

## Packaging

The tests build it unpackaged, with the Windows App SDK and .NET carried in
the output (`WindowsPackageType=None`, `WindowsAppSDKSelfContained`,
`SelfContained`): the build directory runs as it is, and a launch inherits
its environment, which is how the `TOMOSHIBI_*` switches reach it.

For distribution `package.ps1` builds the same project as MSIX
(`WindowsPackageType=MSIX`, `Package.appxmanifest`) for arm64 and for x64,
puts the two in one `Tomoshibi-<version>.msixbundle`, which is the file the
deployment's shelf and an update hand out, and signs it with a certificate
found by its thumbprint in the current user's store, so that no password is
ever on a command line. The version given is written into the manifest's
identity, four dotted numbers as `apps.json` writes them, and the
certificate's subject into its publisher: Windows installs a newer version
over an older one only from the same publisher, and the app checks the same
before it installs an update. Windows picks the package for the machine out
of the bundle. The package registers `tomoshibi://`, and asks for the
camera and the microphone as a packaged app must.

A package is started by the system and hands the app no environment; a
packaged debug build reads its switches from
`%LOCALAPPDATA%\Tomoshibi-tests\switches.env` instead (`NAME=value` a
line), so that a test of the package, launched from its entry or by a link,
still names the local deployment. An unpackaged build never reads that file,
and a release build reads neither.

Tested in the VM with a self-signed certificate (`CN=Tomoshibi`, trusted on
that machine for sideloading only): 0.1.0.1 and 0.1.0.2 built as bundles of
about 220 MB holding both architectures; 0.1.0.1 installed (Windows took the
arm64 package), launched from its entry, signed in to the local deployment
through the switches file, and was upgraded in place by installing 0.1.0.2
over it, and again by updating itself (below); a `tomoshibi://` activation
started it cold ("opened with a link") and, with it running, was handed to
the running app ("handed a link") with no second process left; it
uninstalled to nothing, the scheme's registration with it. Opening such a
link through the
shell put up Windows' own "Pick an app" the first time, as Windows does for a
scheme nobody has chosen an app for; the test activated the package through
`Windows.System.Launcher` with the package named instead, rather than pick a
default for the person. A distributed build needs a real code-signing
certificate (see the report of phase 2b for what that takes).

### Updates

`Tomoshibi.Basics\Updates.cs` is Android's `Updates.kt`: asked at start and
every six hours, a failed check asked again in ten minutes and never said; an
offer shown only in the lobby (sign-in included), never on the join screens,
in a call or at its end; a download held when the lobby is left and carried
on when back; an install under way given up when the lobby is left, the
build kept, so that "Update" again installs it without downloading it;
"Later" for the rest of the run; a check asked for in the settings always
answering. The core asks and downloads (`CheckForUpdate`, `DownloadUpdate`),
as the app (`Tomoshibi/<version> (Windows ...)`), and refuses a file that is
not the build it described. `Model\AppUpdates.cs` installs with
`PackageManager.AddPackageByUriAsync` and `ForceTargetAppShutdown`, after
reading the bundle's own manifest (this app's name and publisher, the
version offered) and the subjects of the certificates its signature carries
(this app's publisher); Windows then checks the signature itself and that the
machine trusts it. The app registers to be restarted, and Windows opens it
again once the new version is in place. Android's permission and
confirmation steps have no Windows counterpart. Only a debug build given a
deployment, or a release build, asks at all; an unpackaged build is offered
updates like any other and says it cannot update itself here when asked to
install one.

The version the app says, in its user agent and to the update check, is the
core's `version_with_build` of the first three parts of its assembly's
version and the fourth, the build (owner, 2026-10-22: a small fix keeps the
version and takes the next build; a new version is for new features or a
large update): 0.1.4 for build 1, 0.1.4.2 for build 2. People are told it as
`version_label` writes it, 0.1.4 (2), in the settings and on the banner alike.
`setup.ps1 -Version 0.1.4 -Build 2` puts the four numbers in the app and the
joined form in the setup's name and its stamp's product version, which every
app holds a downloaded setup to, those from before builds were said included.

The download's bar is drawn at an even pace behind what has arrived
(`Tomoshibi.Basics\Pace.cs`, moved frame by frame from
`CompositionTarget.Rendering` by `UI\Kit\Paced.cs`): nought to full in no less
than 1.2 seconds, never ahead of what has arrived, and back at once where a
download begins again. The install waits for it to be drawn full, three
seconds at most, for a window with no frames. The setup's own window paces
its bar the same way off a timer and draws it full before it goes.

Tested end to end with the packages above against a local deployment of
the newer server (which has `/api/apps/latest/windows`) whose shelf held
0.1.0.2: the installed 0.1.0.1 offered it in the lobby, downloaded the 220 MB
on "Update" with the progress shown, installed it, and Windows opened it again
three seconds later as 0.1.0.2 with nothing more to offer. A 0.1.0.3 signed
by another certificate (CN=Tomoshibi Impostor) was offered, downloaded and
refused before Windows was handed it: "The update is not signed by
Tomoshibi's developer, so it was not installed." The banner's steps, posed for pictures
(`TOMOSHIBI_SHOW=update:offered|downloading|installing|failed|current`, and
`:about` to open the settings there), are in the kit's shots beside
Android's and the Mac's.

## Switches

Read from the environment, the same names the Apple apps read:

| Switch | What it does |
|---|---|
| `TOMOSHIBI_BASE` | The deployment. Every other switch needs it, because the default is the home deployment. |
| `TOMOSHIBI_NAME` | The name shown to the room. |
| `TOMOSHIBI_SIGNIN=name:file` | Signs in first, the passphrase read from the file. |
| `TOMOSHIBI_PASSPHRASE` | A passphrase to join with. |
| `TOMOSHIBI_SEALING_WORD` | Seals the call. |
| `TOMOSHIBI_AUTOJOIN` | Straight into this room. |
| `TOMOSHIBI_MUTE` | Plays nothing; only measures. Always set in tests. `=speakers` opens the speakers and plays silence to them. |
| `TOMOSHIBI_MIC`, `TOMOSHIBI_CAMERA` | Joins with each published. |
| `TOMOSHIBI_SOURCE=devices` | The real camera and microphone instead of the synthetic ones. |
| `TOMOSHIBI_DEVICES=file` | Made-up devices, listed from the file and read again as it changes: a line each, `kind<tab>name[<tab>number]`, kinds `camera`, `microphone`, `speakers`; the number is a camera's hue in degrees or a microphone's tone in hertz. Always with the generated sources. |
| `TOMOSHIBI_DEBUG_LOG=file` | Writes what the app does and, every two seconds, what it sent and received. |
| `TOMOSHIBI_DEBUG_QUIT_AFTER=seconds` | Leaves and quits. |
| `TOMOSHIBI_PREPARE=room` | To this room's join screen. |
| `TOMOSHIBI_OPEN=link` | Opens an invitation or an arranged meeting's link, as the system would hand it over. |
| `TOMOSHIBI_ENDED=left\|closed\|removed\|lost` | Opens on that ending, for pictures of it. |
| `TOMOSHIBI_LANGUAGE=en\|zh-Hans\|zh-Hant\|ja` | Speaks this language for the launch, without keeping it. |
| `TOMOSHIBI_SHOW=…` | Opens something over the screen: `start`, `join`, `schedule`, `servers`, `settings[:general\|media\|account\|about]`, and in a call `chat`, `devices`, `sound`, `host`, `share`, `leave`. Once, in the first call. |
| `TOMOSHIBI_SHOW=update:…` | Poses an update at a step (`offered`, `downloading`, `installing`, `failed`, `current`) and asks the deployment nothing; `:about` after it opens the settings there. |
| `TOMOSHIBI_BLUR` | Blurs the background from the start. |
| `TOMOSHIBI_CAMERA_PICTURE=file` | The generated camera shows this picture (a person) drifting over its bands, for the blur to find. |
| `TOMOSHIBI_DEBUG_SEGMENT=folder` | Runs the segmenter on `portrait-input.bin` and logs how near its mask is to `portrait-mediapipe.bin` (both from `Blur/convert.py`). |
| `TOMOSHIBI_WATCH=link` | Watches this video together straight after joining, once the server says this person may. |
| `TOMOSHIBI_WATCH_YOUTUBE_API=url` | Where the YouTube player's page loads YouTube's IFrame API from: the kit's stand-in, so that no test reaches YouTube. |
| `TOMOSHIBI_SHARE=screen\|sound\|both` | Shares straight after joining: a generated screen and chord unless `TOMOSHIBI_SOURCE=devices`, and then the first screen without the picker. |
| `TOMOSHIBI_KEEP=name` | Keeps what the launch is told in a store of the test's own, `%LOCALAPPDATA%\Tomoshibi-tests\name`, so that a later launch can find the sealed session. |

Only a debug build reads them. Everything that drives the app needs
`TOMOSHIBI_BASE` as well: a launch without it is the release app's, at the
home deployment, with nothing automatic (only `TOMOSHIBI_MUTE` and the log
are honoured). A launch with it is a rehearsal and keeps nothing (no name,
passphrase, session, identity, language or colour) unless `TOMOSHIBI_KEEP`
gives it a store of its own.

## Testing

As for the other platforms, against a local deployment and a headless
browser, never the production fleet. The deployment runs on the Mac,
listening on its address on the Parallels shared network with `node_ip` set
to it, and the browser (Chrome for Testing, `--mute-audio`, fake devices) on
the Mac as well; it treats that plain-http origin as secure
(`--unsafely-treat-insecure-origin-as-secure`), or it would have no
`getUserMedia`. The app, in the VM, is launched with `TOMOSHIBI_BASE` naming
that deployment, `TOMOSHIBI_MUTE`, and the synthetic sources. A muted launch plays a video
watched together at nought, and its YouTube player in a WebView2 started with
every sound off.

The app's log says what it drew and heard; the browser's `inbound-rtp` stats
and `interop/channels.js` say what it received.
