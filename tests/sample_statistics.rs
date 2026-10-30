// Copyright (C) 2014, 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Faithful scalar originals: src/TopicStatistics_TEST.cc, pinned Gazebo source.
use gz_transport::SampleStatistics;
use std::{
    fmt::Write as _,
    io::Write,
    process::{Command, Stdio},
    thread,
};

#[test]
fn topics_statistics_min_max() {
    let mut stats = SampleStatistics::new();
    stats.update(1.0);
    assert_eq!(1.0, stats.min());
    assert_eq!(1.0, stats.max());
    stats.update(2.0);
    assert_eq!(1.0, stats.min());
    assert_eq!(2.0, stats.max());
    stats.update(0.1);
    assert_eq!(0.1, stats.min());
    assert_eq!(2.0, stats.max());
}

#[test]
fn topics_statistics_avg_std_dev() {
    let mut stats = SampleStatistics::new();
    stats.update(1.0);
    assert_eq!(1_u64, stats.count());
    assert_eq!(1.0, stats.avg());
    assert_eq!(0.0, stats.std_dev());
    stats.update(2.0);
    assert_eq!(2_u64, stats.count());
    assert_eq!(1.5, stats.avg());
    assert_eq!(0.5, stats.std_dev());
    stats.update(3.0);
    assert_eq!(3_u64, stats.count());
    assert_eq!(2.0, stats.avg());
    assert!((stats.std_dev() - 0.816).abs() <= 1e-3);
}

#[test]
fn scalar_defaults_extrema_nonfinite_and_owned_copies() {
    let empty = SampleStatistics::default();
    assert_eq!(0, empty.count());
    assert_eq!(0.0, empty.avg());
    assert_eq!(0.0, empty.std_dev());
    assert_eq!(f64::MAX, empty.min());
    assert_eq!(f64::MIN_POSITIVE, empty.max());
    let mut negative = empty;
    for value in [-0.0, 0.0, -2.0, f64::NAN] {
        negative.update(value);
    }
    assert_eq!(4, negative.count());
    assert_eq!(-2.0, negative.min());
    assert_eq!(f64::MIN_POSITIVE, negative.max());
    assert!(negative.avg().is_nan());
    assert!(negative.std_dev().is_nan());
    assert_eq!(0, empty.count());
    assert_eq!(f64::MAX, empty.min());
    let mut signed = empty;
    signed.update(-0.0);
    signed.update(0.0);
    assert_eq!((-0.0_f64).to_bits(), signed.min().to_bits());
    let mut copied = signed;
    copied.update(-1.0);
    assert_eq!(-1.0, copied.min());
    assert_eq!((-0.0_f64).to_bits(), signed.min().to_bits());
}

#[test]
fn population_deviation_keeps_native_underflow_boundary() {
    let mut stats = SampleStatistics::new();
    for bits in [0x9e70000000000000, 0x9e50000000000000, 0] {
        stats.update(f64::from_bits(bits));
    }
    assert_eq!(3, stats.count());
    assert_eq!(0x1e60000000000000, stats.std_dev().to_bits());
}

fn compare_native(input: &str, expected_states: usize) {
    let path = format!(
        "{}/artifacts/sample-statistics-peer",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut child = Command::new(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Read output concurrently: the long corpus exceeds a pipe's capacity.
    let mut stdin = child.stdin.take().unwrap();
    let owned_input = input.as_bytes().to_owned();
    let writer = thread::spawn(move || stdin.write_all(&owned_input).unwrap());
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    assert!(
        output.status.success(),
        "{:?}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert_eq!(expected_states, text.lines().count());
    let mut stats = SampleStatistics::new();
    let mut rows = text.lines();
    for line in input.lines() {
        let (command, value) = line.split_once(' ').unwrap();
        match command {
            "RESET" => stats = SampleStatistics::new(),
            "ADD" => stats.update(f64::from_bits(u64::from_str_radix(value, 16).unwrap())),
            _ => panic!("unexpected corpus command"),
        }
        let native = rows.next().unwrap().split_whitespace().collect::<Vec<_>>();
        assert_eq!("S", native[0]);
        if command == "RESET" {
            assert_eq!(value, native[1]);
            assert_eq!("0", native[2]);
            assert_eq!("-", native[3]);
        } else {
            assert_eq!(value, native[3]);
            assert_eq!(stats.count().to_string(), native[2]);
        }
        assert_eq!(stats.count(), native[4].parse::<u64>().unwrap(), "{line}");
        for (index, actual) in [
            (5, stats.avg()),
            (6, stats.std_dev()),
            (7, stats.min()),
            (8, stats.max()),
        ] {
            let bits = u64::from_str_radix(native[index], 16).unwrap();
            assert_eq!(
                bits,
                actual.to_bits(),
                "{} step{} field{index}",
                native[1],
                native[2]
            );
        }
    }
    assert!(rows.next().is_none());
}

#[test]
fn retained_native_corpus_and_reduced_library_counterexamples() {
    let mut input = include_str!("fixtures/sample-statistics.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| format!("{line}\n"))
        .collect::<String>();
    let a = 0x9e70000000000000_u64;
    let b = 0x9e50000000000000_u64;
    for (name, values) in [
        ("streaming", vec![a, b, 0]),
        ("average", vec![a, a, b]),
        ("welford", vec![a, a, a, b]),
        ("rolling", vec![a, b]),
    ] {
        writeln!(input, "RESET reduced_{name}").unwrap();
        for bits in values {
            writeln!(input, "ADD {bits:016x}").unwrap();
        }
    }
    compare_native(&input, 604 + 4 + 3 + 3 + 4 + 2);
}

#[test]
fn long_adversarial_native_families() {
    let mut input = String::new();
    for name in [
        "repeat_decimal",
        "alternating_wire",
        "monotonic",
        "near_constant",
        "reciprocals",
        "subnormal_variance",
        "subnormal_samples",
        "alternating_max",
        "late_nan",
        "late_infinity",
    ] {
        writeln!(input, "RESET {name}").unwrap();
        for i in 0..4096 {
            let value = match name {
                "repeat_decimal" => 0.1,
                "alternating_wire" => {
                    if i % 2 == 0 {
                        (i % 17) as f64
                    } else {
                        u64::MAX as f64
                    }
                }
                "monotonic" => i as f64,
                "near_constant" => 1e12 + (i % 7) as f64 * 0.125,
                "reciprocals" => 1.0 / (i + 1) as f64,
                "subnormal_variance" => {
                    if i % 2 == 0 {
                        -1e-160
                    } else {
                        1e-160
                    }
                }
                "subnormal_samples" => f64::from_bits(1) * (i % 7) as f64,
                "alternating_max" => {
                    if i % 2 == 0 {
                        -f64::MAX
                    } else {
                        f64::MAX
                    }
                }
                "late_nan" => {
                    if i == 1000 {
                        f64::NAN
                    } else {
                        i as f64
                    }
                }
                "late_infinity" => {
                    if i == 1000 {
                        f64::INFINITY
                    } else {
                        i as f64
                    }
                }
                _ => unreachable!(),
            };
            writeln!(input, "ADD {:016x}", value.to_bits()).unwrap();
        }
    }
    for reverse in [false, true] {
        writeln!(input, "RESET power_{reverse}").unwrap();
        for i in 0..1075 {
            let exponent = if reverse { i - 1074 } else { -i };
            let bits = if exponent < -1022 {
                1_u64 << (exponent + 1074)
            } else {
                ((exponent + 1023) as u64) << 52
            };
            writeln!(input, "ADD {bits:016x}").unwrap();
        }
    }
    compare_native(&input, 43_122);
}
