// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
#![cfg(all(target_os = "linux", target_env = "gnu"))]
use gz_transport::{Config, Error};
use std::{
    net::Ipv4Addr,
    path::PathBuf,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

fn artifact(variable: &str, name: &str) -> PathBuf {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("artifacts")
                .join(name)
        })
}

#[test]
fn interface_config_child() {
    if std::env::var_os("GZ_INTERFACE_CHILD").is_none() {
        return;
    }
    assert_eq!(Config::default().interface, Ipv4Addr::LOCALHOST);
    let explicit = Config {
        interface: Ipv4Addr::new(127, 0, 0, 5),
        ..Default::default()
    }
    .with_environment(|_| None)
    .unwrap();
    assert_eq!(explicit.interface, Ipv4Addr::new(127, 0, 0, 5));
    match Config::from_env() {
        Ok(config) => println!("HOST {}", config.interface),
        Err(error) => println!("ERROR {error:?}"),
    }
}

fn command(native: bool) -> Command {
    let mut command = if native {
        Command::new(artifact("GZ_INTERFACE_PEER", "interface-selection-peer"))
    } else {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", "interface_config_child", "--nocapture"]);
        command.env("GZ_INTERFACE_CHILD", "1");
        command
    };
    for (key, _) in std::env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("GZ_"))
    {
        command.env_remove(key);
    }
    if !native {
        command.env("GZ_INTERFACE_CHILD", "1");
    }
    command.env("GZ_PARTITION", "interface-fixture").env(
        "LD_PRELOAD",
        artifact("GZ_INTERFACE_INPUTS", "interface-inputs.so"),
    );
    command
}

fn output(command: &mut Command) -> Output {
    let mut child = command
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
    assert!(!expired && output.status.success(), "{output:?}");
    output
}

fn counts(output: &Output) -> Vec<usize> {
    std::str::from_utf8(&output.stderr)
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("COUNTS "))
        .expect("actual interposer counts")
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect()
}

#[test]
fn public_native_automatic_interface_corpus_preserves_selection_and_admission() {
    let fixture: gz_msgs::serde_json::Value =
        gz_msgs::serde_json::from_str(include_str!("fixtures/interface-selection.json")).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 33);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let rows = case["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row.as_str().unwrap())
            .collect::<Vec<_>>();
        let mut native = command(true);
        let mut rust = command(false);
        for command in [&mut native, &mut rust] {
            command
                .env("GZ_INTERFACE_HOST", case["hostname"].as_str().unwrap())
                .env("GZ_INTERFACE_RESOLVED", case["resolved"].as_str().unwrap())
                .env("GZ_INTERFACE_ROWS", rows.join("|"));
            if let Some(value) = case["override"].as_str() {
                command.env("GZ_IP", value);
            }
        }
        let native = output(&mut native);
        let native_text = std::str::from_utf8(&native.stdout).unwrap();
        let expected_interfaces = case["expected_interfaces"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            native_text,
            format!(
                "HOST {}\nINTERFACES {expected_interfaces}\n",
                case["expected_host"].as_str().unwrap()
            ),
            "{name}"
        );
        let [h, r, i] = <[usize; 3]>::try_from(
            case["expected_calls"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap() as usize)
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let family_rows = rows
            .iter()
            .filter(|row| !["null", "unknown"].contains(&row.splitn(3, ',').nth(2).unwrap()))
            .count();
        assert_eq!(counts(&native), [h, r, i, i, i * family_rows, i], "{name}");
        let rust = output(&mut rust);
        let rust_text = std::str::from_utf8(&rust.stdout).unwrap();
        let expected = case["rust_error"]
            .as_str()
            .map(|error| format!("ERROR {error}"))
            .unwrap_or_else(|| format!("HOST {}", case["expected_host"].as_str().unwrap()));
        assert!(
            rust_text.lines().any(|line| line == expected),
            "{name}: {rust_text}"
        );
        let automatic = case["override"].as_str().is_none_or(str::is_empty);
        let hostname = case["hostname"].as_str().unwrap();
        let hostname_calls = usize::from(automatic);
        let resolve_calls =
            usize::from(automatic && !["", "localhost", "!fail"].contains(&hostname));
        let inventory_calls = usize::from(automatic);
        assert_eq!(
            counts(&rust),
            [
                hostname_calls,
                resolve_calls,
                inventory_calls,
                0,
                inventory_calls * family_rows,
                inventory_calls
            ],
            "{name}"
        );
    }
}

#[test]
fn public_resource_errors_are_visible_and_explicit_ip_bypasses_system_setup() {
    for (mode, expected, inventory) in [
        ("range", "ERROR Unsupported(\"resolver scratch limit\")", 0),
        (
            "family",
            "ERROR Native(\"interface resolver: MalformedResult\")",
            0,
        ),
        ("again", "HOST 198.51.100.2", 1),
    ] {
        let mut child = command(false);
        let inputs = artifact("GZ_INTERFACE_INPUTS", "interface-inputs.so");
        let failures = artifact("GZ_NSS_FAILURE_LIBRARY", "nss-failure.so");
        child
            .env("GZ_INTERFACE_HOST", "fixture")
            .env("GZ_INTERFACE_ROWS", "1,4097,198.51.100.2")
            .env("GZ_NSS_STUDY_MODE", mode)
            .env(
                "LD_PRELOAD",
                format!("{}:{}", failures.display(), inputs.display()),
            );
        let result = output(&mut child);
        assert!(
            std::str::from_utf8(&result.stdout)
                .unwrap()
                .lines()
                .any(|line| line == expected),
            "{result:?}"
        );
        assert_eq!(counts(&result), [1, 0, inventory, 0, inventory, inventory]);
        child.env("GZ_IP", "127.0.0.2");
        let result = output(&mut child);
        assert!(
            std::str::from_utf8(&result.stdout)
                .unwrap()
                .lines()
                .any(|line| line == "HOST 127.0.0.2")
        );
        assert_eq!(counts(&result), [0, 0, 0, 0, 0, 0]);
    }
    for (flag, expected, freed) in [
        (
            "GZ_INTERFACE_INVENTORY_ERROR",
            format!(
                "ERROR {:?}",
                Error::Native(nix::errno::Errno::EIO.to_string())
            ),
            0,
        ),
        (
            "GZ_INTERFACE_AMBIG_NAME",
            "ERROR Unsupported(\"ambiguous interface name encoding\")".into(),
            1,
        ),
    ] {
        let result = output(
            command(false)
                .env("GZ_INTERFACE_HOST", "localhost")
                .env("GZ_INTERFACE_ROWS", "1,4097,198.51.100.2")
                .env(flag, "1"),
        );
        assert!(
            std::str::from_utf8(&result.stdout)
                .unwrap()
                .lines()
                .any(|line| line == expected),
            "{result:?}"
        );
        assert_eq!(counts(&result), [1, 0, 1, 0, 0, freed]);
    }
    assert_eq!(
        Config::default()
            .with_environment(|name| (name == "GZ_IP").then(|| "bad".into()))
            .unwrap_err(),
        Error::InvalidConfig
    );
}
