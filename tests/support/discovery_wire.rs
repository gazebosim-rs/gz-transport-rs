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
