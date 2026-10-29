//! When a platform's network monitor makes the call resume (network.rs).
//!
//! Every update a monitor gives used to be a candidate for `network_changed`,
//! and each one in a connected call is a resume: the media stalls while it is
//! done. A monitor speaks on being started, on every property of a path that
//! changes, and several times for one hand-over, so these hold the call to
//! hearing only a change of the default route, or the device back online.

use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use tomoshibi_protocol::network::{path_changed, NetworkPath, NetworkWatch, SETTLE};

fn online(route: &str) -> NetworkPath {
    NetworkPath { online: true, route: route.into() }
}

fn offline() -> NetworkPath {
    NetworkPath { online: false, route: String::new() }
}

#[test]
fn the_first_sighting_is_the_baseline() {
    assert!(!path_changed(None, &online("en0"), false));
    assert!(!path_changed(None, &offline(), false));
}

#[test]
fn another_route_or_coming_back_online_is_a_change() {
    let wifi = online("en0 wifi 192.0.2.1");
    assert!(path_changed(Some(&wifi), &online("pdp_ip0 cellular"), false));
    assert!(path_changed(Some(&offline()), &wifi, false));
    // Another Wi-Fi on the same interface is another gateway, and so another
    // route.
    assert!(path_changed(Some(&wifi), &online("en0 wifi 198.51.100.1"), false));
}

#[test]
fn the_same_route_again_and_going_offline_are_not() {
    let wifi = online("en0 wifi 192.0.2.1");
    assert!(!path_changed(Some(&wifi), &wifi.clone(), false));
    assert!(!path_changed(Some(&wifi), &offline(), false));
    assert!(!path_changed(Some(&offline()), &offline(), false));
    // While offline the route means nothing.
    let gone = NetworkPath { online: false, route: "en0".into() };
    assert!(!path_changed(Some(&offline()), &gone, false));
}

#[test]
fn a_wake_counts_once_the_device_is_online() {
    let wifi = online("en0");
    assert!(path_changed(Some(&wifi), &wifi.clone(), true));
    assert!(!path_changed(Some(&wifi), &offline(), true));
}

/// Settled by hand: what stands is compared with what stood before, and a
/// sighting overtaken by another says nothing.
#[test]
fn a_burst_is_one_sighting_and_only_the_last_one_counts() {
    let watch = NetworkWatch::default();

    let baseline = watch.note(online("en0"));
    assert!(!watch.settle(baseline), "the baseline");

    // Wi-Fi to nothing to mobile data, as one hand-over reports it.
    let overtaken = watch.note(offline());
    let last = watch.note(online("pdp_ip0"));
    assert!(!watch.settle(overtaken), "overtaken by the next sighting");
    assert!(watch.settle(last), "another route");
    assert!(!watch.settle(last), "and told once");

    // The same route said again, as a monitor does when a property changes.
    let again = watch.note(online("pdp_ip0"));
    assert!(!watch.settle(again));

    // Back and forth within one settling is no change at all.
    watch.note(online("en0"));
    let back = watch.note(online("pdp_ip0"));
    assert!(!watch.settle(back));

    // Offline settles, and coming back counts.
    let gone = watch.note(offline());
    assert!(!watch.settle(gone));
    let returned = watch.note(online("pdp_ip0"));
    assert!(watch.settle(returned));
}

#[test]
fn a_wake_counts_the_path_standing_or_the_next_one_online() {
    let watch = NetworkWatch::default();

    // Woken before the monitor said anything: nothing to settle yet.
    assert_eq!(watch.note_wake(), None);

    // The wake is still owed, and the first sighting pays it: the sockets
    // went to sleep with the device.
    let first = watch.note(online("en0"));
    assert!(watch.settle(first));

    let woke = watch.note_wake().unwrap();
    assert!(watch.settle(woke), "the same route, after a sleep");
    assert!(!watch.settle(woke), "and once only");

    // Woken offline: the wake waits for the network.
    let gone = watch.note(offline());
    assert!(!watch.settle(gone));
    let woke = watch.note_wake().unwrap();
    assert!(!watch.settle(woke));
    let still = watch.note(offline());
    assert!(!watch.settle(still));
    let back = watch.note(online("en0"));
    assert!(watch.settle(back));
}

/// The timing itself, on the core's runtime: a burst of updates is told once,
/// SETTLE after its last, and an update that changes nothing is never told.
#[test]
fn the_call_is_told_once_per_change_after_it_settles() {
    let told = Arc::new(AtomicUsize::new(0));
    let tell = {
        let told = told.clone();
        move || {
            let told = told.clone();
            move || {
                told.fetch_add(1, Ordering::SeqCst);
            }
        }
    };
    let watch = Arc::new(NetworkWatch::default());

    watch.seen(online("en0"), tell());
    std::thread::sleep(SETTLE + Duration::from_millis(200));
    assert_eq!(told.load(Ordering::SeqCst), 0, "the baseline is not a change");

    // Five updates within a moment, ending on another route.
    for route in ["en0", "en0", "", "pdp_ip0", "pdp_ip0"] {
        let path = if route.is_empty() { offline() } else { online(route) };
        watch.seen(path, tell());
        std::thread::sleep(Duration::from_millis(40));
    }
    assert_eq!(told.load(Ordering::SeqCst), 0, "not before it has stood");
    std::thread::sleep(SETTLE + Duration::from_millis(300));
    assert_eq!(told.load(Ordering::SeqCst), 1, "once for the burst");

    // A property of the same path changing, many times.
    for _ in 0..5 {
        watch.seen(online("pdp_ip0"), tell());
    }
    std::thread::sleep(SETTLE + Duration::from_millis(200));
    assert_eq!(told.load(Ordering::SeqCst), 1);

    // A watch the call has let go of tells nobody.
    watch.seen(online("en0"), tell());
    drop(watch);
    std::thread::sleep(SETTLE + Duration::from_millis(200));
    assert_eq!(told.load(Ordering::SeqCst), 1);
}
