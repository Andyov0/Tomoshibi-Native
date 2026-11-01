//! Tells the call what the machine's connection is, every time the system
//! says anything about it (`Call::path_seen`), and when the machine wakes
//! from sleep (`Call::woke`), as NetworkWatch.cs and NetworkWatch.swift do.
//!
//! Without it a call on a connection that went away noticed only when the
//! server's ping went unanswered, a call gone quiet for that long, and a call
//! already reconnecting when the connection came back sat out the rest of
//! its wait, up to eight seconds; the web resumes or retries the moment the
//! browser says it is online. GLib's network monitor (netlink, or
//! NetworkManager through the portal) speaks many times for one change and
//! for changes that are not the default route's, so every report is passed
//! on as it comes and the core decides (protocol network.rs): it waits for
//! a burst to settle and acts only on a real change of path.
//!
//! The monitor says whether there is a network, not which one: the route is
//! the default route's interface and gateway as the kernel lists them, for
//! IPv4 and IPv6, which is what changes when a cable is plugged in or the
//! Wi-Fi hands over to a phone's hotspot.
//!
//! A machine waking from sleep may report no change at all (the route is the
//! one it went to sleep on) while the call's sockets died with the sleep;
//! logind says so on the system bus (`PrepareForSleep(false)`).
//!
//! The call is the room's (ui/room `RoomState`); the watch looks it up there
//! each time, and gives a call it has not seen before the path standing now
//! as its first sighting, which the core takes as the baseline.

use std::{cell::RefCell, sync::Arc};

use gio::prelude::*;
use tomoshibi_core::call::Call;
use tomoshibi_protocol::network::NetworkPath;

use super::{log, AppModel, Change};
use crate::ui::room::RoomState;

/// What the watch keeps (`AppModel::kept`).
#[derive(Default)]
pub struct NetworkWatch {
    /// The call last given a baseline, by address, so that each call is
    /// given one once.
    told: RefCell<Option<usize>>,
    /// Held for the life of the app: dropped, the bus stops telling.
    sleep: RefCell<Option<gio::SignalSubscription>>,
    monitor: RefCell<Option<(gio::NetworkMonitor, glib::SignalHandlerId)>>,
}

/// Starts watching, once for the life of the model.
pub fn start(model: &AppModel) {
    let watch = model.kept::<NetworkWatch>();
    if watch.monitor.borrow().is_some() {
        return;
    }
    let monitor = gio::NetworkMonitor::default();
    let weak = model.downgrade();
    let handler = monitor.connect_network_changed(move |monitor, _| {
        let Some(model) = weak.upgrade() else {
            return;
        };
        let path = path_of(monitor);
        if let Some(call) = call_of(&model) {
            log(&format!(
                "network path: online={} route={}",
                path.online,
                if path.route.is_empty() { "none" } else { "default" }
            ));
            call.path_seen(path);
        }
    });
    watch.monitor.replace(Some((monitor, handler)));

    // A new call is given the path as it stands, as its baseline.
    let weak = model.downgrade();
    model.subscribe(move |_, change| {
        if change != Change::Module("room") {
            return;
        }
        let Some(model) = weak.upgrade() else {
            return;
        };
        let watch = model.kept::<NetworkWatch>();
        let call = call_of(&model);
        let address = call.as_ref().map(|call| Arc::as_ptr(call) as usize);
        if *watch.told.borrow() == address {
            return;
        }
        watch.told.replace(address);
        if let Some(call) = call {
            call.path_seen(path_of(&gio::NetworkMonitor::default()));
        }
    });

    let weak = model.downgrade();
    gio::bus_get(gio::BusType::System, gio::Cancellable::NONE, move |bus| {
        let Ok(bus) = bus else {
            log("no system bus: a wake from sleep is noticed only by the call itself");
            return;
        };
        let Some(model) = weak.upgrade() else {
            return;
        };
        let told = model.downgrade();
        let subscription = bus.subscribe_to_signal(
            Some("org.freedesktop.login1"),
            Some("org.freedesktop.login1.Manager"),
            Some("PrepareForSleep"),
            Some("/org/freedesktop/login1"),
            None,
            gio::DBusSignalFlags::NONE,
            move |signal| {
                // true going to sleep, false coming back.
                let going = signal.parameters.get::<(bool,)>().map(|(going,)| going);
                if going != Some(false) {
                    return;
                }
                let Some(model) = told.upgrade() else {
                    return;
                };
                if let Some(call) = call_of(&model) {
                    log("woke from sleep");
                    call.woke();
                }
            },
        );
        model.kept::<NetworkWatch>().sleep.replace(Some(subscription));
    });
}

fn call_of(model: &AppModel) -> Option<Arc<Call>> {
    model.kept::<RoomState>().call.borrow().clone()
}

/// The path as the monitor and the kernel's routing table say it now.
pub fn path_of(monitor: &gio::NetworkMonitor) -> NetworkPath {
    let online = monitor.is_network_available()
        && monitor.connectivity() != gio::NetworkConnectivity::Local;
    let route = [
        std::fs::read_to_string("/proc/net/route")
            .ok()
            .and_then(|table| default_route_v4(&table)),
        std::fs::read_to_string("/proc/net/ipv6_route")
            .ok()
            .and_then(|table| default_route_v6(&table)),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ");
    NetworkPath { online, route }
}

/// The interface and gateway of the IPv4 default route in /proc/net/route,
/// the one with the lowest metric where there are several.
fn default_route_v4(table: &str) -> Option<String> {
    table
        .lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let (iface, destination, gateway, flags, metric, mask) = (
                fields.first()?,
                fields.get(1)?,
                fields.get(2)?,
                fields.get(3)?,
                fields.get(6)?,
                fields.get(7)?,
            );
            let up = u32::from_str_radix(flags, 16).ok()? & 1 == 1;
            (up && *destination == "00000000" && *mask == "00000000").then(|| {
                (
                    metric.parse::<u32>().unwrap_or(u32::MAX),
                    format!("{iface}/{gateway}"),
                )
            })
        })
        .min()
        .map(|(_, route)| route)
}

/// The same for IPv6, in /proc/net/ipv6_route: a destination of all zeros
/// with a prefix of nought, not the loopback's.
fn default_route_v6(table: &str) -> Option<String> {
    table
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let destination = fields.first()?;
            let prefix = fields.get(1)?;
            let gateway = fields.get(4)?;
            let metric = u32::from_str_radix(fields.get(5)?, 16).ok()?;
            let iface = fields.get(9)?;
            (destination.chars().all(|c| c == '0') && *prefix == "00" && *iface != "lo")
                .then(|| (metric, format!("{iface}/{gateway}")))
        })
        .min()
        .map(|(_, route)| route)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_route_is_the_lowest_metric_one() {
        let table = "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT\n\
wlan0\t00000000\t0101A8C0\t0003\t0\t0\t600\t00000000\t0\t0\t0\n\
eth0\t00000000\t0100000A\t0003\t0\t0\t100\t00000000\t0\t0\t0\n\
eth0\t0000000A\t00000000\t0001\t0\t0\t100\t00FFFFFF\t0\t0\t0\n";
        assert_eq!(default_route_v4(table).as_deref(), Some("eth0/0100000A"));
        assert_eq!(default_route_v4("Iface\tDestination\n"), None);
    }

    #[test]
    fn the_ipv6_default_route_is_not_the_loopback() {
        let table = "00000000000000000000000000000000 00 00000000000000000000000000000000 00 fe800000000000000000000000000001 00000400 00000001 00000000 00000003 wlan0\n\
00000000000000000000000000000000 00 00000000000000000000000000000000 00 00000000000000000000000000000000 ffffffff 00000001 00000000 00200200 lo\n";
        assert_eq!(
            default_route_v6(table).as_deref(),
            Some("wlan0/fe800000000000000000000000000001")
        );
    }
}
