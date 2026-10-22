# Working on Tomoshibi Native

This repository holds native clients for a Tomoshibi deployment. The server and
its web client live elsewhere: the deployment runs a fork of Tomoshibi
(https://github.com/Andyov0/Tomoshibi/tree/relay-mode; upstream
https://github.com/5t-RawBeRry/Tomoshibi), with server features the apps rely
on that upstream does not have and the public fork does not all carry. The
deployment's web client is the reference for every rule here.

## The rule everything follows from

**A native participant is indistinguishable from a browser to everybody else.**
Track sources and names, participant attributes, data topics and their JSON,
the encryption key derivation, the simulcast ladders: all of them are what the
web client does, and where they are not, a browser in the same room shows the
difference. When a choice here seems free, look up what the web client does
first and say in a comment where.

## Verifying

Verify against the real thing. A unit test cannot hear a stereo track arrive in
mono, and that happened. Anything about media is checked by putting the core in
a room with a headless browser (`interop/`) and reading what the browser
received. Run browsers with `--mute-audio`; a fake microphone beeps once a
second and goes to the real speakers otherwise.

A test that passes when the code is broken is worse than no test: break the
guard, watch it go red, put it back.

Never run test traffic through the production fleet. A local deployment of the
server and a headless browser are enough for almost everything.

## Comments and commits

Comments say why, in prose, about the thing rather than the change; name the
fault that prompted the code, and what breaks if it is undone. No emoji
anywhere. Commit messages are prose: what was wrong, what was considered, what
was chosen, what was verified.

Never attribute a commit, or anything else, to an assistant.

## Apple builds

- Every Apple target links with `-ObjC` (`core/.cargo/config.toml`). Without it
  the process aborts before `main` with no message at all.
- Build with the release Xcode (`/Applications/Xcode.app`), not a beta.
- `perl -e 'alarm …'` strips `DYLD_*` variables, because perl is a protected
  system binary; run a binary directly when injecting anything.
- Test the Swift packages with `swift test` (for example
  `cd apple/TomoshibiKit && swift test`). `xcodebuild test` timed out before
  the first test on this machine, on the Mac and in the simulator alike, for a
  bundle with nothing of the core in it.
- Touches on an iPhone are tested with UI tests, which do run:
  `xcodebuild test -scheme Tomoshibi-iOS -only-testing:TomoshibiUITests
  -destination 'platform=iOS Simulator,id=<udid>'`, against the local
  deployment on 127.0.0.1:8080. They are the only check that a control can be
  pressed with a finger; the debug presses reach the model, not the hit
  testing. A failed assertion then sits for minutes collecting the
  accessibility hierarchy. Switches for the app go on the line as
  `TEST_RUNNER_TOMOSHIBI_<name>=…`.
- On iOS a view inside `glassEffect` can be touched only within the glass
  shape. Anything that hangs outside its surface (a popover off a bar) needs
  the glass behind it (`glassBacking`), or it is drawn and cannot be pressed.
- A Debug build of the app launched from a shell may restore a window state
  left by an earlier build and open no window at all; pass
  `-ApplePersistenceIgnoreState YES`. `TOMOSHIBI_BASE` and
  `TOMOSHIBI_AUTOJOIN` take it straight into a call, which is what the
  interop scripts use.
