// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn unsigned_native_count_wrap_and_zero_denominator_are_explicit() {
    let mut statistics = SampleStatistics::new();
    statistics.update(3.0);
    // This synthetic state checks the source-defined unsigned boundary. We do
    // not claim to have executed 2^64 native updates or modified its private ABI.
    statistics.count = u64::MAX;
    statistics.update(5.0);
    assert_eq!(0, statistics.count());
    assert_eq!(f64::INFINITY, statistics.avg());
    assert_eq!(0.0, statistics.std_dev());
    assert_eq!(3.0, statistics.min());
    assert_eq!(5.0, statistics.max());
    statistics.update(6.0);
    assert_eq!(1, statistics.count());
    assert!(statistics.avg().is_nan());
    assert!(statistics.std_dev().is_nan());
}

#[test]
fn count_does_not_round_trip_through_usize() {
    let mut statistics = SampleStatistics::new();
    // A synthetic count checks integer width without claiming that billions of
    // native updates have been performed.
    statistics.count = u64::from(u32::MAX);
    statistics.update(1.0);
    assert_eq!(statistics.count(), 1_u64 << 32);
}
