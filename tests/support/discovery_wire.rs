// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use super::*;
use gz_msgs::{
    discovery::discovery::{Flags, Subscriber, publisher},
    prost::encoding::{WireType, encode_key, encode_varint},
};
use std::{
    io::Write,
    process::{Command, Stdio},
};

fn field(tag: u32, wire: WireType, payload: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    encode_key(tag, wire, &mut result);
    if wire == WireType::LengthDelimited {
        encode_varint(payload.len() as u64, &mut result);
    }
    result.extend(payload);
    result
}

fn native_known(body: &[u8]) -> Option<Discovery> {
    let path = std::env::var_os("GZ_CPP_PEER")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("artifacts/cpp-peer")
        });
    let mut peer = Command::new(path)
        .args(["decode-discovery", "unused", "unused"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("build the mandatory C++ peer with scripts/build-cpp-peer.sh");
    peer.stdin.take().unwrap().write_all(body).unwrap();
    let result = peer.wait_with_output().unwrap();
    match result.status.code() {
        Some(0) => Some(Discovery::decode(result.stdout.as_slice()).unwrap()),
        Some(30) => None,
        status => panic!(
            "native parser infrastructure failed: {status:?}: {:?}",
            result.stderr
        ),
    }
}

fn native_relay(body: &[u8], mode: &str) -> Vec<u8> {
    let path = std::env::var_os("GZ_DISCOVERY_RELAY_PEER")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("artifacts/discovery-relay-peer")
        });
    let mut peer = Command::new(path)
        .arg(mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("build the discovery relay C++ peer");
    peer.stdin.take().unwrap().write_all(body).unwrap();
    let output = peer.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "native relay oracle failed: {:?}",
        output.stderr
    );
    output.stdout
}

#[test]
fn relay_flags_preserve_native_merges_and_opaque_unknown_fields() {
    let base = discovery("p", Type::Heartbeat, None).encode_to_vec();
    let flags = [
        vec![],
        vec![0x2a, 0],
        vec![0x2a, 4, 8, 1, 16, 1],
        vec![0x2a, 4, 8, 0, 16, 0],
        vec![0x2a, 2, 8, 1, 0x2a, 2, 16, 1],
        vec![0x2a, 8, 8, 1, 16, 1, 0xa0, 6, 0x81, 0],
        vec![0x2a, 5, 10, 1, 255, 16, 1],
        vec![0x28, 1],
    ];
    let unknown = [
        vec![],
        vec![0xa0, 6, 0x81, 0],
        vec![0x12, 1, 255],
        vec![0xa3, 6, 8, 1, 0xa4, 6],
    ];
    for flags in flags {
        for unknown in &unknown {
            let body = [base.as_slice(), &flags, unknown].concat();
            let frame = [(body.len() as u16).to_ne_bytes().as_slice(), &body].concat();
            let original = decode(&frame).unwrap();
            for (unicast, mode) in [(true, "unicast"), (false, "multicast")] {
                let forwarded = relay_frame(&frame, unicast).unwrap();
                let non_flags = if flags == [0x28, 1] {
                    [base.as_slice(), &flags, unknown].concat()
                } else {
                    [base.as_slice(), unknown].concat()
                };
                assert_eq!(&forwarded[2..2 + non_flags.len()], non_flags);
                if unknown.is_empty() {
                    for raw in [&[0xa0, 6, 0x81, 0][..], &[10, 1, 255][..]] {
                        if flags.windows(raw.len()).any(|value| value == raw) {
                            assert!(forwarded.windows(raw.len()).any(|value| value == raw));
                        }
                    }
                }
                let observed = decode(&forwarded).unwrap();
                assert_eq!(observed.process_uuid, original.process_uuid);
                assert_eq!(observed.r#type, original.r#type);
                let observed_flags = observed.flags.unwrap();
                assert_eq!(observed_flags.relay, unicast);
                assert_eq!(
                    observed_flags.no_relay,
                    !unicast || original.flags.as_ref().is_some_and(|value| value.no_relay)
                );
                assert_eq!(
                    native_relay(&forwarded[2..], "parse"),
                    native_relay(&body, mode)
                );
            }
        }
    }
}

#[test]
fn relay_flags_enforce_final_datagram_bound() {
    assert_eq!(relay_frame(&[], true), Err(Error::InvalidFrame));
    let frame = relay_frame_at_size(MAX_DATAGRAM - 4, false);
    let exact = relay_frame(&frame, true).unwrap();
    assert_eq!(exact.len(), MAX_DATAGRAM);
    assert_eq!(
        u16::from_ne_bytes(exact[..2].try_into().unwrap()) as usize,
        MAX_DATAGRAM - 2
    );
    let too_large = relay_frame_at_size(MAX_DATAGRAM - 3, false);
    assert_eq!(relay_frame(&too_large, true), Err(Error::InvalidFrame));
    assert_eq!(
        relay_frame(&vec![0; MAX_DATAGRAM], true),
        Err(Error::InvalidFrame)
    );
}

fn relay_frame_at_size(size: usize, relay: bool) -> Vec<u8> {
    let mut message = discovery(&"x".repeat(size - 32), Type::Heartbeat, None);
    if relay {
        message.flags = Some(Flags {
            relay: true,
            no_relay: false,
        });
    }
    let remaining = size - encode(&message).unwrap().len();
    message.process_uuid.push_str(&"x".repeat(remaining));
    let frame = encode(&message).unwrap();
    assert_eq!(frame.len(), size);
    frame
}

#[test]
fn relay_reinjection_keeps_native_maximum_datagram() {
    let frame = relay_frame_at_size(MAX_DATAGRAM, true);
    let native = native_relay(&frame[2..], "multicast");
    assert_eq!(native.len() + 2, MAX_DATAGRAM);
    let forwarded = relay_frame(&frame, false).unwrap();
    assert!(forwarded.len() <= MAX_DATAGRAM);
    assert_eq!(native_relay(&forwarded[2..], "parse"), native);
}

#[test]
fn relay_flags_merge_length_boundaries_and_reject_malformed_fields() {
    let base = discovery("p", Type::Heartbeat, None).encode_to_vec();
    for length in [0, 127, 128, 16383, 16384, 60000] {
        let opaque = field(127, WireType::LengthDelimited, &vec![255; length]);
        let flags = [vec![8, 1], opaque.clone()].concat();
        let body = [
            base.clone(),
            field(5, WireType::LengthDelimited, &flags),
            field(5, WireType::LengthDelimited, &[16, 1]),
        ]
        .concat();
        let frame = [(body.len() as u16).to_ne_bytes().as_slice(), &body].concat();
        let forwarded = relay_frame(&frame, false).unwrap();
        assert!(forwarded.len() <= frame.len());
        assert!(forwarded.windows(opaque.len()).any(|value| value == opaque));
        assert_eq!(
            native_relay(&forwarded[2..], "parse"),
            native_relay(&body, "multicast")
        );
    }
    for malformed in [
        vec![0],
        vec![0x2a, 0x80],
        vec![0x2a, 5, 1],
        vec![0x2a, 1, 0],
        vec![0x2a, 1, 8],
        vec![0x32, 5, 1],
        vec![0x2b],
    ] {
        let frame = [
            (malformed.len() as u16).to_ne_bytes().as_slice(),
            &malformed,
        ]
        .concat();
        assert_eq!(relay_frame(&frame, false), Err(Error::InvalidFrame));
    }
}

#[test]
fn native_discovery_wrong_wire_values_presence_and_framing() {
    let base = discovery("wire-process", Type::Heartbeat, None).encode_to_vec();
    let wrong_version = field(2, WireType::SixtyFourBit, &[0; 8]);
    let mut cases = vec![
        ("canonical heartbeat".to_owned(), base.clone()),
        (
            "wrong version before known".into(),
            [wrong_version.clone(), base.clone()].concat(),
        ),
        (
            "wrong version after known".into(),
            [base.clone(), wrong_version.clone()].concat(),
        ),
        ("wrong version only".into(), wrong_version),
        ("empty protobuf".into(), Vec::new()),
    ];
    for (tag, wire, payload) in [
        (2, WireType::LengthDelimited, vec![255]),
        (3, WireType::Varint, vec![1]),
        (4, WireType::ThirtyTwoBit, vec![0; 4]),
        (127, WireType::Varint, vec![1]),
        (7, WireType::Varint, vec![1]),
    ] {
        cases.push((
            format!("top-level field {tag}"),
            [base.clone(), field(tag, wire, &payload)].concat(),
        ));
    }
    for version in [9, 110] {
        let mut message = discovery("wire-process", Type::Heartbeat, None);
        message.version = version;
        cases.push((
            format!("unsupported version {version}"),
            message.encode_to_vec(),
        ));
    }
    let flags = Flags {
        relay: true,
        ..Default::default()
    }
    .encode_to_vec();
    cases.push((
        "nested flags".into(),
        [
            base.clone(),
            field(
                5,
                WireType::LengthDelimited,
                &[flags, field(1, WireType::SixtyFourBit, &[0; 8])].concat(),
            ),
        ]
        .concat(),
    ));

    let publisher = Publisher {
        topic: "@@/wire".into(),
        address: "tcp://127.0.0.1:45000".into(),
        process_uuid: "wire-process".into(),
        node_uuid: "wire-node".into(),
        scope: publisher::Scope::All as i32,
        ..Default::default()
    };
    let message_type = publisher::MessagePublisher {
        ctrl: "tcp://127.0.0.1:45001".into(),
        msg_type: "gz.msgs.StringMsg".into(),
        ..Default::default()
    }
    .encode_to_vec();
    let service_type = publisher::ServicePublisher {
        socket_id: "socket".into(),
        request_type: "gz.msgs.StringMsg".into(),
        response_type: "gz.msgs.StringMsg".into(),
    }
    .encode_to_vec();
    for (label, kind, encoded) in [
        ("message", 6, message_type.clone()),
        ("service", 7, service_type),
    ] {
        for tag in [1, 2, 3] {
            let nested = [encoded.clone(), field(tag, WireType::Varint, &[1])].concat();
            let body = [
                publisher.encode_to_vec(),
                field(kind, WireType::LengthDelimited, &nested),
            ]
            .concat();
            cases.push((
                format!("nested {label} field {tag}"),
                [base.clone(), field(7, WireType::LengthDelimited, &body)].concat(),
            ));
        }
    }
    let advertised = [
        base.clone(),
        field(7, WireType::LengthDelimited, &publisher.encode_to_vec()),
    ]
    .concat();
    let subscriber = Subscriber {
        topic: "@@/sub".into(),
    }
    .encode_to_vec();
    cases.push((
        "valid oneof replaces prior arm".into(),
        [
            advertised.clone(),
            field(6, WireType::LengthDelimited, &subscriber),
            field(7, WireType::Varint, &[1]),
        ]
        .concat(),
    ));
    cases.push((
        "duplicate publisher message merge".into(),
        [
            advertised,
            field(
                7,
                WireType::LengthDelimited,
                &field(4, WireType::LengthDelimited, b"replacement-node"),
            ),
        ]
        .concat(),
    ));
    for (label, body) in [
        (
            "publisher scope wrong wire",
            [
                publisher.encode_to_vec(),
                field(5, WireType::ThirtyTwoBit, &[0; 4]),
            ]
            .concat(),
        ),
        (
            "publisher oneof wrong arm",
            [
                publisher.encode_to_vec(),
                field(6, WireType::LengthDelimited, &message_type),
                field(7, WireType::Varint, &[1]),
            ]
            .concat(),
        ),
        (
            "nested invalid UTF8",
            [
                publisher.encode_to_vec(),
                field(
                    6,
                    WireType::LengthDelimited,
                    &field(2, WireType::LengthDelimited, &[255]),
                ),
            ]
            .concat(),
        ),
    ] {
        cases.push((
            label.into(),
            [base.clone(), field(7, WireType::LengthDelimited, &body)].concat(),
        ));
    }
    let mut group = field(2, WireType::StartGroup, &field(1, WireType::Varint, &[1]));
    group.extend(field(2, WireType::EndGroup, &[]));
    cases.push((
        "wrong-wire group before known".into(),
        [group, base.clone()].concat(),
    ));
    for (label, malformed) in [
        ("bad wire type", vec![0x17]),
        ("truncated varint", vec![0x10, 0x80]),
        ("truncated nested message", vec![0x3a, 4, 1]),
        (
            "invalid known UTF8",
            field(3, WireType::LengthDelimited, &[255]),
        ),
        ("unclosed group", field(2, WireType::StartGroup, &[])),
    ] {
        cases.push((label.into(), [base.clone(), malformed].concat()));
    }
    cases.push((
        "optional flags retain empty presence".into(),
        [
            base.clone(),
            field(
                5,
                WireType::LengthDelimited,
                &field(1, WireType::SixtyFourBit, &[0; 8]),
            ),
        ]
        .concat(),
    ));
    assert_eq!(cases.len(), 31);
    for (label, body) in cases {
        let expected = native_known(&body).filter(|value| value.version == VERSION);
        let mut frame = (body.len() as u16).to_ne_bytes().to_vec();
        frame.extend(body);
        match expected {
            Some(expected) => assert_eq!(decode(&frame).unwrap(), expected, "{label}"),
            None => assert_eq!(decode(&frame), Err(Error::InvalidFrame), "{label}"),
        }
    }
}

#[test]
fn statistics_discovery_requires_the_selected_wire_version() {
    let mut message = discovery("profile", Type::Heartbeat, None);
    let ordinary = encode(&message).unwrap();
    assert_eq!(decode(&ordinary), Ok(message.clone()));
    assert_eq!(decode_version(&ordinary, 110), Err(Error::InvalidFrame));
    message.version = 110;
    let statistics = encode(&message).unwrap();
    assert_eq!(decode(&statistics), Err(Error::InvalidFrame));
    assert_eq!(decode_version(&statistics, 110), Ok(message));
}
