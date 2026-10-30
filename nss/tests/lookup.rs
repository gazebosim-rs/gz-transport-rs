// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
#![cfg(all(target_os = "linux", target_env = "gnu"))]
use gz_transport_nss::lookup_first_v4;
use std::{
    ffi::CString,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

#[test]
fn lookup_child() {
    let Ok(names) = std::env::var("GZ_NSS_CHILD_NAMES") else {
        return;
    };
    let retained = names
        .split('|')
        .map(|name| lookup_first_v4(&CString::new(name).unwrap()))
        .collect::<Vec<_>>();
    for result in retained {
        println!("RESULT {result:?}");
    }
}

fn child(mode: &str, names: &str) -> Output {
    let library = std::env::var_os("GZ_NSS_FAILURE_LIBRARY")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../artifacts/nss-failure.so")
        });
    assert!(
        library.is_file(),
        "missing required child interposer: {}",
        library.display()
    );
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "lookup_child", "--nocapture"])
        .env("GZ_NSS_CHILD_NAMES", names)
        .env("GZ_NSS_STUDY_MODE", mode)
        .env("LD_PRELOAD", library)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let expired = loop {
        if child.try_wait().unwrap().is_some() {
            break false;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            break true;
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    let output = child.wait_with_output().unwrap();
    assert!(
        !expired && output.status.success(),
        "child failed: {output:?}"
    );
    output
}

fn results(output: &Output) -> Vec<&str> {
    std::str::from_utf8(&output.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| line.strip_prefix("RESULT "))
        .collect()
}

#[test]
fn injected_results_preserve_bounds_errors_and_first_record_only() {
    for (mode, expected, lengths) in [
        ("valid", "Ok(Some(203.0.113.9))", vec![1024]),
        ("retry", "Ok(Some(203.0.113.9))", vec![1024, 2048, 4096]),
        (
            "range",
            "Err(BufferLimit)",
            vec![1024, 2048, 4096, 8192, 16384, 32768, 65536],
        ),
        (
            "range-other-error",
            "Err(Lookup { code: 34, host_error: 2 })",
            vec![1024],
        ),
        (
            "again",
            "Err(Lookup { code: 11, host_error: 2 })",
            vec![1024],
        ),
        (
            "memory",
            "Err(Lookup { code: 12, host_error: -1 })",
            vec![1024],
        ),
        ("absent", "Ok(None)", vec![1024]),
        ("family", "Err(MalformedResult)", vec![1024]),
        ("length", "Err(MalformedResult)", vec![1024]),
        ("null-list", "Err(MalformedResult)", vec![1024]),
        ("null-first", "Ok(None)", vec![1024]),
        ("foreign-result", "Err(MalformedResult)", vec![1024]),
        ("foreign-list", "Err(MalformedResult)", vec![1024]),
        ("list-tail", "Err(MalformedResult)", vec![1024]),
        ("foreign-address", "Err(MalformedResult)", vec![1024]),
        ("address-tail", "Err(MalformedResult)", vec![1024]),
        ("unaligned", "Ok(Some(203.0.113.9))", vec![1024]),
        ("later-malformed", "Ok(Some(203.0.113.9))", vec![1024]),
    ] {
        let output = child(mode, "fixture");
        assert_eq!(results(&output), [expected], "{mode}");
        let expected_calls = lengths
            .into_iter()
            .map(|length| format!("CALL {length}\n"))
            .collect::<String>();
        assert_eq!(
            std::str::from_utf8(&output.stderr).unwrap(),
            expected_calls,
            "{mode}"
        );
    }
}

#[test]
fn input_limit_precedes_ffi_and_copied_values_outlive_subsequent_calls() {
    for (length, expected, calls) in [
        (1023, "Ok(Some(203.0.113.9))", "CALL 1024\n"),
        (1024, "Err(NameTooLong)", ""),
    ] {
        let output = child("valid", &"a".repeat(length));
        assert_eq!(results(&output), [expected]);
        assert_eq!(std::str::from_utf8(&output.stderr).unwrap(), calls);
    }
    let output = child("alternating", "first|second");
    assert_eq!(
        results(&output),
        ["Ok(Some(203.0.113.1))", "Ok(Some(203.0.113.2))"]
    );
    assert_eq!(
        std::str::from_utf8(&output.stderr).unwrap(),
        "CALL 1024\nCALL 1024\n"
    );
}

#[test]
fn real_native_numeric_first_a_and_absence() {
    assert_eq!(
        lookup_first_v4(c"127.1").unwrap(),
        Some(std::net::Ipv4Addr::LOCALHOST)
    );
    assert_eq!(lookup_first_v4(c"999.0.0.1").unwrap(), None);
}
