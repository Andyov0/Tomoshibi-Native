//! Music that makes way for speech (duck.ts; the Mac's MusicDuck).
//!
//! Whenever anybody in the call is talking, this participant included, the
//! song desk's music is turned down where it is heard here, and brought back
//! up a moment after they stop: what a radio presenter's desk does under a
//! voice. Each participant decides for itself from the voices it hears and
//! turns down only its own playback, so what goes out is still the file and
//! nobody's choice reaches anybody else. Music also arrives gently: a stream
//! that starts is faded in over a second and a half.
//!
//! The arithmetic alone, with the times given in milliseconds by the
//! caller's clock, so that the tests hold it to the web's cases without
//! waiting.

use std::collections::HashMap;

/// How far the music comes down while somebody talks.
pub const DUCKED: f64 = 0.3;
/// Down quickly, so the first word is heard; up slowly, so a pause is not a
/// lurch.
pub const ATTACK_MS: f64 = 150.0;
pub const HOLD_MS: f64 = 700.0;
pub const RELEASE_MS: f64 = 1200.0;
pub const FADE_IN_MS: f64 = 1500.0;
/// How often the voices are read and the level moved.
pub const TICK_MS: f64 = 40.0;
/// The RMS of a frame that counts as talking, about -38 dB below full
/// scale: a voice after processing sits well above it, and the residue of
/// music after echo cancelling well below.
pub const SPEECH: f64 = 0.012;
/// Readings in a row above `SPEECH` before it is taken for speech: a click
/// is one.
pub const ONSET: u32 = 2;
/// The key this participant's own music is filed under, beside the
/// identities of whoever else plays music (duck.ts `LOCAL`).
pub const LOCAL: &str = "\u{0}local";

#[derive(Debug, Clone)]
pub struct Duck {
    /// The panel's switch, kept as `meet-live.music-duck`.
    pub enabled: bool,
    pub level: f64,
    last_speech: f64,
    loud: u32,
    arrivals: HashMap<String, f64>,
}

impl Duck {
    pub fn new(enabled: bool) -> Duck {
        Duck {
            enabled,
            level: 1.0,
            last_speech: f64::NEG_INFINITY,
            loud: 0,
            arrivals: HashMap::new(),
        }
    }

    /// One reading of every voice that is on, and the level moved by the
    /// time since the last. Whether any music's factor moved.
    pub fn tick(&mut self, levels: &[f64], now: f64, elapsed: f64) -> bool {
        let speaking = levels.iter().any(|level| *level > SPEECH);
        self.loud = if speaking { self.loud + 1 } else { 0 };
        self.step(self.loud >= ONSET, now, elapsed)
    }

    /// Moves the level one tick (duck.ts `step`): toward `DUCKED` at the
    /// attack's pace, held there until `HOLD_MS` after the last word, then
    /// back at the release's pace.
    pub fn step(&mut self, speaking: bool, now: f64, elapsed: f64) -> bool {
        let before = self.level;
        if speaking {
            self.last_speech = now;
        }
        let target = if now - self.last_speech < HOLD_MS {
            DUCKED
        } else {
            1.0
        };
        if self.level > target {
            self.level = target.max(self.level - (1.0 - DUCKED) * elapsed / ATTACK_MS);
        } else if self.level < target {
            self.level = target.min(self.level + (1.0 - DUCKED) * elapsed / RELEASE_MS);
        }
        self.level != before || self.fading(now)
    }

    /// What to multiply a music stream's volume by now: the duck, and the
    /// fade-in of a stream that has just started.
    pub fn factor(&self, key: &str, now: f64) -> f64 {
        let fade = self
            .arrivals
            .get(key)
            .map(|at| ((now - at) / FADE_IN_MS).clamp(0.0, 1.0))
            .unwrap_or(1.0);
        (if self.enabled { self.level } else { 1.0 }) * fade
    }

    /// A music stream started from `key`: faded in once, and again only
    /// after it has gone.
    pub fn arrived(&mut self, key: &str, now: f64) {
        self.arrivals.entry(key.to_owned()).or_insert(now);
    }

    pub fn left(&mut self, key: &str) {
        self.arrivals.remove(key);
    }

    pub fn fading(&self, now: f64) -> bool {
        self.arrivals
            .values()
            .any(|at| now - at < FADE_IN_MS + TICK_MS)
    }

    /// Whether anything still moves without a voice saying so.
    pub fn settled(&self, now: f64) -> bool {
        self.level == 1.0 && !self.fading(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// duck.test.ts: talking brings the music down to 0.3 within the
    /// attack, holds it through a short pause, and lets it back up slowly.
    #[test]
    fn speech_ducks_and_the_music_comes_back() {
        let mut duck = Duck::new(true);
        let mut now = 0.0;
        for _ in 0..10 {
            now += TICK_MS;
            duck.tick(&[0.05], now, TICK_MS);
        }
        assert!((duck.level - DUCKED).abs() < 1e-9, "{}", duck.level);
        // Silence shorter than the hold keeps it down.
        for _ in 0..10 {
            now += TICK_MS;
            duck.tick(&[0.0], now, TICK_MS);
        }
        assert!((duck.level - DUCKED).abs() < 1e-9);
        // Then it rises over the release.
        for _ in 0..60 {
            now += TICK_MS;
            duck.tick(&[0.0], now, TICK_MS);
        }
        assert_eq!(duck.level, 1.0);
    }

    #[test]
    fn a_click_is_not_speech_and_switched_off_nothing_ducks() {
        let mut duck = Duck::new(true);
        duck.tick(&[0.5], 40.0, 40.0);
        duck.tick(&[0.0], 80.0, 40.0);
        assert_eq!(duck.level, 1.0, "one loud reading is a click");
        let mut off = Duck::new(false);
        for n in 1..20 {
            off.tick(&[0.5], n as f64 * 40.0, 40.0);
        }
        assert_eq!(off.factor("ann", 800.0), 1.0);
    }

    #[test]
    fn music_that_arrives_fades_in_once() {
        let mut duck = Duck::new(true);
        duck.arrived("ann", 1000.0);
        assert_eq!(duck.factor("ann", 1000.0), 0.0);
        assert!((duck.factor("ann", 1750.0) - 0.5).abs() < 1e-9);
        assert_eq!(duck.factor("ann", 3000.0), 1.0);
        duck.arrived("ann", 5000.0);
        assert_eq!(
            duck.factor("ann", 5000.0),
            1.0,
            "the next song is not faded"
        );
    }
}
