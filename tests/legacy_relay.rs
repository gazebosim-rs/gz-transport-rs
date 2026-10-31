// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use gz_transport::{Config, Node};
use socket2::{Domain, Protocol, Socket, Type};
use std::{
    net::{Ipv4Addr, SocketAddrV4},
    path::PathBuf,
    process::{Command, Output},
};

fn artifact(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("artifacts")
        .join(name)
}
fn child_command(executable: impl AsRef<std::ffi::OsStr>, value: Option<&str>) -> Command {
    let mut command = Command::new(executable);
    for name in [
        "GZ_RELAY",
        "GZ_IP",
        "GZ_PARTITION",
        "GZ_DISCOVERY_MULTICAST_IP",
        "GZ_DISCOVERY_MSG_PORT",
        "GZ_DISCOVERY_SRV_PORT",
        "GZ_TRANSPORT_SNDHWM",
        "GZ_TRANSPORT_RCVHWM",
        "GZ_TRANSPORT_TOPIC_STATISTICS",
        "GZ_TRANSPORT_USERNAME",
        "GZ_TRANSPORT_PASSWORD",
        "GZ_TRANSPORT_IMPLEMENTATION",
        "LD_PRELOAD",
        "GZ_LEGACY_CHILD",
        "GZ_LEGACY_FAIL_BROADCAST",
        "GZ_LEGACY_CAPTURE_READY",
    ] {
        command.env_remove(name);
    }
    command
        .env("GZ_IP", "127.0.0.1")
        .env("GZ_PARTITION", uuid::Uuid::new_v4().to_string());
    if let Some(value) = value {
        command.env("GZ_RELAY", value);
    }
    command
}
fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
#[test]
fn legacy_relay_child() {
    let Ok(mode) = std::env::var("GZ_LEGACY_CHILD") else {
        return;
    };
    let config = Config::from_env().unwrap();
    println!(
        "LEGACY {}",
        config
            .legacy_discovery_relays
            .addresses()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    );
    if mode == "worker" {
        assert_eq!(
            std::env::var("GZ_LEGACY_CAPTURE_READY").as_deref(),
            Ok("sendto-intercepted"),
            "missing send interception; no worker may start"
        );
        let mut expected = config.legacy_discovery_relays.addresses().to_vec();
        // Existing public query contract is sorted; captured sends test insertion order.
        expected.sort_unstable();
        let mut node = Node::new(config).unwrap();
        assert_eq!(node.discovery_relays().unwrap(), expected);
        node.shutdown();
    }
}

#[test]
fn native_environment_corpus_matches_pure_conversion_constructor_and_public_config() {
    let fixture: gz_msgs::serde_json::Value =
        gz_msgs::serde_json::from_str(include_str!("fixtures/legacy-relays.json")).unwrap();
    assert_eq!(
        fixture["native_source"],
        "41913b9dfc3dbce33ac5464ddcb427cc82450f26"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 78);
    for case in cases {
        let value = case["input"].as_str();
        let native = child_command(artifact("legacy-relay-peer"), value)
            .output()
            .unwrap();
        assert_eq!(native.stderr, case["stderr"].as_str().unwrap().as_bytes());
        assert_eq!(
            success(native),
            case["stdout"].as_str().unwrap(),
            "{value:?}"
        );
        let expected: Vec<_> = case["addresses"]
            .as_array()
            .unwrap()
            .iter()
            .map(|address| address.as_str().unwrap())
            .collect();
        // Capture EVERY send, including native destructor BYE. Invalid-to-broadcast
        // and multicast/unspecified destinations never reach a kernel send call.
        let constructor = child_command(artifact("legacy-relay-peer"), value)
            .arg("constructor")
            .env("LD_PRELOAD", artifact("legacy-relay-sends.so"))
            .output()
            .unwrap();
        let captures = String::from_utf8(constructor.stderr.clone()).unwrap();
        let addresses: Vec<_> = captures
            .lines()
            .filter_map(|line| line.strip_prefix("CAPTURE "))
            .map(|record| record.split_whitespace().next().unwrap())
            .collect();
        let expected_sends: Vec<_> = (0..2)
            .flat_map(|_| std::iter::once("239.255.0.7").chain(expected.iter().copied()))
            .collect();
        assert_eq!(addresses, expected_sends, "{value:?}");
        let constructor = success(constructor);
        let expected_constructor: String = ["message", "service"]
            .into_iter()
            .flat_map(|channel| {
                expected
                    .iter()
                    .map(move |address| format!("{channel} {address}\n"))
            })
            .collect();
        assert_eq!(constructor, expected_constructor, "{value:?}");
        let rust = success(
            child_command(std::env::current_exe().unwrap(), value)
                .args(["--exact", "legacy_relay_child", "--nocapture"])
                .env("GZ_LEGACY_CHILD", "config")
                .output()
                .unwrap(),
        );
        assert!(
            rust.lines()
                .any(|line| line == format!("LEGACY {}", expected.join(" "))),
            "{value:?}: {rust}"
        );
    }
}

fn reserve() -> Socket {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).unwrap();
    socket.set_reuse_address(true).unwrap();
    socket
        .bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into())
        .unwrap();
    #[cfg(target_os = "linux")]
    socket.set_multicast_all_v4(false).unwrap();
    socket
}
#[test]
fn native_permissive_seeds_reach_both_workers_and_failed_send_stops_in_order() {
    let reservations = [reserve(), reserve()];
    let ports = reservations.each_ref().map(|socket| {
        socket
            .local_addr()
            .unwrap()
            .as_socket_ipv4()
            .unwrap()
            .port()
    });
    assert_ne!(ports[0], ports[1]);
    for fail in [false, true] {
        let mut command = child_command(
            std::env::current_exe().unwrap(),
            Some("bad::0:224.1.2.3:127.2"),
        );
        command
            .args(["--exact", "legacy_relay_child", "--nocapture"])
            .env("GZ_LEGACY_CHILD", "worker")
            .env("GZ_DISCOVERY_MSG_PORT", ports[0].to_string())
            .env("GZ_DISCOVERY_SRV_PORT", ports[1].to_string())
            .env("LD_PRELOAD", artifact("legacy-relay-sends.so"));
        if fail {
            command.env("GZ_LEGACY_FAIL_BROADCAST", "1");
        }
        let output = command.output().unwrap();
        let captures = String::from_utf8(output.stderr.clone()).unwrap();
        success(output);
        let records: Vec<_> = captures
            .lines()
            .filter_map(|line| line.strip_prefix("CAPTURE "))
            .map(|record| record.split_whitespace().collect::<Vec<_>>())
            .collect();
        assert!(!records.is_empty(), "interposer did not capture sendto");
        for port in ports {
            let addresses: Vec<_> = records
                .iter()
                .filter(|record| record[1] == port.to_string())
                .map(|record| record[0])
                .collect();
            assert!(addresses.contains(&"255.255.255.255"));
            if fail {
                assert!(!addresses.contains(&"0.0.0.0"));
                assert!(!addresses.contains(&"224.1.2.3"));
                assert!(!addresses.contains(&"127.0.0.2"));
            } else {
                assert!(addresses.windows(4).any(
                    |window| window == ["255.255.255.255", "0.0.0.0", "224.1.2.3", "127.0.0.2"]
                ));
            }
        }
    }
}

#[test]
fn bounded_numeric_and_malformed_families_match_actual_inet_addr() {
    use std::{collections::BTreeSet, fmt::Write};
    let parts = [
        "0",
        "00",
        "01",
        "07",
        "08",
        "09",
        "0x",
        "0X",
        "0x0",
        "0x1",
        "0Xff",
        "255",
        "256",
        "65535",
        "65536",
        "16777215",
        "16777216",
        "4294967295",
        "4294967296",
        "0xffffffff",
        "0x100000000",
        "037777777777",
        "040000000000",
        "+1",
        "-1",
        "a",
        "",
    ];
    let mut cases = BTreeSet::new();
    let mut seed = 41913u64;
    for width in 1..=5 {
        for _ in 0..3000 {
            let mut token = Vec::new();
            for _ in 0..width {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                token.push(parts[(seed >> 32) as usize % parts.len()]);
            }
            cases.insert(token.join("."));
        }
    }
    for zeros in [1, 64, 256, 4096, 64000] {
        for suffix in ["", "1", "7", "8", "255", "0x1", "4294967295", "4294967296"] {
            let padding = "0".repeat(zeros);
            cases.extend([
                format!("{padding}{suffix}"),
                format!("0.{padding}{suffix}"),
                format!("0x{padding}{suffix}"),
            ]);
        }
    }
    for value in [
        "127.1",
        "0",
        "0xffffffff",
        "1.2.3.4",
        "08",
        "1.2.3.256",
        "1.",
        "0x",
    ] {
        for whitespace in [' ', '\t', '\n', '\r', '\u{b}', '\u{c}'] {
            cases.extend([
                format!("{value}{whitespace}garbage"),
                format!("{whitespace}{value}"),
                format!("{value}{whitespace}é"),
            ]);
        }
    }
    cases.extend(
        [
            "１.２.３.４",
            "127.0.0.1\u{a0}tail",
            "%31.1",
            "1%2e1",
            "0x1p2",
            "[::1]",
        ]
        .map(str::to_owned),
    );
    // Colons are environment separators, exercised by the retained corpus.
    cases.retain(|case| !case.is_empty() && !case.contains(':'));
    let mut input = String::new();
    for case in &cases {
        for byte in case.bytes() {
            write!(input, "{byte:02x}").unwrap();
        }
        input.push('\n');
    }
    let path = std::env::temp_dir().join(format!("gz-relay-tokens-{}", uuid::Uuid::new_v4()));
    std::fs::write(&path, input).unwrap();
    let output = child_command(artifact("legacy-relay-peer"), None)
        .arg("batch")
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(output.stderr.is_empty());
    let native = success(output);
    assert_eq!(native.lines().count(), cases.len());
    for (input, expected) in cases.iter().zip(native.lines()) {
        let actual = Config::default()
            .with_environment(|key| (key == "GZ_RELAY").then(|| input.clone()))
            .unwrap();
        assert_eq!(
            actual.legacy_discovery_relays.addresses(),
            [expected.parse::<Ipv4Addr>().unwrap()],
            "{input:?}"
        );
    }
    println!("native numeric/malformed records: {}", cases.len());
}

#[test]
fn missing_interception_rejects_before_any_native_or_rust_worker_start() {
    let native = child_command(artifact("legacy-relay-peer"), Some("bad:224.1.2.3"))
        .arg("constructor")
        .output()
        .unwrap();
    assert_eq!(native.status.code(), Some(6));
    assert!(native.stdout.is_empty());
    assert!(native.stderr.is_empty());
    let rust = child_command(std::env::current_exe().unwrap(), Some("bad:224.1.2.3"))
        .args(["--exact", "legacy_relay_child", "--nocapture"])
        .env("GZ_LEGACY_CHILD", "worker")
        .output()
        .unwrap();
    assert_eq!(rust.status.code(), Some(101));
    assert!(
        String::from_utf8(rust.stderr)
            .unwrap()
            .contains("missing send interception; no worker may start")
    );
}
