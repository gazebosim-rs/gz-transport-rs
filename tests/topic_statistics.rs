// Copyright (C) 2014, 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Faithful constructor/drop originals: pinned src/TopicStatistics_TEST.cc.
use gz_transport::{Error, SampleStatistics, TopicStatistics};
use std::{
    collections::HashMap,
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn topics_statistics_constructor() {
    let stats = SampleStatistics::new();
    assert_eq!(0_u64, stats.count());
    assert_eq!(0.0, stats.avg());
    assert_eq!(0.0, stats.std_dev());
    assert_eq!(f64::MAX, stats.min());
    assert_eq!(f64::MIN_POSITIVE, stats.max());
    // Explicit generous Rust limits replace native's unbounded default map.
    let topic = TopicStatistics::new(4, 32);
    assert_eq!(0_u64, topic.dropped_msg_count());
    assert_eq!(0.0, topic.publication_statistics().avg());
    assert_eq!(0.0, topic.publication_statistics().std_dev());
    assert_eq!(f64::MAX, topic.publication_statistics().min());
    assert_eq!(f64::MIN_POSITIVE, topic.publication_statistics().max());
    assert_eq!(0.0, topic.reception_statistics().avg());
    assert_eq!(0.0, topic.reception_statistics().std_dev());
    assert_eq!(f64::MAX, topic.reception_statistics().min());
    assert_eq!(f64::MIN_POSITIVE, topic.reception_statistics().max());
}

#[test]
fn topics_statistics_dropped_msg() {
    let mut stats = TopicStatistics::new(4, 32);
    // Native reads an implicit clock; reception times are explicit here. The
    // original sender, publication stamps, sequences and predicates stay exact.
    stats.update_at(10, "foo", 1, 0).unwrap();
    stats.update_at(20, "foo", 2, 1).unwrap();
    assert_eq!(0_u64, stats.dropped_msg_count());
    stats.update_at(30, "foo", 3, 3).unwrap();
    assert_eq!(1_u64, stats.dropped_msg_count());
    stats.update_at(40, "foo", 4, 5).unwrap();
    assert_eq!(2_u64, stats.dropped_msg_count());
    stats.update_at(50, "foo", 5, 6).unwrap();
    assert_eq!(2_u64, stats.dropped_msg_count());
}

#[test]
fn explicit_time_samples_and_returned_values_are_independent() {
    let mut stats = TopicStatistics::new(1, 1);
    stats.update_at(20, "a", 10, 0).unwrap();
    stats.update_at(30, "a", 15, 1).unwrap();
    stats.update_at(38, "a", 18, 2).unwrap();
    for (sample, mean, deviation, min, max) in [
        (stats.publication_statistics(), 4.0, 1.0, 3.0, 5.0),
        (stats.reception_statistics(), 9.0, 1.0, 8.0, 10.0),
        (stats.age_statistics(), 17.5, 2.5, 15.0, 20.0),
    ] {
        assert_eq!(2, sample.count());
        assert_eq!(mean, sample.avg());
        assert_eq!(deviation, sample.std_dev());
        assert_eq!(min, sample.min());
        assert_eq!(max, sample.max());
    }
    let mut value = stats.publication_statistics();
    value.update(100.0);
    assert_eq!(3, value.count());
    assert_eq!(2, stats.publication_statistics().count());
}

#[test]
fn clones_keep_independent_history_and_configured_admission() {
    let mut original = TopicStatistics::new(2, 2);
    original.update_at(10, "a", 1, 0).unwrap();
    original.update_at(20, "a", 2, 1).unwrap();
    let mut copy = original.clone();
    original.update_at(30, "a", 3, 3).unwrap();
    copy.update_at(30, "a", 3, 2).unwrap();
    assert_eq!(1, original.dropped_msg_count());
    assert_eq!(0, copy.dropped_msg_count());
    original.update_at(40, "b", 4, 1).unwrap();
    copy.update_at(40, "c", 4, 1).unwrap();
    assert_eq!(Err(Error::QueueFull), original.update_at(50, "c", 5, 1));
    assert_eq!(Err(Error::QueueFull), copy.update_at(50, "b", 5, 1));
    assert_eq!(3, original.publication_statistics().count());
    assert_eq!(3, copy.publication_statistics().count());
}

#[test]
fn wrapping_timestamps_and_zero_sentinel_follow_native_policy() {
    let mut stats = TopicStatistics::new(1, 1);
    stats.update_at(u64::MAX, "a", u64::MAX, u64::MAX).unwrap();
    stats.update_at(0, "a", 0, 0).unwrap();
    assert_eq!(0, stats.dropped_msg_count());
    assert_eq!(1.0, stats.publication_statistics().avg());
    assert_eq!(1.0, stats.reception_statistics().avg());
    assert_eq!(0.0, stats.age_statistics().avg());
    stats.update_at(5, "a", 5, 900).unwrap();
    assert_eq!(1, stats.publication_statistics().count());
    assert_eq!(0, stats.dropped_msg_count());
    stats.update_at(4, "a", 4, 901).unwrap();
    assert_eq!(2, stats.publication_statistics().count());
    assert_eq!(u64::MAX as f64 / 2.0, stats.publication_statistics().avg());
    assert_eq!(u64::MAX as f64 / 2.0, stats.reception_statistics().avg());
    assert_eq!(0.0, stats.age_statistics().avg());
}

#[test]
fn actual_native_constructor_drop_publication_and_copy_states() {
    let input = include_str!("fixtures/topic-statistics.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| format!("{line}\n"))
        .collect::<String>();
    let path = format!(
        "{}/artifacts/topic-statistics-peer",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut child = Command::new(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let output = String::from_utf8(output.stdout).unwrap();
    assert_eq!(58, output.lines().count());
    let mut rows = output.lines();
    let mut stats = TopicStatistics::new(16, 64);
    let mut saved = HashMap::new();
    let mut name = "";
    for command in input.lines() {
        let args = command.split_whitespace().collect::<Vec<_>>();
        match args[0] {
            "RESET" => {
                stats = TopicStatistics::new(16, 64);
                name = args[1];
            }
            "ADD" => {
                let sender = if args[2] == "-" {
                    vec![]
                } else {
                    (0..args[2].len())
                        .step_by(2)
                        .map(|i| u8::from_str_radix(&args[2][i..i + 2], 16).unwrap())
                        .collect()
                };
                stats
                    .update_at(
                        args[1].parse().unwrap(),
                        sender,
                        args[3].parse().unwrap(),
                        args[4].parse().unwrap(),
                    )
                    .unwrap();
            }
            "SAVE" => {
                saved.insert(args[1], stats.clone());
            }
            "LOAD" => {
                stats = saved.get(args[1]).unwrap().clone();
                name = args[1];
            }
            _ => panic!("invalid corpus command"),
        }
        let native = rows.next().unwrap().split_whitespace().collect::<Vec<_>>();
        assert_eq!(19, native.len());
        assert_eq!("T", native[0]);
        assert_eq!(name, native[1]);
        assert_eq!(
            stats.dropped_msg_count(),
            native[3].parse::<u64>().unwrap(),
            "{command}"
        );
        for (offset, sample) in [
            (4, stats.publication_statistics()),
            (9, stats.reception_statistics()),
            (14, stats.age_statistics()),
        ] {
            assert_eq!(
                sample.count(),
                native[offset].parse::<u64>().unwrap(),
                "{command}"
            );
            for (i, value) in [sample.avg(), sample.std_dev(), sample.min(), sample.max()]
                .into_iter()
                .enumerate()
            {
                let bits = u64::from_str_radix(native[offset + 1 + i], 16).unwrap();
                // The SDK reads its hidden clock; Rust uses explicit fixture
                // time. Compare all constructor fields, and all publication
                // getters thereafter. Timing populations are checked above;
                // their numerical policy has separate deterministic controls.
                if args[0] == "RESET" || offset == 4 {
                    assert_eq!(bits, value.to_bits(), "{command} field{}", offset + 1 + i);
                }
            }
        }
    }
    assert!(rows.next().is_none());
}
