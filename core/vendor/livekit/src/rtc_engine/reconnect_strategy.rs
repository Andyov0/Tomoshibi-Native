// Copyright 2025 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Reconnect schedule.
//!
//! Tomoshibi: the web client's schedule, which this replaces the SDK's own
//! with. A native participant gave up on a broken connection in about 22 s
//! (ten attempts, each followed by a wait drawn from `[0, min(7 s, 0.3 s *
//! 2^(n-1))]`, the last attempt's wait included), where a browser on the same
//! network keeps trying for about 45 s: a lift, a tunnel or a Wi-Fi to mobile
//! hand-over of twenty to forty seconds dropped the app from the call and not
//! the browser beside it. The numbers are livekit-client's
//! `DefaultReconnectPolicy` as the deployment's web client is built with it
//! (livekit-client 2.21.0, `[0, 300, 4*300, 9*300, 16*300, 7000 x 5]` ms, and
//! up to a second more at random before every attempt after the second; read
//! out of the built bundle, as the web's `node_modules` was not at hand). The
//! wait comes before each attempt, as the web's does, so the first attempt is
//! immediate and nothing is waited for after the last.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How many attempts are made before the engine gives up and closes.
pub const RECONNECT_ATTEMPTS: u32 = 10;

/// The wait before each attempt, in order: livekit-client's
/// `DEFAULT_RETRY_DELAYS_IN_MS`.
pub const RECONNECT_DELAYS: [Duration; RECONNECT_ATTEMPTS as usize] = [
    Duration::from_millis(0),
    Duration::from_millis(300),
    Duration::from_millis(1200),
    Duration::from_millis(2700),
    Duration::from_millis(4800),
    Duration::from_millis(7000),
    Duration::from_millis(7000),
    Duration::from_millis(7000),
    Duration::from_millis(7000),
    Duration::from_millis(7000),
];

/// The most added at random to a wait, from the third attempt on
/// (`nextRetryDelayInMs`: `retryCount <= 1 ? delay : delay + Math.random() * 1000`),
/// so that a fleet of clients cut off together does not come back in step.
pub const RECONNECT_JITTER: Duration = Duration::from_secs(1);

/// The wait before 1-based `attempt`, with `unit` the random draw in
/// `[0, 1)`. Beyond the last attempt there is nothing to wait for.
pub fn wait_before(attempt: u32, unit: f64) -> Duration {
    let Some(delay) = attempt.checked_sub(1).and_then(|at| RECONNECT_DELAYS.get(at as usize)) else {
        return Duration::ZERO;
    };
    if attempt <= 2 {
        return *delay;
    }
    *delay + RECONNECT_JITTER.mul_f64(unit.clamp(0.0, 1.0))
}

/// [`wait_before`] with a draw from the clock: the jitter only needs to
/// differ between clients, not to be unpredictable.
pub(super) fn delay(attempt: u32) -> Duration {
    let nanos =
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    wait_before(attempt, f64::from(nanos) / 1e9)
}

// The schedule's tests are in the core (core/tests/reconnect_schedule.rs):
// this crate's own cannot run where it is vendored.
