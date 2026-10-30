// Copyright (C) 2018, 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use gz_transport::{Error, WallClock};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

#[test]
fn wall_clock_original() {
    let clock = WallClock::new().unwrap();
    assert!(clock.is_ready());
    let start = clock.time_ns().unwrap();
    thread::sleep(Duration::from_nanos(100_000_000));
    let end = clock.time_ns().unwrap();
    assert!(end - start >= 100_000_000);
}

struct Activated(PathBuf);
impl Activated {
    fn new() -> Self {
        let path = PathBuf::from(std::env::var_os("GZ_WALL_OWNER_FILE").unwrap());
        let target = fs::read_link("/proc/thread-self").unwrap();
        let tid = target.file_name().unwrap().to_str().unwrap();
        fs::write(&path, tid).unwrap();
        Self(path)
    }
}
impl Drop for Activated {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[test]
fn wall_clock_fixture_child() {
    if std::env::var("GZ_WALL_FIXTURE").as_deref() != Ok("1") {
        return;
    }
    let active = Activated::new();
    let observed = WallClock::new().and_then(|clock| {
        let ready = clock.is_ready();
        let first = clock.time_ns()?;
        let last = clock.time_ns()?;
        Ok((ready, first, last))
    });
    // Test-harness timing runs normally on every thread. Only the actual
    // library reads on this exact test thread are inside the controlled scope.
    drop(active);
    let output = match observed {
        Ok((ready, first, last)) => {
            gz_msgs::serde_json::json!({"ready":ready,"first_ns":first,"last_ns":last})
        }
        Err(error) => gz_msgs::serde_json::json!({"error":format!("{error:?}")}),
    };
    println!("WALL {output}");
}

fn bounded_output(command: &mut Command) -> Output {
    let mut process = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while process.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            process.kill().unwrap();
            let output = process.wait_with_output().unwrap();
            panic!("wall-clock child exceeded watchdog: {output:?}");
        }
        thread::sleep(Duration::from_millis(2));
    }
    process.wait_with_output().unwrap()
}

fn fixture() -> gz_msgs::serde_json::Value {
    gz_msgs::serde_json::from_str(include_str!("fixtures/wall-clock.json")).unwrap()
}

#[test]
fn actual_sdk_replays_every_controlled_clock_observation() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let data = fixture();
    for (source, key) in [
        ("tests/wall_clock_peer.cc", "source_sha256"),
        ("tests/wall_clock_control.cc", "control_source_sha256"),
    ] {
        let hash = Command::new("sha256sum")
            .arg(root.join(source))
            .output()
            .unwrap();
        assert!(hash.status.success());
        assert_eq!(
            String::from_utf8(hash.stdout)
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap(),
            data[key].as_str().unwrap()
        );
    }
    for case in data["cases"].as_array().unwrap() {
        let input = &case["input"];
        let mut command = Command::new(root.join("artifacts/wall-clock-peer"));
        for (key, _) in
            std::env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("GZ_"))
        {
            command.env_remove(key);
        }
        command
            .arg(input["next_utc_ns"].as_i64().unwrap().to_string())
            .env("GZ_WALL_VALUES", "1")
            .env(
                "GZ_WALL_REAL_NS",
                input["utc_ns"].as_i64().unwrap().to_string(),
            )
            .env("LD_PRELOAD", root.join("artifacts/wall-clock-control.so"))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for i in 0..3 {
            command.env(
                format!("GZ_WALL_MONO_{i}"),
                input["mono_ns"][i].as_i64().unwrap().to_string(),
            );
        }
        let output = bounded_output(&mut command);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            case["stdout"].as_str().unwrap()
        );
        assert_eq!(
            String::from_utf8(output.stderr).unwrap(),
            case["stderr"].as_str().unwrap()
        );
    }
}

fn child(values: &[(&str, String)]) -> (gz_msgs::serde_json::Value, String) {
    let marker = std::env::temp_dir().join(format!(
        "gz-wall-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let mut command = Command::new(std::env::current_exe().unwrap());
    for (key, _) in std::env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("GZ_"))
    {
        command.env_remove(key);
    }
    command
        .args(["--exact", "wall_clock_fixture_child", "--nocapture"])
        .env("GZ_WALL_FIXTURE", "1")
        .env("GZ_WALL_VALUES", "1")
        .env("GZ_WALL_REPORT", "1")
        .env("GZ_WALL_OWNER_FILE", &marker)
        .env("GZ_WALL_REAL_NS", "13900000000")
        .env("GZ_WALL_NEXT_REAL_NS", "100900000000")
        .env("GZ_WALL_MONO_0", "17000000000")
        .env("GZ_WALL_MONO_1", "17012345678")
        .env("GZ_WALL_MONO_2", "17200000000")
        .env(
            "LD_PRELOAD",
            format!(
                "{}/artifacts/wall-clock-control.so",
                env!("CARGO_MANIFEST_DIR")
            ),
        )
        .env("RUST_BACKTRACE", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in values {
        command.env(key, value);
    }
    let output = bounded_output(&mut command);
    let _ = fs::remove_file(marker);
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("test wall_clock_fixture_child ... ok"),
        "{stdout}"
    );
    let records: Vec<_> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("WALL "))
        .collect();
    assert_eq!(records.len(), 1, "{stdout}");
    (
        gz_msgs::serde_json::from_str(records[0]).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

#[test]
fn controlled_std_clock_replays_native_rounding_and_realtime_step_values() {
    let data = fixture();
    for case in data["cases"].as_array().unwrap() {
        let input = &case["input"];
        let args = [
            (
                "GZ_WALL_REAL_NS",
                input["utc_ns"].as_i64().unwrap().to_string(),
            ),
            (
                "GZ_WALL_MONO_0",
                input["mono_ns"][0].as_i64().unwrap().to_string(),
            ),
            (
                "GZ_WALL_MONO_1",
                input["mono_ns"][1].as_i64().unwrap().to_string(),
            ),
            (
                "GZ_WALL_MONO_2",
                input["mono_ns"][2].as_i64().unwrap().to_string(),
            ),
            (
                "GZ_WALL_NEXT_REAL_NS",
                input["next_utc_ns"].as_i64().unwrap().to_string(),
            ),
        ];
        let (actual, stderr) = child(&args);
        assert_eq!(actual, case["rust"]);
        assert_eq!(stderr, "COUNTS 0 3 1\n");
    }
}

#[test]
fn source_failures_and_anchor_range_errors_are_visible_owned_errors() {
    for (key, value, counts) in [
        ("GZ_WALL_FAIL_REAL", "1", "COUNTS 0 0 1\n"),
        ("GZ_WALL_FAIL_MONO", "1", "COUNTS 0 1 1\n"),
        ("GZ_WALL_FAIL_MONO_AT", "1", "COUNTS 0 2 1\n"),
    ] {
        let (actual, stderr) = child(&[(key, value.into())]);
        assert_eq!(
            actual,
            gz_msgs::serde_json::json!({"error":format!("{:?}",Error::Native("wall clock source unavailable".into()))})
        );
        assert!(stderr.ends_with(counts), "{stderr}");
    }
    for seconds in [10_000_000_000_i64, -10_000_000_000] {
        let (actual, stderr) = child(&[("GZ_WALL_REAL_SEC", seconds.to_string())]);
        assert_eq!(
            actual,
            gz_msgs::serde_json::json!({"error":format!("{:?}",Error::Unsupported("wall clock anchor range"))})
        );
        assert_eq!(stderr, "COUNTS 0 0 1\n");
    }
    let (actual, stderr) = child(&[("GZ_WALL_MONO_1", "16900000000".into())]);
    assert_eq!(
        actual,
        gz_msgs::serde_json::json!({"error":format!("{:?}",Error::Native("monotonic clock moved backwards".into()))})
    );
    assert_eq!(stderr, "COUNTS 0 2 1\n");
}
