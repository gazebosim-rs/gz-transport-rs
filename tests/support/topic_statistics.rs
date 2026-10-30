// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use super::*;

fn assert_same(actual: &TopicStatistics, before: &TopicStatistics) {
    assert_eq!(actual.sequences, before.sequences);
    assert_eq!(actual.sender_bytes, before.sender_bytes);
    assert_eq!(actual.max_senders, before.max_senders);
    assert_eq!(actual.max_sender_bytes, before.max_sender_bytes);
    assert_eq!(actual.dropped, before.dropped);
    assert_eq!(actual.previous_publication, before.previous_publication);
    assert_eq!(actual.previous_reception, before.previous_reception);
    for (a, b) in [
        (actual.publication, before.publication),
        (actual.reception, before.reception),
        (actual.age, before.age),
    ] {
        assert_eq!(a.count(), b.count());
        assert_eq!(a.avg().to_bits(), b.avg().to_bits());
        assert_eq!(a.std_dev().to_bits(), b.std_dev().to_bits());
        assert_eq!(a.min().to_bits(), b.min().to_bits());
        assert_eq!(a.max().to_bits(), b.max().to_bits());
    }
}

#[test]
fn cardinality_and_byte_failures_precede_all_mutation() {
    for (limits, rejected) in [((1, 8), "b"), ((8, 1), "bb")] {
        let mut stats = TopicStatistics::new(limits.0, limits.1);
        stats.update_at(10, "a", 1, 0).unwrap();
        stats.update_at(20, "a", 2, 1).unwrap();
        let before = stats.clone();
        assert_eq!(
            Err(Error::QueueFull),
            stats.update_at(10000, rejected, 999, 500)
        );
        assert_same(&stats, &before);
        stats.update_at(30, "a", 3, 2).unwrap();
        assert_eq!(0, stats.dropped_msg_count());
        assert_eq!(2, stats.publication_statistics().count());
        assert_eq!(1.0, stats.publication_statistics().avg());
    }
}

#[test]
fn retained_budget_is_total_and_known_sender_storage_is_reused() {
    let mut stats = TopicStatistics::new(8, 3);
    stats.update_at(10, "a", 1, 0).unwrap();
    stats.update_at(20, "b", 2, 1).unwrap();
    let before = stats.clone();
    assert_eq!(Err(Error::QueueFull), stats.update_at(999, "cc", 999, 999));
    assert_same(&stats, &before);
    stats.update_at(30, "c", 3, 1).unwrap();
    let sender = stats
        .sequences
        .get_key_value(b"a".as_slice())
        .unwrap()
        .0
        .as_ptr();
    for sequence in 1..100 {
        stats
            .update_at(30 + sequence, "a", 3 + sequence, sequence)
            .unwrap();
    }
    assert_eq!(3, stats.sender_bytes);
    assert_eq!(3, stats.sequences.len());
    assert_eq!(
        sender,
        stats
            .sequences
            .get_key_value(b"a".as_slice())
            .unwrap()
            .0
            .as_ptr()
    );
}

#[test]
fn zero_limits_and_exact_binary_sender_keys_remain_explicit() {
    let mut disabled = TopicStatistics::new(0, usize::MAX);
    let before = disabled.clone();
    assert_eq!(Err(Error::QueueFull), disabled.update_at(1, [], 1, 0));
    assert_same(&disabled, &before);
    let mut empty_only = TopicStatistics::new(2, 0);
    empty_only.update_at(1, [], 1, 0).unwrap();
    let before = empty_only.clone();
    assert_eq!(Err(Error::QueueFull), empty_only.update_at(2, "a", 2, 1));
    assert_same(&empty_only, &before);
    empty_only.update_at(2, [], 2, 1).unwrap();
    let mut bytes = TopicStatistics::new(4, 4);
    for (i, sender) in [b"a".as_slice(), b"a\0".as_slice(), &[0xff]]
        .into_iter()
        .enumerate()
    {
        bytes.update_at(i as u64, sender, i as u64 + 1, 1).unwrap();
    }
    assert_eq!(3, bytes.sequences.len());
    assert_eq!(4, bytes.sender_bytes);
    let copy = bytes.clone();
    assert_ne!(
        bytes
            .sequences
            .get_key_value(b"a".as_slice())
            .unwrap()
            .0
            .as_ptr(),
        copy.sequences
            .get_key_value(b"a".as_slice())
            .unwrap()
            .0
            .as_ptr()
    );
}

#[test]
fn arithmetic_guards_and_drop_counter_wrap_are_source_boundaries() {
    let mut stats = TopicStatistics::new(2, usize::MAX);
    stats.update_at(1, "a", 1, 0).unwrap();
    // Synthetic byte-counter overflow avoids impossible allocations; failures
    // still must preserve all observable and internal state.
    stats.sender_bytes = usize::MAX;
    let before = stats.clone();
    assert_eq!(Err(Error::QueueFull), stats.update_at(2, "b", 2, 5));
    assert_same(&stats, &before);
    stats.dropped = u64::MAX;
    stats.update_at(2, "a", 2, 5).unwrap();
    assert_eq!(0, stats.dropped_msg_count());
}

#[test]
fn metric_nonfinite_scalar_values_preserve_generated_double_encoding() {
    let mut stats = TopicStatistics::new(0, 0);
    // Synthetic scalar state checks the formatting boundary independently of
    // the public topic input's finite unsigned-millisecond domain.
    stats.publication.update(f64::NAN);
    stats.reception.update(f64::INFINITY);
    stats.age.update(f64::NEG_INFINITY);
    let before = stats.clone();
    let mut metric = Metric::default();
    stats.fill_metric(&mut metric, usize::MAX).unwrap();
    assert_same(&stats, &before);
    assert!(metric.statistics_groups[0].statistics[0].value.is_nan());
    assert_eq!(
        0.0_f64.to_bits(),
        metric.statistics_groups[1].statistics[0].value.to_bits()
    );
    assert_eq!(
        f64::NEG_INFINITY,
        metric.statistics_groups[2].statistics[0].value
    );
    let bytes = metric.encode_to_vec();
    let decoded = Metric::decode(bytes.as_slice()).unwrap();
    for (expected, actual) in metric
        .statistics_groups
        .iter()
        .zip(&decoded.statistics_groups)
    {
        for (expected, actual) in expected.statistics.iter().zip(&actual.statistics) {
            assert_eq!(expected.value.to_bits(), actual.value.to_bits());
        }
    }
}
