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
- `peek.mjs <port> <expression>` — evaluates one expression in the page.
- `listen.py <port>` — answers every request with 401 and prints what was asked
  for; used to prove the core sends no `/rtc/validate`.
