// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use super::*;
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
            &[("GZ_TRANSPORT_TOPIC_STATISTICS", "1")][..],
            Error::Unsupported("statistics wire version 110"),
        ),
        (
            &[("GZ_RELAY", "10.0.0.1")][..],
            Error::Unsupported("discovery relay"),
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
