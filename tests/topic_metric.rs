// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use gz_msgs::{
    msgs::{Header, Metric, Statistic, StatisticsGroup, Time, statistic::DataType},
    prost::Message,
};
use gz_transport::{Error, TopicStatistics};
use std::process::Command;

fn samples() -> TopicStatistics {
    let mut stats = TopicStatistics::new(4, 32);
    for (received, stamp, sequence) in [(20, 10, 0), (30, 15, 1), (38, 18, 3)] {
        stats.update_at(received, "foo", stamp, sequence).unwrap();
    }
    stats
}

fn stat(kind: DataType, name: &str, value: f64) -> Statistic {
    Statistic {
        r#type: kind.into(),
        name: name.into(),
        value,
        ..Default::default()
    }
}

fn prefix() -> Metric {
    Metric {
        header: Some(Header {
            stamp: Some(Time { sec: 17, nsec: 18 }),
            ..Default::default()
        }),
        unit: "seconds".into(),
        statistics: vec![stat(DataType::Average, "keep", 42.0)],
        statistics_groups: vec![StatisticsGroup {
            name: "keep_group".into(),
            statistics: vec![stat(DataType::Maximum, "pre", 7.0)],
            ..Default::default()
        }],
    }
}

#[test]
fn defaults_preserve_native_nonfinite_values_labels_order_and_wire() {
    let stats = TopicStatistics::new(0, 0);
    let mut metric = Metric::default();
    stats.fill_metric(&mut metric, usize::MAX).unwrap();
    assert!(metric.header.is_none());
    assert_eq!("milliseconds", metric.unit);
    assert_eq!(1, metric.statistics.len());
    assert_eq!(
        stat(DataType::SampleCount, "dropped_message_count", 0.0),
        metric.statistics[0]
    );
    assert_eq!(3, metric.statistics_groups.len());
    for (index, group) in metric.statistics_groups.iter().enumerate() {
        assert!(group.header.is_none());
        assert_eq!(
            [
                "publication_statistics",
                "reception_statistics",
                "age_statistics"
            ][index],
            group.name
        );
        assert_eq!(4, group.statistics.len());
        let names = if index < 2 {
            [
                "avg_hz",
                "min_period",
                "max_period",
                "period_standard_devation",
            ]
        } else {
            ["avg_age", "min_age", "max_age", "age_standard_devation"]
        };
        let values = [
            if index < 2 { f64::INFINITY } else { 0.0 },
            f64::MAX,
            f64::MIN_POSITIVE,
            0.0,
        ];
        for (i, value) in group.statistics.iter().enumerate() {
            assert!(value.header.is_none());
            assert_eq!(
                [
                    DataType::Average,
                    DataType::Minimum,
                    DataType::Maximum,
                    DataType::Stddev
                ][i] as i32,
                value.r#type
            );
            assert_eq!(names[i], value.name);
            assert_eq!(values[i].to_bits(), value.value.to_bits());
        }
    }
    let wire = metric.encode_to_vec();
    assert_eq!(395, wire.len());
    assert_eq!(
        wire,
        Metric::decode(wire.as_slice()).unwrap().encode_to_vec()
    );
    // A real zero period also yields the native positive infinite frequency.
    let mut zero_period = TopicStatistics::new(1, 1);
    zero_period.update_at(1, "a", 1, 0).unwrap();
    zero_period.update_at(1, "a", 1, 1).unwrap();
    zero_period.fill_metric(&mut metric, usize::MAX).unwrap();
    assert_eq!(
        f64::INFINITY,
        metric.statistics_groups[3].statistics[0].value
    );
    assert_eq!(
        f64::INFINITY,
        metric.statistics_groups[4].statistics[0].value
    );
}

#[test]
fn final_encoded_budget_is_atomic_for_empty_prefixed_and_repeated_messages() {
    let stats = samples();
    for mut original in [Metric::default(), prefix()] {
        for repeat in 0..3 {
            let before = original.encode_to_vec();
            let mut expected = original.clone();
            stats.fill_metric(&mut expected, usize::MAX).unwrap();
            let size = expected.encoded_len();
            for budget in [0, size - 1] {
                assert_eq!(
                    Err(Error::QueueFull),
                    stats.fill_metric(&mut original, budget)
                );
                assert_eq!(before, original.encode_to_vec());
            }
            stats.fill_metric(&mut original, size).unwrap();
            assert_eq!(expected.encode_to_vec(), original.encode_to_vec());
            assert_eq!(size, original.encoded_len());
            assert_eq!(
                (repeat + 1) * 3 + usize::from(original.header.is_some()),
                original.statistics_groups.len()
            );
        }
    }
    for old_unit_bytes in [0, 1, 127, 128, 4096] {
        let mut metric = prefix();
        metric.unit = "u".repeat(old_unit_bytes);
        metric.statistics[0].name = "n".repeat(128);
        metric.statistics[0].header = metric.header.clone();
        metric.statistics_groups[0].header = metric.header.clone();
        metric.statistics_groups[0].statistics[0].header = metric.header.clone();
        metric.statistics_groups[0].statistics[0].value = -0.0;
        let header = metric.header.clone();
        let old_stat = metric.statistics[0].clone();
        let old_group = metric.statistics_groups[0].clone();
        let mut expected = metric.clone();
        stats.fill_metric(&mut expected, usize::MAX).unwrap();
        let before = metric.encode_to_vec();
        assert_eq!(
            Err(Error::QueueFull),
            stats.fill_metric(&mut metric, expected.encoded_len() - 1)
        );
        assert_eq!(before, metric.encode_to_vec());
        stats
            .fill_metric(&mut metric, expected.encoded_len())
            .unwrap();
        assert_eq!(expected.encode_to_vec(), metric.encode_to_vec());
        assert_eq!(header, metric.header);
        assert_eq!(old_stat, metric.statistics[0]);
        assert_eq!(old_group, metric.statistics_groups[0]);
        assert_eq!(
            old_group.statistics[0].value.to_bits(),
            metric.statistics_groups[0].statistics[0].value.to_bits()
        );
    }
}

#[test]
fn populated_values_append_without_changing_samples_or_prefix() {
    let stats = samples();
    let mut metric = prefix();
    stats.fill_metric(&mut metric, usize::MAX).unwrap();
    assert_eq!(1.0, metric.statistics[1].value);
    for (group, expected) in metric.statistics_groups[1..].iter().zip([
        [250.0_f64, 3.0, 5.0, 1.0],
        [1000.0 / 9.0, 8.0, 10.0, 1.0],
        [17.5, 15.0, 20.0, 2.5],
    ]) {
        for (statistic, expected) in group.statistics.iter().zip(expected) {
            assert_eq!(expected.to_bits(), statistic.value.to_bits());
        }
    }
    assert_eq!(2, stats.publication_statistics().count());
    assert_eq!(1, stats.dropped_msg_count());
    stats.fill_metric(&mut metric, usize::MAX).unwrap();
    assert_eq!(metric.statistics[1], metric.statistics[2]);
    assert_eq!(
        metric.statistics_groups[1..4],
        metric.statistics_groups[4..7]
    );
}

#[test]
fn controlled_clock_native_fill_message_matches_all_four_complete_wires() {
    let root = env!("CARGO_MANIFEST_DIR");
    // Interposition is confined to this pure value helper child: no Node,
    // Rust process, unchanged original or network peer receives LD_PRELOAD.
    let result = Command::new(format!("{root}/artifacts/topic-metric-peer"))
        .env(
            "LD_PRELOAD",
            format!("{root}/artifacts/topic-metric-clock.so"),
        )
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stderr.is_empty());
    let output = String::from_utf8(result.stdout).unwrap();
    let cases = output
        .lines()
        .filter(|line| line.starts_with("CASE "))
        .map(|line| line.split_whitespace().nth(1).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        [
            "controlled_empty",
            "controlled_populated",
            "controlled_prefix",
            "controlled_repeated"
        ],
        cases.as_slice()
    );
    let native = output
        .lines()
        .filter_map(|line| line.strip_prefix("WIRE "))
        .collect::<Vec<_>>();
    assert_eq!(4, native.len());
    let empty = TopicStatistics::new(4, 32);
    let stats = samples();
    let mut values = [
        Metric::default(),
        Metric::default(),
        prefix(),
        Metric::default(),
    ];
    empty.fill_metric(&mut values[0], usize::MAX).unwrap();
    for value in &mut values[1..] {
        stats.fill_metric(value, usize::MAX).unwrap();
    }
    stats.fill_metric(&mut values[3], usize::MAX).unwrap();
    for (value, native) in values.iter().zip(native) {
        let wire = value.encode_to_vec();
        let hex = wire
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(native, hex);
        let decoded = Metric::decode(wire.as_slice()).unwrap();
        assert_eq!(wire, decoded.encode_to_vec());
    }
}
