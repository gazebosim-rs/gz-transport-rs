// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn whole_second_anchor_preserves_signed_floor_and_rejects_range() {
    for (stamp, expected) in [
        (UNIX_EPOCH, 0),
        (UNIX_EPOCH + Duration::from_nanos(999_999_999), 0),
        (UNIX_EPOCH + Duration::new(13, 900_000_000), 13_000_000_000),
        (UNIX_EPOCH - Duration::from_nanos(1), -1_000_000_000),
        (UNIX_EPOCH - Duration::from_millis(100), -1_000_000_000),
        (UNIX_EPOCH - Duration::from_secs(1), -1_000_000_000),
        (UNIX_EPOCH - Duration::from_millis(1900), -2_000_000_000),
    ] {
        assert_eq!(whole_second_anchor(stamp), Ok(expected));
    }
    for stamp in [
        UNIX_EPOCH + Duration::from_secs(10_000_000_000),
        UNIX_EPOCH - Duration::from_secs(10_000_000_000),
    ] {
        assert_eq!(
            whole_second_anchor(stamp),
            Err(Error::Unsupported("wall clock anchor range"))
        );
    }
}

#[test]
fn immutable_anchor_handles_reversal_and_all_output_bounds() {
    let mono = Instant::now();
    let clock = WallClock { utc: 0, mono };
    assert!(clock.is_ready());
    assert_eq!(clock.time_at(mono), Ok(0));
    assert_eq!(clock.time_at(mono + Duration::from_nanos(1)), Ok(1));
    assert_eq!(
        clock.time_at(mono - Duration::from_nanos(1)),
        Err(Error::Native("monotonic clock moved backwards".into()))
    );
    assert_eq!(
        clock.time_at(mono + Duration::from_secs(10_000_000_000)),
        Err(Error::Unsupported("wall clock elapsed range"))
    );
    let near_limit = WallClock {
        utc: i64::MAX,
        mono,
    };
    assert_eq!(near_limit.time_at(mono), Ok(i64::MAX));
    assert_eq!(
        near_limit.time_at(mono + Duration::from_nanos(1)),
        Err(Error::Unsupported("wall clock sample range"))
    );
    assert_eq!(clock.time_at(mono), Ok(0));
    assert_eq!(near_limit.time_at(mono), Ok(i64::MAX));
}
