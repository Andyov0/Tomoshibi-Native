//! How long a broken call keeps trying before it gives up (vendor livekit,
//! rtc_engine/reconnect_strategy.rs).
//!
//! The SDK's own schedule gave up in about 22 s, half the web's, so a lift, a
//! tunnel or a Wi-Fi to mobile hand-over that a browser rode out dropped the
//! app from the call. These are the web's numbers: livekit-client's
//! `DefaultReconnectPolicy`, as the deployment's web client is built with it.

use std::time::Duration;

use livekit::rtc_engine::{reconnect_wait, RECONNECT_ATTEMPTS, RECONNECT_DELAYS, RECONNECT_JITTER};

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

#[test]
fn the_waits_are_the_webs() {
    assert_eq!(RECONNECT_ATTEMPTS, 10);
    assert_eq!(
        RECONNECT_DELAYS,
        [ms(0), ms(300), ms(1200), ms(2700), ms(4800), ms(7000), ms(7000), ms(7000), ms(7000), ms(7000)]
    );
    assert_eq!(RECONNECT_JITTER, ms(1000));
}

#[test]
fn the_first_attempt_is_at_once_and_the_second_unjittered() {
    for unit in [0.0, 0.5, 0.999] {
        assert_eq!(reconnect_wait(1, unit), Duration::ZERO);
        assert_eq!(reconnect_wait(2, unit), ms(300));
    }
}

#[test]
fn later_waits_take_up_to_a_second_more_at_random() {
    for attempt in 3..=RECONNECT_ATTEMPTS {
        let delay = RECONNECT_DELAYS[attempt as usize - 1];
        assert_eq!(reconnect_wait(attempt, 0.0), delay);
        assert_eq!(reconnect_wait(attempt, 0.5), delay + ms(500));
        assert!(reconnect_wait(attempt, 0.999_999) < delay + RECONNECT_JITTER);
    }
}

/// Nothing is waited for after the last attempt: the SDK waited up to seven
/// seconds more before saying the call was lost.
#[test]
fn there_is_no_wait_after_the_last_attempt() {
    assert_eq!(reconnect_wait(RECONNECT_ATTEMPTS + 1, 0.5), Duration::ZERO);
    assert_eq!(reconnect_wait(0, 0.5), Duration::ZERO);
}

/// About forty-five seconds of waiting in all, as a browser on the same
/// network keeps trying, where the SDK's own came to about twenty-two.
#[test]
fn the_whole_budget_matches_the_webs() {
    let least: Duration = (1..=RECONNECT_ATTEMPTS).map(|at| reconnect_wait(at, 0.0)).sum();
    let most: Duration = (1..=RECONNECT_ATTEMPTS).map(|at| reconnect_wait(at, 1.0)).sum();
    assert_eq!(least, ms(44_000));
    assert_eq!(most, ms(52_000));
}
