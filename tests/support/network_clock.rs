// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn missing_selected_field_preserves_each_owned_sample() {
    for base in [
        ClockTimeBase::Simulation,
        ClockTimeBase::Real,
        ClockTimeBase::System,
    ] {
        let state = AtomicI64::new(123);
        update(&state, base, &Clock::default()).unwrap();
        assert_eq!(state.load(Ordering::Acquire), 123);
        for other in [
            ClockTimeBase::Simulation,
            ClockTimeBase::Real,
            ClockTimeBase::System,
        ] {
            if other != base {
                update(&state, base, &other.message(-999)).unwrap();
                assert_eq!(state.load(Ordering::Acquire), 123);
            }
        }
    }
}

#[test]
fn receive_overflow_is_rejected_before_atomic_mutation() {
    let state = AtomicI64::new(123);
    for stamp in [
        Time {
            sec: i64::MAX,
            nsec: 0,
        },
        Time {
            sec: i64::MIN,
            nsec: 0,
        },
        Time {
            sec: i64::MAX / 1_000_000_000,
            nsec: i32::MAX,
        },
        Time {
            sec: i64::MIN / 1_000_000_000,
            nsec: i32::MIN,
        },
    ] {
        let message = Clock {
            sim: Some(stamp),
            ..Default::default()
        };
        assert_eq!(
            update(&state, ClockTimeBase::Simulation, &message),
            Err(Error::InvalidFrame)
        );
        assert_eq!(state.load(Ordering::Acquire), 123);
    }
    // Native integer addition accepts these unnormalized and mixed-sign values.
    for (stamp, expected) in [
        (Time { sec: -3, nsec: 7 }, -2_999_999_993),
        (Time { sec: 3, nsec: -7 }, 2_999_999_993),
        (
            Time {
                sec: 0,
                nsec: i32::MAX,
            },
            i64::from(i32::MAX),
        ),
        (
            Time {
                sec: 0,
                nsec: i32::MIN,
            },
            i64::from(i32::MIN),
        ),
    ] {
        update(
            &state,
            ClockTimeBase::Simulation,
            &Clock {
                sim: Some(stamp),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(state.load(Ordering::Acquire), expected);
    }
}
