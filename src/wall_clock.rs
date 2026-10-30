// Copyright (C) 2018, 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use crate::Error;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn whole_second_anchor(value: SystemTime) -> Result<i64, Error> {
    // Duration's u64 seconds and subsecond nanos fit exactly in i128. Euclidean
    // division floors pre-epoch fractions as native time(NULL) does.
    let nanos = |value: Duration| {
        i128::from(value.as_secs()) * 1_000_000_000 + i128::from(value.subsec_nanos())
    };
    let value = match value.duration_since(UNIX_EPOCH) {
        Ok(value) => nanos(value),
        Err(error) => -nanos(error.duration()),
    };
    i64::try_from(value.div_euclid(1_000_000_000) * 1_000_000_000)
        .map_err(|_| Error::Unsupported("wall clock anchor range"))
}

fn read<T>(clock: fn() -> T) -> Result<T, Error> {
    // These private readers are only std clock functions, never user callbacks.
    // std reports an OS clock-read failure by unwinding. Preserve a fallible
    // API without replacing the process-wide panic hook or reading via FFI.
    std::panic::catch_unwind(clock)
        .map_err(|_| Error::Native("wall clock source unavailable".into()))
}

/// Owned whole-second UTC anchor followed by monotonic elapsed nanoseconds.
///
/// This preserves native WallClock values without a global singleton, Node,
/// thread or endpoint. Construction samples realtime before monotonic time;
/// subsequent reads do not follow realtime adjustments. Readiness means this
/// initialized clock exists, including when its time is zero, not liveness or
/// an assertion that a future read cannot fail.
pub struct WallClock {
    utc: i64,
    mono: Instant,
}

impl WallClock {
    /// Sample the native whole-second UTC anchor, then monotonic time.
    /// No invalid/default-zero object or silent saturation is constructed.
    pub fn new() -> Result<Self, Error> {
        let utc = whole_second_anchor(read(SystemTime::now)?)?;
        let mono = read(Instant::now)?;
        Ok(Self { utc, mono })
    }

    /// Native WallClock is always initialized/ready, even at zero time.
    pub fn is_ready(&self) -> bool {
        true
    }

    /// Read signed nanoseconds from the immutable anchor and monotonic elapsed.
    /// Reversed monotonic reads and unrepresentable values return errors;
    /// neither clamps to zero nor updates the anchor.
    pub fn time_ns(&self) -> Result<i64, Error> {
        self.time_at(read(Instant::now)?)
    }

    fn time_at(&self, now: Instant) -> Result<i64, Error> {
        let elapsed = now
            .checked_duration_since(self.mono)
            .ok_or_else(|| Error::Native("monotonic clock moved backwards".into()))?;
        let elapsed = i64::try_from(elapsed.as_nanos())
            .map_err(|_| Error::Unsupported("wall clock elapsed range"))?;
        self.utc
            .checked_add(elapsed)
            .ok_or(Error::Unsupported("wall clock sample range"))
    }
}

#[cfg(test)]
#[path = "../tests/support/wall_clock.rs"]
mod tests;
