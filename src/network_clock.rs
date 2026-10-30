// Copyright (C) 2018, 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use crate::{CallbackSubscription, Error, Node, Publisher};
use gz_msgs::msgs::{Clock, Time};
use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

/// Selected field of a canonical `gz.msgs.Clock` message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockTimeBase {
    Simulation,
    Real,
    System,
}

impl ClockTimeBase {
    fn selected(self, message: &Clock) -> Option<&Time> {
        match self {
            Self::Simulation => message.sim.as_ref(),
            Self::Real => message.real.as_ref(),
            Self::System => message.system.as_ref(),
        }
    }

    fn message(self, nanoseconds: i64) -> Clock {
        let stamp = Some(Time {
            // Pinned native SetTime uses an int seconds temporary. Preserve
            // its Linux 32-bit narrowing, including timestamps outside that
            // seconds range, rather than silently widening the wire value.
            sec: i64::from((nanoseconds / 1_000_000_000) as i32),
            nsec: (nanoseconds % 1_000_000_000) as i32,
        });
        let mut message = Clock::default();
        match self {
            Self::Simulation => message.sim = stamp,
            Self::Real => message.real = stamp,
            Self::System => message.system = stamp,
        }
        message
    }
}

fn update(time: &AtomicI64, base: ClockTimeBase, message: &Clock) -> Result<(), Error> {
    let Some(stamp) = base.selected(message) else {
        return Ok(());
    };
    // Native accepts signed, unnormalized nanoseconds. Only reject arithmetic
    // outside its signed nanosecond representation; no partial sample escapes.
    let value = stamp
        .sec
        .checked_mul(1_000_000_000)
        .and_then(|seconds| seconds.checked_add(i64::from(stamp.nsec)))
        .ok_or(Error::InvalidFrame)?;
    time.store(value, Ordering::Release);
    Ok(())
}

/// Owned canonical clock subscription and publication on an existing Node.
///
/// No extra IO worker or global Node is created. The Node's demand-created
/// shared callback dispatcher supplies updates. Missing selected fields leave
/// the last sample unchanged; rejected decode/arithmetic increments that Node's
/// existing routing_errors counter. Reads retain the last owned value after
/// owner retirement and do not report liveness or remote convergence.
pub struct NetworkClock {
    // Retire the callback first. Off-dispatcher Drop waits for capture exit;
    // same-dispatcher cleanup is deferred and publication can withdraw sooner.
    _subscription: CallbackSubscription,
    publisher: Publisher,
    time: Arc<AtomicI64>,
    base: ClockTimeBase,
}

impl NetworkClock {
    /// Admit a selected clock on the caller's bounded Node/domain.
    /// Duplicate publication and capacity failures preserve existing Node rules.
    pub fn new(node: &Node, topic: &str, base: ClockTimeBase) -> Result<Self, Error> {
        let time = Arc::new(AtomicI64::new(0));
        let observed = time.clone();
        let subscription = node
            .subscribe_typed_callback::<Clock, _>(topic, move |message, _| {
                update(&observed, base, message)
            })?;
        let publisher = node.advertise_typed::<Clock>(topic)?;
        Ok(Self {
            _subscription: subscription,
            publisher,
            time,
            base,
        })
    }

    /// Last selected time as one coherent signed nanosecond sample.
    pub fn time_ns(&self) -> i64 {
        self.time.load(Ordering::Acquire)
    }

    /// Native readiness is current time != 0; a later zero clears readiness.
    pub fn is_ready(&self) -> bool {
        self.time_ns() != 0
    }

    /// Publish the selected field; the sample changes through actual receipt.
    /// Success is bounded publication admission, not receipt acknowledgement.
    /// Native signed division and 32-bit seconds narrowing are preserved.
    pub fn set_time_ns(&self, nanoseconds: i64) -> Result<(), Error> {
        if !self._subscription.is_active() {
            return Err(Error::Shutdown);
        }
        self.publisher
            .publish_typed(&self.base.message(nanoseconds))
    }
}

#[cfg(test)]
#[path = "../tests/support/network_clock.rs"]
mod tests;
