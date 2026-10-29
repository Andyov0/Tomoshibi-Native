# Interop scripts

All of these drive a Chrome for Testing started with remote debugging and
`--mute-audio` (the server repository's `dev/twobrowsers/launch.sh` does that),
against a local deployment on 127.0.0.1:8080.

- `web-peer.mjs <port> <room> <invite> <seconds>` — follows the probe into its
  room and reports the audio and video it received, whether the hand and the
  chat arrived, the volume rows the sound panel offers, and says something back.
- `web-sealed.mjs <port> <room> <invite> [word]` — the same, sealed with a word,
  and counts what was decoded rather than what was declared.
- `channels.js` — evaluated in the page: the dominant frequency of each channel
  of every received audio track, which is how stereo is told from mono.
- `share-layers.mjs <port> <base> <room> <invite> [shot.png]` — follows the
  probe in, waits for its shared screen, then draws the page large, middling
  and small in turn so that adaptive streaming asks for each rung, and reports
  the size and rate decoded at each: the check that a share's ladder arrives as
  the web publishes it. `base` is the deployment's address, for a deployment
  that is not on 127.0.0.1:8080.
- `follow.mjs <port> <base> <room> [invite]` — takes the browser into a room
  on the deployment at `base` as an ordinary participant and leaves it there,
  with its peer connections kept for `sent.js` and `channels.js`.
- `sent.js` — evaluated in the page: the tracks the media server is sending
  the browser now, by SID, read off its peer connections' remote
  descriptions. A track its sender unpublished leaves the list; one it only
  stopped feeding stays, which is a frozen camera told from a camera that is
  off. With `core/examples/republish.rs` it is the check that a camera, a
  share and a voice can still be taken back after a full reconnect.
- `peek.mjs <port> <expression>` — evaluates one expression in the page.
- `listen.py <port>` — answers every request with 401 and prints what was asked
  for; used to prove the core sends no `/rtc/validate`.
- `app-window.sh <pid> <out.png>` — a picture of that process's window alone.

Driving the app itself: launch the built binary directly with
`TOMOSHIBI_MUTE=1 TOMOSHIBI_BASE=http://127.0.0.1:8080 TOMOSHIBI_AUTOJOIN=<room>`
and `-ApplePersistenceIgnoreState YES`. TOMOSHIBI_MUTE is not optional: the
browser's fake microphone beeps, and the app would play it on the real speakers.
