// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use gz_msgs::{int32::Int32, prost::Message};
use gz_transport::{Config, Error, Node};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{Ipv4Addr, UdpSocket},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
const INT: &str = "gz.msgs.Int32";
fn config(statistics_wire: bool) -> Config {
    let sockets = [
        UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap(),
        UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap(),
    ];
    let [message_port, service_port] = sockets.each_ref().map(|s| s.local_addr().unwrap().port());
    assert_ne!(message_port, service_port);
    drop(sockets);
    Config {
        statistics_wire,
        partition: format!("statistics-wire-{}", uuid::Uuid::new_v4()),
        message_port,
        service_port,
        ..Default::default()
    }
}
fn value(data: i32) -> Vec<u8> {
    Int32 {
        data,
        ..Default::default()
    }
    .encode_to_vec()
}
fn unhex(text: &str) -> Vec<u8> {
    if text == "-" {
        return vec![];
    }
    assert_eq!(text.len() % 2, 0);
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}
fn hex(data: &[u8]) -> String {
    if data.is_empty() {
        "-".into()
    } else {
        data.iter().map(|b| format!("{b:02x}")).collect()
    }
}
struct Native {
    child: Child,
    lines: mpsc::Receiver<String>,
}
impl Native {
    fn start(config: &Config, statistics: bool, mode: &str, topic: &str, value: &str) -> Self {
        let path = std::env::var("GZ_CPP_PEER")
            .unwrap_or_else(|_| format!("{}/artifacts/cpp-peer", env!("CARGO_MANIFEST_DIR")));
        let mut child = Command::new(path)
            .args([mode, topic, value])
            .env("GZ_PARTITION", &config.partition)
            .env("GZ_IP", config.interface.to_string())
            .env("GZ_DISCOVERY_MULTICAST_IP", config.multicast.to_string())
            .env("GZ_DISCOVERY_MSG_PORT", config.message_port.to_string())
            .env("GZ_DISCOVERY_SRV_PORT", config.service_port.to_string())
            .env("GZ_TRANSPORT_IMPLEMENTATION", "zeromq")
            .env(
                "GZ_TRANSPORT_TOPIC_STATISTICS",
                if statistics { "1" } else { "0" },
            )
            .env("GZ_VERBOSE", "0")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = mpsc::sync_channel(32);
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if tx.send(line.unwrap()).is_err() {
                    break;
                }
            }
        });
        Self { child, lines }
    }
    fn line(&mut self) -> String {
        self.lines
            .recv_timeout(Duration::from_secs(8))
            .unwrap_or_else(|e| panic!("native response {e:?}; status {:?}", self.child.try_wait()))
    }
    fn command(&mut self, command: &str) -> String {
        writeln!(self.child.stdin.as_mut().unwrap(), "{command}").unwrap();
        self.line()
    }
    fn finish(&mut self, command: bool) {
        if command {
            writeln!(self.child.stdin.as_mut().unwrap(), "QUIT").unwrap();
        }
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                let mut stderr = String::new();
                self.child
                    .stderr
                    .take()
                    .unwrap()
                    .read_to_string(&mut stderr)
                    .unwrap();
                assert!(status.success(), "{status:?}: {stderr}");
                assert!(stderr.is_empty(), "{stderr}");
                return;
            }
            assert!(Instant::now() < end, "native process did not exit");
            thread::sleep(Duration::from_millis(2));
        }
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn connected(publisher: &gz_transport::Publisher, expected: bool) {
    let end = Instant::now()
        + if expected {
            Duration::from_secs(5)
        } else {
            Duration::from_millis(2500)
        };
    loop {
        let actual = publisher.has_connections().unwrap();
        if expected && actual {
            return;
        }
        assert!(
            !actual || expected,
            "mismatched statistics profile registered a connection"
        );
        if Instant::now() >= end {
            assert!(!expected, "no native connection");
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
}
fn monotonic_ms() -> u64 {
    Duration::from(nix::time::clock_gettime(nix::time::ClockId::CLOCK_MONOTONIC).unwrap())
        .as_millis()
        .try_into()
        .unwrap()
}
fn observed(
    peer: &mut Native,
    publisher: &gz_transport::Publisher,
    data: &[u8],
    topic: &str,
    address: &str,
    expected: (bool, u64, usize),
) {
    let (statistics, sequence, count) = expected;
    let before = monotonic_ms();
    publisher.publish(data.to_vec()).unwrap();
    let line = peer.command("CAPTURE");
    let after = monotonic_ms();
    let columns: Vec<_> = line.split_whitespace().collect();
    assert!(columns.len() >= 2, "{line}");
    assert_eq!(
        &columns[..2],
        &["FRAMES", if statistics { "5" } else { "4" }]
    );
    let frames: Vec<_> = columns[2..].iter().map(|s| unhex(s)).collect();
    assert_eq!(frames.len(), if statistics { 5 } else { 4 });
    assert_eq!(
        frames[..4],
        [topic.as_bytes(), address.as_bytes(), data, INT.as_bytes()]
    );
    if statistics {
        assert_eq!(frames[4].len(), 16);
        let stamp = u64::from_ne_bytes(frames[4][..8].try_into().unwrap());
        assert!(
            before <= stamp && stamp <= after,
            "{before} {stamp} {after}"
        );
        assert_eq!(
            u64::from_ne_bytes(frames[4][8..].try_into().unwrap()),
            sequence
        );
    }
    let expected = format!("COUNT {count} {} {INT}", hex(data));
    let end = Instant::now() + Duration::from_secs(1);
    loop {
        let actual = peer.command("COUNT");
        if actual == expected {
            break;
        }
        assert!(Instant::now() < end, "{actual} != {expected}");
        thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn rust_publications_preserve_actual_native_framing_lifecycle_and_mode_isolation() {
    for (statistics, native_mode) in [(false, false), (true, true), (false, true), (true, false)] {
        let cfg = config(statistics);
        let owner = Node::new(cfg.clone()).unwrap();
        let mut first = owner.advertise("/foo", INT).unwrap();
        let second = owner.advertise("/bar", INT).unwrap();
        // Process this command before any native subscriber exists, so the
        // first observed statistics packet must still carry sequence zero.
        first.publish(value(99)).unwrap();
        owner.known_publisher_topics().unwrap();
        let info = owner.known_topic_publishers("/foo").unwrap();
        assert_eq!(info.len(), 1);
        let address = info[0].address.clone();
        let topic = info[0].topic.clone();
        let mut peer = Native::start(
            &cfg,
            native_mode,
            "statistics-wire-subscriber",
            "/foo",
            &address,
        );
        assert_eq!(peer.line(), "READY");
        if statistics != native_mode {
            connected(&first, false);
            first.publish(value(5)).unwrap();
            thread::sleep(Duration::from_millis(300));
            assert!(peer.command("COUNT").starts_with("COUNT 0 "));
            peer.finish(true);
            continue;
        }
        connected(&first, true);
        connected(&second, true);
        // HasConnections records SDK membership, not raw SUB pipe completion.
        // This independent capture pipe needs its own fixture settling interval.
        thread::sleep(Duration::from_millis(200));
        observed(
            &mut peer,
            &first,
            &value(5),
            &topic,
            &address,
            (statistics, 0, 1),
        );
        observed(
            &mut peer,
            &first,
            &value(6),
            &topic,
            &address,
            (statistics, 1, 2),
        );
        let bar = owner.known_topic_publishers("/bar").unwrap();
        observed(
            &mut peer,
            &second,
            &value(7),
            &bar[0].topic,
            &address,
            (statistics, 0, 3),
        );
        observed(
            &mut peer,
            &first,
            &[0, 255, 1, 0],
            &topic,
            &address,
            (statistics, 2, 4),
        );
        observed(&mut peer, &first, &[], &topic, &address, (statistics, 3, 5));
        drop(first);
        first = owner.advertise("/foo", INT).unwrap();
        connected(&first, true);
        observed(
            &mut peer,
            &first,
            &value(8),
            &topic,
            &address,
            (statistics, 4, 6),
        );
        drop(owner);
        observed(
            &mut peer,
            &first,
            &value(9),
            &topic,
            &address,
            (statistics, 5, 7),
        );
        peer.finish(true);
    }
}
#[test]
fn native_publications_reach_rust_without_modifying_opaque_or_empty_payloads() {
    for (statistics, native_mode) in [(false, false), (true, true), (false, true), (true, false)] {
        let cfg = config(statistics);
        let node = Node::new(cfg.clone()).unwrap();
        let foo = node.subscribe("/foo", INT).unwrap();
        let bar = node.subscribe("/bar", INT).unwrap();
        let mut peer = Native::start(
            &cfg,
            native_mode,
            "statistics-wire-publisher",
            "/foo",
            "/bar",
        );
        let _address = peer.line();
        let topic = peer.line();
        let other = peer.line();
        assert_eq!(peer.line(), "READY");
        let same = statistics == native_mode;
        assert_eq!(
            peer.command(&format!("CONNECTIONS {}", u8::from(same))),
            format!("CONNECTIONS {0} {0}", u8::from(same))
        );
        for (command, data) in [
            ("PUB foo 5", value(5)),
            ("OPAQUE foo", vec![0, 255, 1, 0]),
            ("EMPTY foo", vec![]),
        ] {
            assert_eq!(peer.command(command), format!("PUBLISHED 1 {}", hex(&data)));
            if same {
                let message = foo.recv_timeout(Duration::from_secs(3)).unwrap();
                assert_eq!(message.data, data);
                assert_eq!(message.type_name, INT);
                assert_eq!(message.topic, topic);
            } else {
                assert_eq!(
                    foo.recv_timeout(Duration::from_millis(300)),
                    Err(Error::Timeout)
                );
            }
        }
        assert_eq!(
            peer.command("PUB bar 7"),
            format!("PUBLISHED 1 {}", hex(&value(7)))
        );
        if same {
            let message = bar.recv_timeout(Duration::from_secs(3)).unwrap();
            assert_eq!(message.data, value(7));
            assert_eq!(message.topic, other);
        } else {
            assert_eq!(
                bar.recv_timeout(Duration::from_millis(300)),
                Err(Error::Timeout)
            );
        }
        peer.finish(true);
    }
}
#[test]
fn statistics_discovery_preserves_real_native_rpc_in_both_directions_and_modes() {
    for (statistics, native_mode) in [(false, false), (true, true), (false, true), (true, false)] {
        let cfg = config(statistics);
        let node = Node::new(cfg.clone()).unwrap();
        let mut provider = Native::start(
            &cfg,
            native_mode,
            "statistics-wire-rpc-provider",
            "/native",
            "",
        );
        assert_eq!(provider.line(), "READY");
        let result = node.request(
            "/native",
            INT,
            INT,
            value(5),
            Duration::from_millis(if statistics == native_mode { 5000 } else { 300 }),
        );
        if statistics == native_mode {
            assert_eq!(result, Ok((value(5), true)));
            assert_eq!(provider.command("COUNT"), "COUNT 1");
        } else {
            assert_eq!(result, Err(Error::Timeout));
            assert_eq!(provider.command("COUNT"), "COUNT 0");
        }
        provider.finish(true);
        let entries = Arc::new(AtomicUsize::new(0));
        let count = entries.clone();
        let _service = node
            .serve_callback("/rust", INT, INT, move |bytes| {
                count.fetch_add(1, Ordering::AcqRel);
                assert_eq!(bytes, value(5));
                Ok((value(5), true))
            })
            .unwrap();
        let mut caller = Native::start(
            &cfg,
            native_mode,
            "statistics-wire-rpc-request",
            "/rust",
            if statistics == native_mode {
                "present"
            } else {
                "absent"
            },
        );
        assert_eq!(
            caller.line(),
            if statistics == native_mode {
                "RPC 1 1 1 5"
            } else {
                "RPC 0 0 0 0"
            }
        );
        caller.finish(false);
        assert_eq!(
            entries.load(Ordering::Acquire),
            usize::from(statistics == native_mode)
        );
    }
}

#[test]
fn local_pubsub_resolver_and_each_service_path_require_the_same_wire_mode() {
    for (provider_mode, caller_mode) in [(false, false), (true, true), (false, true), (true, false)]
    {
        let cfg = config(provider_mode);
        let mut other = cfg.clone();
        other.statistics_wire = caller_mode;
        let owner = Node::new(cfg).unwrap();
        let caller = Node::new(other).unwrap();
        let same = provider_mode == caller_mode;
        let publisher = owner.advertise("/publication", INT).unwrap();
        let receiver = caller.subscribe("/publication", INT).unwrap();
        publisher.publish(value(5)).unwrap();
        if same {
            assert_eq!(
                receiver.recv_timeout(Duration::from_secs(1)).unwrap().data,
                value(5)
            );
            assert_eq!(
                receiver.recv_timeout(Duration::from_millis(30)),
                Err(Error::Timeout)
            );
        } else {
            assert_eq!(
                receiver.recv_timeout(Duration::from_millis(300)),
                Err(Error::Timeout)
            );
            assert!(caller.known_publisher_topics().unwrap().is_empty());
        }
        let queue = owner.serve("/queued", INT, INT).unwrap();
        assert_eq!(
            caller
                .resolve_service_types("/queued", "", "", Duration::ZERO)
                .unwrap(),
            if same {
                gz_transport::ServiceTypeResolution::Resolved(gz_transport::ServiceTypes {
                    request_type: INT.into(),
                    response_type: INT.into(),
                })
            } else {
                gz_transport::ServiceTypeResolution::NoProviders
            }
        );
        let pending = caller
            .begin_request("/queued", INT, INT, value(5), Duration::from_millis(300))
            .unwrap();
        if same {
            let request = queue.recv_timeout(Duration::from_millis(300)).unwrap();
            assert_eq!(request.data, value(5));
            request.respond(value(5), true).unwrap();
            assert_eq!(pending.wait(Duration::from_secs(1)), Ok((value(5), true)));
        } else {
            assert!(matches!(
                queue.recv_timeout(Duration::from_millis(300)),
                Err(Error::Timeout)
            ));
            assert_eq!(pending.wait(Duration::from_secs(1)), Err(Error::Timeout));
        }
        let entries = Arc::new(AtomicUsize::new(0));
        let count = entries.clone();
        let _response = owner
            .serve_callback("/inline", INT, INT, move |bytes| {
                count.fetch_add(1, Ordering::AcqRel);
                assert!(same, "wrong-mode service callback entered");
                assert_eq!(bytes, value(5));
                Ok((value(5), true))
            })
            .unwrap();
        let responses = Arc::new(AtomicUsize::new(0));
        let count = responses.clone();
        let callback = caller
            .request_callback(
                "/inline",
                INT,
                INT,
                value(5),
                Duration::from_millis(300),
                move |bytes, result| {
                    count.fetch_add(1, Ordering::AcqRel);
                    assert!(same, "wrong-mode response callback entered");
                    assert_eq!(bytes, value(5));
                    assert!(result);
                },
            )
            .unwrap();
        assert_eq!(entries.load(Ordering::Acquire), usize::from(same));
        assert_eq!(responses.load(Ordering::Acquire), usize::from(same));
        assert_eq!(
            callback.wait(Duration::from_secs(1)),
            if same { Ok(()) } else { Err(Error::Timeout) }
        );
        assert_eq!(entries.load(Ordering::Acquire), usize::from(same));
        assert_eq!(responses.load(Ordering::Acquire), usize::from(same));
        let count = entries.clone();
        let _oneway = owner
            .serve_oneway_callback("/oneway", INT, move |bytes| {
                count.fetch_add(1, Ordering::AcqRel);
                assert!(same, "wrong-mode one-way callback entered");
                assert_eq!(bytes, value(5));
                Ok(())
            })
            .unwrap();
        assert_eq!(
            caller.request_oneway("/oneway", INT, value(5), Duration::from_millis(300)),
            if same { Ok(()) } else { Err(Error::Timeout) }
        );
        assert_eq!(entries.load(Ordering::Acquire), if same { 2 } else { 0 });
    }
}
