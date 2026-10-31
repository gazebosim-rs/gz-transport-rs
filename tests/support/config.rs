// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn initial_relay_configuration_is_bounded_atomic_and_copied() {
    let first = Ipv4Addr::new(127, 0, 0, 2);
    let second = Ipv4Addr::new(127, 0, 0, 3);
    let mut config = Config {
        capacity: 2,
        ..Default::default()
    };
    assert!(config.discovery_relays.is_empty());
    config.add_discovery_relay(first).unwrap();
    config.add_discovery_relay(first).unwrap();
    config.add_discovery_relay(second).unwrap();
    assert_eq!(config.discovery_relays, vec![first, second]);
    assert_eq!(
        config.add_discovery_relay(Ipv4Addr::new(127, 0, 0, 4)),
        Err(Error::QueueFull)
    );
    for invalid in [
        Ipv4Addr::UNSPECIFIED,
        Ipv4Addr::BROADCAST,
        Ipv4Addr::new(224, 0, 0, 1),
    ] {
        assert_eq!(
            config.add_discovery_relay(invalid),
            Err(Error::InvalidConfig)
        );
        assert_eq!(config.discovery_relays, vec![first, second]);
    }
    let mut copied = config.clone();
    copied.discovery_relays[0] = Ipv4Addr::new(127, 0, 0, 5);
    assert_eq!(config.discovery_relays, vec![first, second]);
    config.validate().unwrap();
    copied.capacity = 1;
    assert_eq!(copied.validate(), Err(Error::InvalidConfig));
    let preserved = config.clone().with_environment(|_| None).unwrap();
    assert_eq!(preserved.discovery_relays, config.discovery_relays);
    let empty = config
        .clone()
        .with_environment(|name| (name == "GZ_RELAY").then(String::new))
        .unwrap();
    assert_eq!(empty.discovery_relays, config.discovery_relays);
    // The legacy capability is now implemented separately; typed seeds persist.
    let legacy = config
        .clone()
        .with_environment(|name| (name == "GZ_RELAY").then(|| "127.0.0.2".into()))
        .unwrap();
    assert_eq!(legacy.discovery_relays, config.discovery_relays);
    assert_eq!(legacy.legacy_discovery_relays.addresses(), [first]);
}

#[test]
fn raw_initial_relay_fields_are_validated_before_worker_creation() {
    for address in [
        Ipv4Addr::UNSPECIFIED,
        Ipv4Addr::BROADCAST,
        Ipv4Addr::new(239, 1, 2, 3),
    ] {
        let config = Config {
            discovery_relays: vec![address],
            ..Default::default()
        };
        assert_eq!(config.validate(), Err(Error::InvalidConfig));
        assert!(matches!(
            crate::Node::new(config),
            Err(Error::InvalidConfig)
        ));
    }
    let address = Ipv4Addr::LOCALHOST;
    let config = Config {
        capacity: 1,
        discovery_relays: vec![address, address],
        ..Default::default()
    };
    assert_eq!(config.validate(), Err(Error::InvalidConfig));
    assert!(matches!(
        crate::Node::new(config),
        Err(Error::InvalidConfig)
    ));
    Config {
        capacity: 2,
        discovery_relays: vec![address, address],
        ..Default::default()
    }
    .validate()
    .unwrap();
}
fn configured(values: &[(&str, &str)]) -> Result<Config, Error> {
    Config::default().with_environment(|key| {
        values
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.to_string())
    })
}
#[test]
fn cpp_stoi_nonnegative_prefix_and_fallback() {
    assert_eq!(nonnegative(None, 1000), 1000);
    for (input, expected) in [
        ("", 1000),
        (" ", 1000),
        ("-1", 1000),
        ("99999999999999999999", 1000),
        ("2147483648", 1000),
        ("2147483647", i32::MAX),
        ("12tail", 12),
        ("3.5", 3),
        ("  +8 rest", 8),
        ("\t\n\r7", 7),
        ("-0", 0),
        ("0x10", 0),
        ("+", 1000),
        ("--2", 1000),
        ("\u{a0}1", 1000),
    ] {
        assert_eq!(nonnegative(Some(input.into()), 1000), expected, "{input}");
    }
}
#[test]
fn explicit_environment_settings_and_duplicate_ports() {
    let config = configured(&[
        ("GZ_PARTITION", "partition"),
        ("GZ_IP", "127.0.0.1"),
        ("GZ_DISCOVERY_MULTICAST_IP", "239.255.0.8"),
        ("GZ_DISCOVERY_MSG_PORT", "11317tail"),
        ("GZ_DISCOVERY_SRV_PORT", "11318"),
        ("GZ_TRANSPORT_SNDHWM", "8"),
        ("GZ_TRANSPORT_RCVHWM", "7"),
        ("GZ_TRANSPORT_IMPLEMENTATION", "ZeRoMq"),
        ("GZ_TRANSPORT_TOPIC_STATISTICS", "true"),
        ("GZ_TRANSPORT_USERNAME", "partial"),
    ])
    .unwrap();
    assert_eq!(config.partition, "partition");
    assert_eq!(config.interface, Ipv4Addr::LOCALHOST);
    assert_eq!(config.multicast, Ipv4Addr::new(239, 255, 0, 8));
    assert_eq!((config.message_port, config.service_port), (11317, 11318));
    assert_eq!(
        (config.send_high_watermark, config.receive_high_watermark),
        (8, 7)
    );
    for (input, expected) in [("12345", 12346), ("65535", 65534)] {
        let config = configured(&[
            ("GZ_DISCOVERY_MSG_PORT", input),
            ("GZ_DISCOVERY_SRV_PORT", input),
        ])
        .unwrap();
        assert_eq!(config.service_port, expected);
    }
    let config = configured(&[
        ("GZ_PARTITION", ""),
        ("GZ_IP", ""),
        ("GZ_DISCOVERY_MULTICAST_IP", ""),
        ("GZ_TRANSPORT_IMPLEMENTATION", ""),
        ("GZ_RELAY", ""),
    ])
    .unwrap();
    assert_eq!(config.partition, "");
    assert_eq!(config.interface, Ipv4Addr::LOCALHOST);
    let config = configured(&[
        ("GZ_DISCOVERY_MSG_PORT", "-1"),
        ("GZ_DISCOVERY_SRV_PORT", "not-a-number"),
        ("GZ_TRANSPORT_SNDHWM", "2147483648"),
        ("GZ_TRANSPORT_RCVHWM", ""),
    ])
    .unwrap();
    assert_eq!((config.message_port, config.service_port), (10317, 10318));
    assert_eq!(
        (config.send_high_watermark, config.receive_high_watermark),
        (1000, 1000)
    );
    let config = Config {
        partition: "base".into(),
        namespace: "ns".into(),
        send_high_watermark: 37,
        receive_high_watermark: 41,
        ..Default::default()
    }
    .with_environment(|_| None)
    .unwrap();
    assert_eq!(
        (config.send_high_watermark, config.receive_high_watermark),
        (37, 41)
    );
    assert_eq!(config.partition, "base");
    assert_eq!(config.namespace, "ns");
}
#[test]
fn unsupported_and_invalid_configuration_are_explicit() {
    // Exact former guard input now has a native-backed positive contract.
    let legacy = configured(&[("GZ_RELAY", "10.0.0.1")]).unwrap();
    assert_eq!(
        legacy.legacy_discovery_relays.addresses(),
        [Ipv4Addr::new(10, 0, 0, 1)]
    );
    assert!(legacy.discovery_relays.is_empty());
    for (values, error) in [
        (
            &[("GZ_TRANSPORT_SNDHWM", "0")][..],
            Error::Unsupported("unbounded ZeroMQ queues"),
        ),
        (
            &[("GZ_TRANSPORT_RCVHWM", "0")][..],
            Error::Unsupported("unbounded ZeroMQ queues"),
        ),
        (
            &[("GZ_TRANSPORT_IMPLEMENTATION", "zenoh")][..],
            Error::Unsupported("transport implementation"),
        ),
        (
            &[("GZ_TRANSPORT_USERNAME", ""), ("GZ_TRANSPORT_PASSWORD", "")][..],
            Error::Unsupported("PLAIN authentication"),
        ),
    ] {
        assert_eq!(configured(values).unwrap_err(), error);
    }
    for values in [
        vec![("GZ_PARTITION", "invalid partition")],
        vec![("GZ_IP", "invalid")],
        vec![("GZ_DISCOVERY_MULTICAST_IP", "invalid")],
        vec![("GZ_DISCOVERY_MULTICAST_IP", "127.0.0.1")],
        vec![("GZ_DISCOVERY_MSG_PORT", "65536")],
        vec![("GZ_DISCOVERY_SRV_PORT", "65536")],
        vec![("GZ_DISCOVERY_MSG_PORT", "0")],
    ] {
        assert!(configured(&values).is_err());
    }
    for config in [
        Config {
            send_high_watermark: 0,
            ..Default::default()
        },
        Config {
            receive_high_watermark: 0,
            ..Default::default()
        },
        Config {
            send_high_watermark: usize::MAX,
            ..Default::default()
        },
        Config {
            receive_high_watermark: usize::MAX,
            ..Default::default()
        },
    ] {
        assert!(config.clone().with_environment(|_| None).is_err());
        assert!(config.validate().is_err());
    }
    assert!(configured(&[("GZ_TRANSPORT_PASSWORD", "partial")]).is_ok());
}

#[cfg(unix)]
#[test]
fn native_partition_identity_precedence_and_failures() {
    use nix::errno::Errno;
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    assert_eq!(
        partition_identity(Ok("host".into()), Some("user".into()), || panic!(
            "USER must bypass NSS"
        )),
        Ok("host:user".into())
    );
    assert_eq!(
        partition_identity(Ok("host".into()), Some("".into()), || panic!(
            "empty USER is present"
        )),
        Ok("host:".into())
    );
    assert_eq!(
        partition_identity(Ok("host".into()), None, || Ok(Some("system".into()))),
        Ok("host:system".into())
    );
    assert_eq!(
        partition_identity(Ok("host".into()), None, || Ok(None)),
        Err(Error::InvalidConfig)
    );
    assert!(matches!(
        partition_identity(Err(Errno::EIO), None, || panic!(
            "hostname failure must stop lookup"
        )),
        Err(Error::Native(_))
    ));
    assert!(matches!(
        partition_identity(Ok("host".into()), None, || Err(Errno::EIO)),
        Err(Error::Native(_))
    ));
    let invalid = OsString::from_vec(vec![255]);
    assert_eq!(
        partition_identity(Ok(invalid.clone()), Some("user".into()), || unreachable!()),
        Err(Error::InvalidConfig)
    );
    assert_eq!(
        partition_identity(Ok("host".into()), Some(invalid), || unreachable!()),
        Err(Error::InvalidConfig)
    );
}

#[cfg(unix)]
#[test]
fn nss_lossy_names_cannot_silently_change_partition_identity() {
    assert_eq!(
        partition_identity(Ok("host".into()), None, || Ok(Some("user\u{fffd}".into()))),
        Err(Error::Unsupported("ambiguous NSS username encoding"))
    );
    assert_eq!(
        partition_identity(
            Ok("host".into()),
            Some("user\u{fffd}".into()),
            || unreachable!()
        ),
        Ok("host:user\u{fffd}".into())
    );
}

#[test]
fn component_setter_limits_and_rejected_state_match_native() {
    let mut config = Config {
        partition: "original".into(),
        namespace: "namespace".into(),
        ..Default::default()
    };
    for invalid in ["/", "bad name", "~", "a//b", "@", "a:=b"] {
        assert_eq!(config.set_partition(invalid), Err(Error::InvalidName));
        assert_eq!(config.partition, "original");
        assert_eq!(config.set_namespace(invalid), Err(Error::InvalidName));
        assert_eq!(config.namespace, "namespace");
    }
    let limit = "a".repeat(65535);
    config.set_partition(&limit).unwrap();
    config.set_namespace(&limit).unwrap();
    assert_eq!(config.partition, limit);
    assert_eq!(config.namespace, limit);
    let long = "b".repeat(65536);
    assert_eq!(config.set_partition(&long), Err(Error::InvalidName));
    assert_eq!(config.set_namespace(&long), Err(Error::InvalidName));
    assert_eq!(config.partition, limit);
    assert_eq!(config.namespace, limit);
    config.set_partition("").unwrap();
    config.set_namespace("").unwrap();
    assert_eq!(
        (config.partition, config.namespace),
        (String::new(), String::new())
    );
}

#[test]
// Node_TEST.cc assertions: Copyright (C) 2014 Open Source Robotics Foundation.
fn original_wrong_topic_remap() {
    let mut config = Config::default();
    assert_eq!(
        config.add_topic_remap("  ", "/bar"),
        Err(Error::InvalidName)
    );
    assert_eq!(
        config.add_topic_remap("/foo", "  "),
        Err(Error::InvalidName)
    );
    assert_eq!(config.add_topic_remap("/foo", "/bar"), Ok(()));
    assert_eq!(
        config.add_topic_remap("/foo", "/bar"),
        Err(Error::AlreadyRemapped)
    );
}

#[test]
fn remapping_is_bounded_exact_and_validated_before_worker_creation() {
    let mut config = Config {
        capacity: 3,
        ..Default::default()
    };
    config.add_topic_remap("from", "to").unwrap();
    config.add_topic_remap("to", "from").unwrap();
    config.add_topic_remap("self", "self").unwrap();
    assert_eq!(config.topic_remap("from"), Some("to"));
    assert_eq!(config.topic_remap("to"), Some("from"));
    assert_eq!(config.topic_remap("self"), Some("self"));
    for absent in ["/from", "from/", "FROM", "invalid topic", ""] {
        assert_eq!(config.topic_remap(absent), None);
    }
    let saved = config.topic_remaps.clone();
    assert_eq!(
        config.add_topic_remap("from", "bad name"),
        Err(Error::InvalidName)
    );
    assert_eq!(
        config.add_topic_remap("new", "value"),
        Err(Error::QueueFull)
    );
    assert_eq!(
        config.add_topic_remap("from", "other"),
        Err(Error::AlreadyRemapped)
    );
    for invalid in [
        "",
        "/",
        "bad topic",
        "~bad",
        "bad:=name",
        "bad@name",
        "a//b",
        &"a".repeat(65536),
    ] {
        assert_eq!(
            config.add_topic_remap(invalid, "valid"),
            Err(Error::InvalidName)
        );
        assert_eq!(
            config.add_topic_remap("valid", invalid),
            Err(Error::InvalidName)
        );
    }
    assert_eq!(config.topic_remaps, saved);
    config.validate().unwrap();
    let mut cloned = config.clone();
    cloned.topic_remaps.insert("extra".into(), "value".into());
    assert_eq!(cloned.validate(), Err(Error::InvalidConfig));
    for invalid in ["", "invalid name"] {
        let mut cloned = config.clone();
        cloned.topic_remaps.remove("self");
        cloned.topic_remaps.insert(invalid.into(), "valid".into());
        assert_eq!(cloned.validate(), Err(Error::InvalidName));
        cloned.topic_remaps.remove(invalid);
        cloned.topic_remaps.insert("valid".into(), invalid.into());
        assert_eq!(cloned.validate(), Err(Error::InvalidName));
    }
    assert_eq!(config.topic_remaps, saved);
}

#[test]
fn remapping_option_names_match_real_native_validation() {
    let peer = std::env::var("GZ_CPP_PEER")
        .unwrap_or_else(|_| format!("{}/artifacts/cpp-peer", env!("CARGO_MANIFEST_DIR")));
    let names = [
        "",
        "/",
        "bad name",
        "a//b",
        "a:=b",
        "~a",
        "a@b",
        "a\tb",
        "snow☃",
        "relative",
        "/absolute",
        "/trailing/",
    ];
    for from in names {
        for to in names {
            let output = std::process::Command::new(&peer)
                .args(["remap-options", from, to])
                .output()
                .unwrap();
            assert!(output.status.success(), "{from:?} -> {to:?}");
            let mut config = Config::default();
            let added = config.add_topic_remap(from, to).is_ok();
            let duplicate = config.add_topic_remap(from, to).is_ok();
            let mapped = config.topic_remap(from);
            let expected = format!(
                "{} {} {}\n{}",
                u8::from(added),
                u8::from(duplicate),
                u8::from(mapped.is_some()),
                mapped.unwrap_or("sentinel")
            );
            assert_eq!(output.stdout, expected.as_bytes(), "{from:?} -> {to:?}");
        }
    }
}

#[test]
fn statistics_wire_configuration_positive() {
    let baseline = Config::default();
    assert!(!baseline.statistics_wire);
    let enabled = baseline
        .clone()
        .with_environment(|key| (key == "GZ_TRANSPORT_TOPIC_STATISTICS").then(|| "1".into()))
        .unwrap();
    assert!(enabled.statistics_wire);
    enabled.validate().unwrap();
    assert_eq!(enabled.partition, baseline.partition);
    assert_eq!(enabled.namespace, baseline.namespace);
    assert_eq!(enabled.message_port, baseline.message_port);
    assert_eq!(enabled.service_port, baseline.service_port);
    assert_eq!(enabled.capacity, baseline.capacity);
    assert_eq!(enabled.max_message_bytes, baseline.max_message_bytes);
    for value in ["0", "", "true", "01", " 1"] {
        let disabled = enabled
            .clone()
            .with_environment(|key| (key == "GZ_TRANSPORT_TOPIC_STATISTICS").then(|| value.into()))
            .unwrap();
        assert!(!disabled.statistics_wire, "{value:?}");
        disabled.validate().unwrap();
    }
    assert!(enabled.with_environment(|_| None).unwrap().statistics_wire);
}

#[test]
fn statistics_wire_metadata_limit_is_explicit() {
    let mut config = Config {
        statistics_wire: true,
        max_message_bytes: 15,
        ..Default::default()
    };
    assert_eq!(config.validate(), Err(Error::InvalidConfig));
    config.max_message_bytes = 16;
    config.validate().unwrap();
    config.statistics_wire = false;
    config.max_message_bytes = 1;
    config.validate().unwrap();
}

#[test]
fn prepared_legacy_relay_environment_preserves_native_addresses_and_typed_admission() {
    let mut config = Config {
        capacity: 4,
        ..Default::default()
    }
    .with_environment(|key| (key == "GZ_RELAY").then(|| "127.2:0177.0.0.2:bad::224.1.2.3:0".into()))
    .unwrap();
    let expected = vec![
        Ipv4Addr::new(127, 0, 0, 2),
        Ipv4Addr::BROADCAST,
        Ipv4Addr::new(224, 1, 2, 3),
        Ipv4Addr::UNSPECIFIED,
    ];
    assert_eq!(config.legacy_discovery_relays.addresses(), expected);
    assert!(config.discovery_relays.is_empty());
    assert_eq!(config.initial_discovery_relays(), expected);
    config
        .add_discovery_relay(Ipv4Addr::new(127, 0, 0, 2))
        .unwrap();
    assert_eq!(
        config.add_discovery_relay(Ipv4Addr::LOCALHOST),
        Err(Error::QueueFull)
    );
    for invalid in [
        Ipv4Addr::BROADCAST,
        Ipv4Addr::UNSPECIFIED,
        Ipv4Addr::new(224, 1, 2, 3),
    ] {
        assert_eq!(
            config.add_discovery_relay(invalid),
            Err(Error::InvalidConfig)
        );
    }
    let mut copy = config.clone();
    let before = copy.legacy_discovery_relays.clone();
    assert_eq!(
        copy.capture_legacy_relays("127.1:127.2:127.3:127.4:127.5"),
        Err(Error::QueueFull)
    );
    assert_eq!(copy.legacy_discovery_relays, before);
    for invalid in ["x".repeat(MAX_RELAY_ENV_BYTES + 1), "127.1\0suffix".into()] {
        assert_eq!(
            copy.capture_legacy_relays(&invalid),
            Err(Error::InvalidConfig)
        );
        assert_eq!(copy.legacy_discovery_relays, before);
    }
    copy.capture_legacy_relays(&"0".repeat(MAX_RELAY_ENV_BYTES))
        .unwrap();
    assert_eq!(
        copy.legacy_discovery_relays.addresses(),
        [Ipv4Addr::UNSPECIFIED]
    );
    copy.capacity = 1;
    assert_eq!(copy.validate(), Err(Error::InvalidConfig));
    assert_eq!(
        config
            .with_environment(|_| None)
            .unwrap()
            .legacy_discovery_relays,
        before
    );
    assert!(
        copy.with_environment(|key| (key == "GZ_RELAY").then(String::new))
            .unwrap()
            .legacy_discovery_relays
            .addresses()
            .is_empty()
    );
}

#[test]
fn legacy_capture_keeps_explicit_validation_and_capacity_errors() {
    for config in [
        Config {
            capacity: 0,
            ..Default::default()
        },
        Config {
            discovery_relays: vec![Ipv4Addr::BROADCAST],
            ..Default::default()
        },
        Config {
            capacity: 1,
            discovery_relays: vec![Ipv4Addr::LOCALHOST; 2],
            ..Default::default()
        },
    ] {
        assert!(matches!(
            config.with_environment(|key| (key == "GZ_RELAY").then(|| "127.2".into())),
            Err(Error::InvalidConfig)
        ));
    }
    let mut config = Config::default()
        .with_environment(|key| (key == "GZ_RELAY").then(|| "127.2:127.3".into()))
        .unwrap();
    config.capacity = 1;
    assert_eq!(config.validate(), Err(Error::InvalidConfig));
    let mut config = Config {
        capacity: 1,
        ..Default::default()
    }
    .with_environment(|key| (key == "GZ_RELAY").then(|| "127.2".into()))
    .unwrap();
    config
        .add_discovery_relay(Ipv4Addr::new(127, 0, 0, 2))
        .unwrap();
    config
        .add_discovery_relay(Ipv4Addr::new(127, 0, 0, 2))
        .unwrap();
    assert_eq!(config.discovery_relays, [Ipv4Addr::new(127, 0, 0, 2)]);
    assert_eq!(
        config.add_discovery_relay(Ipv4Addr::new(127, 0, 0, 3)),
        Err(Error::QueueFull)
    );
}
