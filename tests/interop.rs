// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Subscriber-only assertion ports: Copyright (C) 2014 Open Source Robotics Foundation.
use gz_msgs::{prost::Message, stringmsg::StringMsg};
use gz_transport::{Config, Error, Node};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdout, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
const TYPE: &str = "gz.msgs.StringMsg";
fn data(value: &str) -> Vec<u8> {
    StringMsg {
        data: value.into(),
        ..Default::default()
    }
    .encode_to_vec()
}
fn config() -> Config {
    let id = uuid::Uuid::new_v4();
    Config {
        partition: format!("rust-{}", id),
        message_port: 10317,
        service_port: 10318,
        ..Default::default()
    }
}

// Original Node_TEST.cc duplicate-publisher assertions: Copyright (C) 2014
// Open Source Robotics Foundation. Invalid C++ handles map to explicit errors.
#[test]
fn original_advertise_two_equal_topics() {
    let config = config();
    let first = Node::new(config.clone()).unwrap();
    let second = Node::new(config).unwrap();
    let _one = first.advertise("/foo", "gz.msgs.Int32").unwrap();
    assert!(matches!(
        first.advertise("/foo", TYPE),
        Err(Error::AlreadyAdvertised)
    ));
    let _two = second.advertise("/foo", TYPE).unwrap();
}

#[test]
fn original_advertise_same_topic_same_type_on_same_node() {
    let node = Node::new(config()).unwrap();
    let _one = node.advertise("/foo", "gz.msgs.Int32").unwrap();
    assert!(matches!(
        node.advertise("/foo", "gz.msgs.Int32"),
        Err(Error::AlreadyAdvertised)
    ));
}

#[test]
fn original_advertise_relative_topic_twice() {
    let node = Node::new(config()).unwrap();
    let _one = node.advertise("relative_topic", "gz.msgs.Int32").unwrap();
    assert!(matches!(
        node.advertise("relative_topic", "gz.msgs.Int32"),
        Err(Error::AlreadyAdvertised)
    ));
}

#[test]
fn original_advertise_namespaced_topic_twice() {
    let mut config = config();
    config.set_namespace("my_ns").unwrap();
    let node = Node::new(config).unwrap();
    let _one = node.advertise("ns_topic", "gz.msgs.Int32").unwrap();
    assert!(matches!(
        node.advertise("ns_topic", "gz.msgs.Int32"),
        Err(Error::AlreadyAdvertised)
    ));
    assert!(matches!(
        node.advertise("/my_ns/ns_topic", TYPE),
        Err(Error::AlreadyAdvertised)
    ));
}
struct Peer(Child, BufReader<ChildStdout>);
impl Peer {
    fn start(config: &Config, mode: &str, topic: &str, value: &str) -> Self {
        let path = std::env::var("GZ_CPP_PEER")
            .unwrap_or_else(|_| format!("{}/artifacts/cpp-peer", env!("CARGO_MANIFEST_DIR")));
        let mut child = Command::new(path)
            .args([mode, topic, value])
            .env("GZ_PARTITION", &config.partition)
            .env("GZ_IP", config.interface.to_string())
            .env("GZ_DISCOVERY_MULTICAST_IP", config.multicast.to_string())
            .env(
                "GZ_TRANSPORT_SNDHWM",
                config.send_high_watermark.to_string(),
            )
            .env(
                "GZ_TRANSPORT_RCVHWM",
                config.receive_high_watermark.to_string(),
            )
            .env("GZ_DISCOVERY_MSG_PORT", config.message_port.to_string())
            .env("GZ_DISCOVERY_SRV_PORT", config.service_port.to_string())
            .env("GZ_TRANSPORT_IMPLEMENTATION", "zeromq")
            .env("GZ_TRANSPORT_TOPIC_STATISTICS", "0")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("Build the required real C++ peer with scripts/build-cpp-peer.sh");
        let mut ready = String::new();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        stdout.read_line(&mut ready).unwrap();
        assert_eq!(ready.trim(), "READY");
        Self(child, stdout)
    }
    fn success(&mut self) {
        assert!(self.0.wait().unwrap().success());
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
fn cpp_publisher_rust_subscriber() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let sub = node.subscribe("/wire", TYPE).unwrap();
    let _peer = Peer::start(&config, "publish", "/wire", "cpp");
    let message = sub.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(message.data, data("cpp"));
    assert_eq!(message.type_name, TYPE);
    assert_eq!(message.topic, format!("@/{}@/wire", config.partition));
}
#[test]
fn rust_publisher_cpp_subscriber_delayed_start() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let publisher = node.advertise("/wire", TYPE).unwrap();
    thread::sleep(Duration::from_millis(1100));
    let mut peer = Peer::start(&config, "subscribe", "/wire", "rust");
    for _ in 0..100 {
        publisher.publish(data("rust")).unwrap();
        thread::sleep(Duration::from_millis(20));
        if peer.0.try_wait().unwrap().is_some() {
            break;
        }
    }
    peer.success();
}
#[test]
fn rust_request_cpp_service() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let _peer = Peer::start(&config, "serve", "/echo", "-cpp");
    let (response, result) = node
        .request("/echo", TYPE, TYPE, data("rust"), Duration::from_secs(5))
        .unwrap();
    assert!(result);
    assert_eq!(response, data("rust-cpp"));
}
#[test]
fn cpp_request_rust_service() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let service = node.serve("/echo", TYPE, TYPE).unwrap();
    thread::sleep(Duration::from_millis(1100));
    let mut peer = Peer::start(&config, "request", "/echo", "cpp");
    let request = service.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(request.data, data("cpp"));
    request.respond(data("cpp-rust"), true).unwrap();
    peer.success();
}
#[test]
fn timeout_partition_shutdown_and_local_delivery() {
    let config = config();
    let mut node = Node::new(config.clone()).unwrap();
    let sub = node.subscribe("/local", TYPE).unwrap();
    let wrong = node.subscribe("/local", "wrong").unwrap();
    let publisher = node.advertise("/local", TYPE).unwrap();
    publisher.publish(data("local")).unwrap();
    assert_eq!(
        sub.recv_timeout(Duration::from_secs(1)).unwrap().data,
        data("local")
    );
    assert_eq!(
        wrong.recv_timeout(Duration::from_millis(10)),
        Err(Error::Timeout)
    );
    assert_eq!(
        node.request("/missing", TYPE, TYPE, vec![], Duration::from_millis(30)),
        Err(Error::Timeout)
    );
    let other = Node::new(Config {
        partition: "different".into(),
        ..config
    })
    .unwrap();
    let isolated = other.subscribe("/local", TYPE).unwrap();
    publisher.publish(data("private")).unwrap();
    assert_eq!(
        isolated.recv_timeout(Duration::from_millis(30)),
        Err(Error::Timeout)
    );
    let start = Instant::now();
    node.shutdown();
    assert!(start.elapsed() < Duration::from_secs(1));
    assert_eq!(publisher.publish(vec![]), Err(Error::Shutdown));
    assert_eq!(
        sub.recv_timeout(Duration::from_millis(10)).unwrap().data,
        data("private")
    );
    assert_eq!(
        sub.recv_timeout(Duration::from_millis(10)),
        Err(Error::Shutdown)
    );
}

#[test]
fn reconnect_and_delayed_responder() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let sub = node.subscribe("/reconnect", TYPE).unwrap();
    let peer = Peer::start(&config, "publish", "/reconnect", "before");
    assert_eq!(
        sub.recv_timeout(Duration::from_secs(4)).unwrap().data,
        data("before")
    );
    drop(peer);
    let _peer = Peer::start(&config, "publish", "/reconnect", "after");
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let message = sub
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
        if message.data == data("after") {
            break;
        }
    }
    thread::scope(|scope| {
        let request =
            scope.spawn(|| node.request("/late", TYPE, TYPE, data("late"), Duration::from_secs(4)));
        thread::sleep(Duration::from_millis(1100));
        let _server = Peer::start(&config, "serve", "/late", "-cpp");
        assert_eq!(request.join().unwrap(), Ok((data("late-cpp"), true)));
    });
}
#[test]
fn same_node_service_failure_and_concurrent_cancel() {
    let node = Node::new(config()).unwrap();
    let service = node.serve("/local_service", TYPE, TYPE).unwrap();
    thread::scope(|scope| {
        let request = scope.spawn(|| {
            node.request(
                "/local_service",
                TYPE,
                TYPE,
                data("input"),
                Duration::from_secs(2),
            )
        });
        let incoming = service.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(incoming.data, data("input"));
        incoming.respond(data("declined"), false).unwrap();
        assert_eq!(request.join().unwrap(), Ok((data("declined"), false)));
    });
    thread::scope(|scope| {
        let request =
            scope.spawn(|| node.request("/never", TYPE, TYPE, vec![], Duration::from_secs(30)));
        thread::sleep(Duration::from_millis(20));
        let start = Instant::now();
        node.cancel();
        assert_eq!(request.join().unwrap(), Err(Error::Shutdown));
        assert!(start.elapsed() < Duration::from_secs(1));
    });
    assert!(matches!(
        service.recv_timeout(Duration::from_secs(1)),
        Err(Error::Shutdown)
    ));
}

#[test]
fn malformed_discovery_is_ignored_and_counted() {
    let node = Node::new(config()).unwrap();
    let sender = std::net::UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    sender
        .send_to(&[255], (std::net::Ipv4Addr::LOCALHOST, 10317))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    while node.statistics().malformed_frames == 0 {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(node.statistics().malformed_frames, 1);
}

#[test]
fn configured_cpp_peer_partition_interface_ports_and_namespace() {
    let config = Config {
        partition: uuid::Uuid::new_v4().to_string(),
        namespace: "robot".into(),
        ..Default::default()
    }
    .with_environment(|key| match key {
        "GZ_DISCOVERY_MULTICAST_IP" => Some("239.255.0.8".into()),
        "GZ_DISCOVERY_MSG_PORT" => Some("12371".into()),
        "GZ_DISCOVERY_SRV_PORT" => Some("12372".into()),
        "GZ_TRANSPORT_SNDHWM" | "GZ_TRANSPORT_RCVHWM" => Some("16".into()),
        _ => None,
    })
    .unwrap();
    let node = Node::new(config.clone()).unwrap();
    let sub = node.subscribe("wire", TYPE).unwrap();
    let _peer = Peer::start(&config, "publish", "/robot/wire", "configured");
    let message = sub.recv_timeout(Duration::from_secs(4)).unwrap();
    assert_eq!(message.data, data("configured"));
    assert_eq!(message.topic, format!("@/{}@/robot/wire", config.partition));
}
#[test]
fn cpp_environment_integer_prefix_fallback_and_duplicate_port_differential() {
    let path = std::env::var("GZ_CPP_PEER")
        .unwrap_or_else(|_| format!("{}/artifacts/cpp-peer", env!("CARGO_MANIFEST_DIR")));
    for values in [
        [
            ("GZ_DISCOVERY_MSG_PORT", "12375tail"),
            ("GZ_DISCOVERY_SRV_PORT", "12376"),
            ("GZ_TRANSPORT_SNDHWM", "12tail"),
            ("GZ_TRANSPORT_RCVHWM", "-1"),
        ],
        [
            ("GZ_DISCOVERY_MSG_PORT", "65535"),
            ("GZ_DISCOVERY_SRV_PORT", "65535"),
            ("GZ_TRANSPORT_SNDHWM", "bad"),
            ("GZ_TRANSPORT_RCVHWM", "2147483648"),
        ],
    ] {
        let config = Config::default()
            .with_environment(|key| {
                values
                    .iter()
                    .find(|(k, _)| key == *k)
                    .map(|(_, v)| v.to_string())
            })
            .unwrap();
        let mut peer = Command::new(&path);
        peer.args(["config", "unused", "unused"])
            .env("GZ_PARTITION", "native-config")
            .env("GZ_IP", "127.0.0.1")
            .env("GZ_TRANSPORT_IMPLEMENTATION", "zeromq")
            .env("GZ_TRANSPORT_TOPIC_STATISTICS", "0");
        for (key, value) in values {
            peer.env(key, value);
        }
        let output = peer.output().unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            format!(
                "CONFIG native-config {} {} {} {}",
                config.message_port,
                config.service_port,
                config.send_high_watermark,
                config.receive_high_watermark
            )
        );
    }
}

#[test]
fn cpp_unknown_application_fields_are_received_without_reencoding() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let sub = node.subscribe("/unknown", TYPE).unwrap();
    let _peer = Peer::start(&config, "publish-unknown", "/unknown", "opaque");
    let mut expected = data("opaque");
    expected.extend([0xf8, 0x07, 0x01]);
    assert_eq!(
        sub.recv_timeout(Duration::from_secs(4)).unwrap().data,
        expected
    );
}
#[test]
fn rust_unknown_application_fields_reach_cpp_unchanged() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let publisher = node.advertise("/unknown", TYPE).unwrap();
    let mut peer = Peer::start(&config, "subscribe-unknown", "/unknown", "opaque");
    let mut bytes = data("opaque");
    bytes.extend([0xf8, 0x07, 0x01]);
    for _ in 0..100 {
        publisher.publish(bytes.clone()).unwrap();
        thread::sleep(Duration::from_millis(20));
        if peer.0.try_wait().unwrap().is_some() {
            break;
        }
    }
    peer.success();
}

#[test]
fn oneway_services_interoperate_without_reply_waiters() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let mut peer = Peer::start(&config, "serve-oneway", "/oneway_cpp", "rust");
    node.request_oneway("/oneway_cpp", TYPE, data("rust"), Duration::from_secs(5))
        .unwrap();
    peer.success(); // C++ executed the callback; it never replies for Empty.
    let service = node.serve("/oneway_rust", TYPE, "gz.msgs.Empty").unwrap();
    let mut peer = Peer::start(&config, "request-oneway", "/oneway_rust", "cpp");
    let request = service.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(request.data, data("cpp"));
    assert!(!request.expects_response());
    request.respond(Vec::new(), true).unwrap();
    peer.success();
    assert_eq!(node.statistics().reply_timeouts, 0);
}

#[test]
fn oneway_local_handoff_bounds_and_matching() {
    let config = Config {
        capacity: 2,
        max_message_bytes: 32,
        ..config()
    };
    let mut node = Node::new(config).unwrap();
    let service = node.serve("/oneway", TYPE, "gz.msgs.Empty").unwrap();
    for value in ["first", "second", "third"] {
        node.request_oneway("/oneway", TYPE, data(value), Duration::from_secs(1))
            .unwrap();
        let request = service.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(!request.expects_response());
        assert_eq!(request.data, data(value));
        request.respond(vec![0; 64], false).unwrap(); // Unused response data is ignored.
    }
    for _ in 0..2 {
        node.request_oneway("/oneway", TYPE, data("queued"), Duration::from_secs(1))
            .unwrap();
    }
    assert_eq!(
        node.request_oneway("/oneway", TYPE, data("full"), Duration::from_secs(1)),
        Err(Error::QueueFull)
    );
    assert_eq!(node.statistics().queue_drops, 1);
    assert_eq!(
        node.request_oneway("/oneway", "wrong", vec![], Duration::from_millis(25)),
        Err(Error::Timeout)
    );
    assert_eq!(
        node.request_oneway("invalid name", TYPE, vec![], Duration::from_secs(1)),
        Err(Error::InvalidName)
    );
    assert_eq!(
        node.request_oneway("/oneway", TYPE, vec![0; 33], Duration::from_secs(1)),
        Err(Error::InvalidFrame)
    );
    node.shutdown();
    assert_eq!(
        node.request_oneway("/oneway", TYPE, vec![], Duration::from_secs(1)),
        Err(Error::Shutdown)
    );
}

#[test]
fn native_host_service_accepts_other_loopback_aliases() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let peer_config = Config {
        interface: "127.0.0.2".parse().unwrap(),
        ..config
    };
    let _peer = Peer::start(&peer_config, "serve-host", "/host_echo", "-host");
    assert_eq!(
        node.request(
            "/host_echo",
            TYPE,
            TYPE,
            data("rust"),
            Duration::from_secs(5)
        ),
        Ok((data("rust-host"), true))
    );
}

#[test]
fn cpp_inspects_subscriber_only_topics_until_the_last_lease_is_dropped() {
    for (type_name, topic) in [
        ("gz.msgs.Int32", "/subscriber_only"),
        ("gz.msgs.Int32", "/unsub_test"),
        ("", "/wildcard_only"),
    ] {
        let config = config();
        let node = Node::new(config.clone()).unwrap();
        let wire_type = if type_name.is_empty() {
            "google.protobuf.Message"
        } else {
            type_name
        };
        let mut peer = Peer::start(&config, "inspect-subscriber", topic, wire_type);
        let first = node.subscribe(topic, type_name).unwrap();
        let last = node.subscribe(topic, type_name).unwrap();
        let mut line = String::new();
        peer.1.read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "FOUND");
        drop(first);
        writeln!(peer.0.stdin.as_mut().unwrap(), "KEEP").unwrap();
        line.clear();
        peer.1.read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "KEPT");
        drop(last);
        writeln!(peer.0.stdin.as_mut().unwrap(), "GONE").unwrap();
        peer.success();
    }
}

#[test]
fn original_oneway_int32_five_dispatches_once_and_rejects_invalid_service() {
    // Node_TEST.cc ServiceWithoutOutputCallAsync / srvWithoutOutput assertions.
    let node = Node::new(config()).unwrap();
    let type_name = "gz.msgs.Int32";
    assert!(matches!(
        node.serve("invalid service", type_name, "gz.msgs.Empty"),
        Err(Error::InvalidName)
    ));
    let service = node.serve("/foo", type_name, "gz.msgs.Empty").unwrap();
    let advertised = node.advertised_services().unwrap();
    assert_eq!(advertised.len(), 1);
    assert_eq!(advertised[0], "/foo");
    let request = gz_msgs::int32::Int32 {
        data: 5,
        ..Default::default()
    }
    .encode_to_vec();
    assert_eq!(
        node.request_oneway(
            "invalid service",
            type_name,
            request.clone(),
            Duration::from_secs(1)
        ),
        Err(Error::InvalidName)
    );
    node.request_oneway("/foo", type_name, request, Duration::from_secs(1))
        .unwrap();
    // A successful local handoff has queued the event, so draining now observes
    // actual delivered requests without a sleep or a fabricated callback count.
    let mut counter = 0;
    while let Ok(request) = service.recv_timeout(Duration::ZERO) {
        assert_eq!(
            gz_msgs::int32::Int32::decode(request.data.as_slice())
                .unwrap()
                .data,
            5
        );
        assert!(!request.expects_response());
        counter += 1;
    }
    assert_eq!(counter, 1);
    assert_eq!(
        node.unadvertise_service("invalid service"),
        Err(Error::InvalidName)
    );
    node.unadvertise_service("/foo").unwrap();
    assert!(node.advertised_services().unwrap().is_empty());
}

#[test]
fn advertised_service_topics_are_local_qualified_and_drop_aware() {
    let mut config = config();
    config.set_namespace("ns").unwrap();
    let node = Node::new(config.clone()).unwrap();
    let other = Node::new(config).unwrap();
    let _foreign = other.serve("/foreign", TYPE, TYPE).unwrap();
    let _independent = other.serve("relative", TYPE, TYPE).unwrap();
    let absolute = node.serve("/absolute", TYPE, TYPE).unwrap();
    let relative = node.serve("relative", TYPE, TYPE).unwrap();
    assert!(matches!(
        node.serve("relative", TYPE, TYPE),
        Err(Error::AlreadyAdvertised)
    ));
    assert!(matches!(
        node.serve("/ns/relative", "gz.msgs.Int32", "gz.msgs.Int32"),
        Err(Error::AlreadyAdvertised)
    ));
    assert_eq!(
        node.advertised_services().unwrap(),
        ["/absolute", "/ns/relative"]
    );
    node.unadvertise_service("/absent").unwrap();
    node.unadvertise_service("relative").unwrap();
    assert_eq!(node.advertised_services().unwrap(), ["/absolute"]);
    drop((relative, absolute));
    assert!(node.advertised_services().unwrap().is_empty());
    let replacement = node.serve("relative", TYPE, TYPE).unwrap();
    assert_eq!(node.advertised_services().unwrap(), ["/ns/relative"]);
    drop(replacement);
}

#[test]
fn native_local_service_enumeration_and_idempotent_withdrawal() {
    let config = config();
    let mut peer = Peer::start(&config, "service-lifecycle", "relative", "/relative");
    peer.success();
}

#[test]
fn cpp_requester_reaches_service_recreated_before_old_receiver_is_dropped() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let retired = node.serve("/recreated_service", TYPE, TYPE).unwrap();
    node.unadvertise_service("/recreated_service").unwrap();
    let replacement = node.serve("/recreated_service", TYPE, TYPE).unwrap();
    assert!(matches!(
        node.serve("/recreated_service", "gz.msgs.Int32", "gz.msgs.Int32"),
        Err(Error::AlreadyAdvertised)
    ));
    let mut peer = Peer::start(&config, "request", "/recreated_service", "cpp");
    let request = replacement.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(request.data, data("cpp"));
    request.respond(data("cpp-rust"), true).unwrap();
    peer.success();
    assert!(matches!(
        retired.recv_timeout(Duration::ZERO),
        Err(Error::Shutdown)
    ));
}

#[test]
fn native_as_valid_topic_preserves_scan_order_utf8_and_byte_boundaries() {
    let mut cases: Vec<(String, String)> = [
        ("/", ""),
        ("//", ""),
        ("///", ""),
        ("////", ""),
        ("a///b", "a/b"),
        ("a////b", "ab"),
        ("a/~/b", ""),
        ("a:~=b", ""),
        (":", ":"),
        (":=", ""),
        (":==", "="),
        ("a/", "a/"),
        ("a:", "a:"),
        ("é🦀 b", "é🦀_b"),
        ("a\u{a0}b", "a\u{a0}b"),
        ("a\tb\nc", "a\tb\nc"),
        ("a\0b", "a\0b"),
    ]
    .into_iter()
    .map(|(a, b)| (a.into(), b.into()))
    .collect();
    cases.extend([
        ("a".repeat(65535), "a".repeat(65535)),
        ("a".repeat(65536), String::new()),
        ("é".repeat(32767) + "a", "é".repeat(32767) + "a"),
        ("é".repeat(32768), String::new()),
        ("@".repeat(65536) + "a", "a".into()),
    ]);
    let peer = std::env::var("GZ_CPP_PEER")
        .unwrap_or_else(|_| format!("{}/artifacts/cpp-peer", env!("CARGO_MANIFEST_DIR")));
    for (input, expected) in cases {
        assert_eq!(gz_transport::as_valid_topic(&input), expected);
        let mut child = Command::new(&peer)
            .args(["valid-topic-bytes", "unused", "unused"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, expected.as_bytes());
    }
}

#[test]
fn native_peer_storage_confirms_address_and_channel_identity() {
    let peer = std::env::var("GZ_CPP_PEER")
        .unwrap_or_else(|_| format!("{}/artifacts/cpp-peer", env!("CARGO_MANIFEST_DIR")));
    let output = Command::new(peer)
        .args(["peer-storage", "/dual", TYPE])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"2 3 3 1 0\n");
}

#[test]
fn cpp_same_node_publishes_and_serves_the_same_topic() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let subscription = node.subscribe("/dual", TYPE).unwrap();
    let mut peer = Peer::start(&config, "dual", "/dual", "-dual");
    assert_eq!(
        subscription
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .data,
        data("-dual")
    );
    assert_eq!(
        node.request("/dual", TYPE, TYPE, data("rust"), Duration::from_secs(5))
            .unwrap(),
        (data("rust-dual"), true)
    );
    writeln!(peer.0.stdin.as_mut().unwrap(), "QUIT").unwrap();
    peer.success();
}

#[test]
fn cpp_subscriber_observes_immediately_recreated_publisher() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let mut peer = Peer::start(&config, "subscribe-recreate", "/recreated", "sample");
    let publisher = node.advertise("/recreated", TYPE).unwrap();
    thread::scope(|scope| {
        let (tx, rx) = std::sync::mpsc::sync_channel(2);
        let stdout = &mut peer.1;
        scope.spawn(move || {
            for expected in ["FIRST", "SECOND"] {
                let mut line = String::new();
                stdout.read_line(&mut line).unwrap();
                assert_eq!(line.trim(), expected);
                tx.send(expected).unwrap();
            }
        });
        let observed = |publisher: &gz_transport::Publisher, value: &str, stage| {
            let deadline = Instant::now() + Duration::from_secs(8);
            loop {
                publisher.publish(data(value)).unwrap();
                match rx.recv_timeout(Duration::from_millis(10)) {
                    Ok(actual) => {
                        assert_eq!(actual, stage);
                        break;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        assert!(Instant::now() < deadline)
                    }
                    Err(error) => panic!("native observation failed: {error}"),
                }
            }
        };
        observed(&publisher, "sample-before", "FIRST");
        assert!(matches!(
            node.advertise("/recreated", TYPE),
            Err(Error::AlreadyAdvertised)
        ));
        drop(publisher);
        let replacement = node.advertise("/recreated", TYPE).unwrap();
        observed(&replacement, "sample-after", "SECOND");
    });
    peer.success();
}

#[test]
fn cpp_and_rust_inspect_real_service_signatures_without_execution() {
    use gz_transport::{ServiceTypeResolution, ServiceTypes};
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let _provider = Peer::start(&config, "serve", "/native", "unused");
    let types = ServiceTypes {
        request_type: TYPE.into(),
        response_type: TYPE.into(),
    };
    assert_eq!(
        node.resolve_service_types("/native", "", "", Duration::from_secs(5)),
        Ok(ServiceTypeResolution::Resolved(types.clone()))
    );
    assert_eq!(node.service_type_signatures("/native").unwrap(), [types]);
    let service = node.serve("/rust", TYPE, TYPE).unwrap();
    let mut observer = Peer::start(&config, "inspect-service", "/rust", TYPE);
    observer.success();
    assert!(matches!(
        service.recv_timeout(Duration::from_millis(5)),
        Err(Error::Timeout)
    ));
}

// Node_TEST.cc assertion port: Copyright (C) 2014 Open Source Robotics Foundation.
#[test]
fn original_advertise_remapped_topic_twice() {
    let mut config = config();
    config.add_topic_remap("/remap_from", "/remap_to").unwrap();
    let node = Node::new(config).unwrap();
    let _first = node.advertise("/remap_from", "gz.msgs.Int32").unwrap();
    assert!(matches!(
        node.advertise("/remap_from", "gz.msgs.Int32"),
        Err(Error::AlreadyAdvertised)
    ));
}

#[test]
fn remapped_subscription_and_request_use_native_destination() {
    let mut config = config();
    config.add_topic_remap("/alias", "/wire").unwrap();
    let node = Node::new(config.clone()).unwrap();
    let subscription = node.subscribe("/alias", TYPE).unwrap();
    let mut peer = Peer::start(&config, "dual", "/wire", "-cpp");
    let message = subscription.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(message.topic, format!("@/{}@/wire", config.partition));
    assert_eq!(message.type_name, TYPE);
    assert_eq!(message.data, data("-cpp"));
    let (reply, success) = node
        .request("/alias", TYPE, TYPE, data("rust"), Duration::from_secs(5))
        .unwrap();
    assert!(success);
    assert_eq!(reply, data("rust-cpp"));
    writeln!(peer.0.stdin.as_mut().unwrap(), "QUIT").unwrap();
    peer.success();
}

#[test]
fn remapped_publication_and_service_reach_native_clients() {
    let mut config = config();
    config.add_topic_remap("/alias", "/wire").unwrap();
    let node = Node::new(config.clone()).unwrap();
    let publication = node.advertise("/alias", TYPE).unwrap();
    let service = node.serve("/alias", TYPE, TYPE).unwrap();
    let mut subscriber = Peer::start(&config, "subscribe", "/wire", "rust");
    let mut requester = Peer::start(&config, "request", "/wire", "client");
    let request = service.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(request.data, data("client"));
    request.respond(data("client-rust"), true).unwrap();
    requester.success();
    for _ in 0..100 {
        publication.publish(data("rust")).unwrap();
        if subscriber.0.try_wait().unwrap().is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    subscriber.success();
}

#[test]
fn remapping_inspection_and_explicit_withdrawal_retain_native_original_name() {
    use gz_transport::{ServiceTypeResolution, ServiceTypes};
    let mut config = config();
    let mut native = Peer::start(&config, "remap-service-lifecycle", "/from", "/to");
    native.success();
    config.add_topic_remap("/from", "/to").unwrap();
    config.add_topic_remap("/to", "/third").unwrap();
    let node = Node::new(config).unwrap();
    let service = node.serve("/from", TYPE, TYPE).unwrap();
    assert_eq!(node.advertised_services().unwrap(), ["/to"]);
    assert_eq!(
        node.resolve_service_types("/from", "", "", Duration::ZERO),
        Ok(ServiceTypeResolution::NoProviders)
    );
    assert_eq!(
        node.resolve_service_types("/to", "", "", Duration::ZERO),
        Ok(ServiceTypeResolution::Resolved(ServiceTypes {
            request_type: TYPE.into(),
            response_type: TYPE.into()
        }))
    );
    assert!(node.service_type_signatures("/from").unwrap().is_empty());
    assert!(matches!(
        service.recv_timeout(Duration::from_millis(5)),
        Err(Error::Timeout)
    ));
    node.unadvertise_service("/from").unwrap();
    assert_eq!(node.advertised_services().unwrap(), ["/to"]);
    assert!(matches!(
        node.serve("/from", TYPE, TYPE),
        Err(Error::AlreadyAdvertised)
    ));
    node.unadvertise_service("/to").unwrap();
    assert!(node.advertised_services().unwrap().is_empty());
    assert!(node.service_type_signatures("/to").unwrap().is_empty());
}

#[test]
fn publisher_membership_matches_native_reference_and_live_subscriber_withdrawal() {
    let config = config();
    let mut reference = Peer::start(&config, "publisher-membership", "/reference", TYPE);
    reference.success();
    let node = Node::new(config.clone()).unwrap();
    let publisher = node.advertise("/wire", TYPE).unwrap();
    assert!(!publisher.has_connections().unwrap());
    let wrong = node.subscribe("/wire", "wrong.type").unwrap();
    assert!(!publisher.has_connections().unwrap());
    let wildcard = node.subscribe("/wire", "").unwrap();
    assert!(publisher.has_connections().unwrap());
    drop(wildcard);
    assert!(!publisher.has_connections().unwrap());
    let exact = node.subscribe("/wire", TYPE).unwrap();
    assert!(publisher.has_connections().unwrap());
    drop(exact);
    assert!(!publisher.has_connections().unwrap());
    drop(wrong);
    let mut peer = Peer::start(&config, "subscriber-membership", "/wire", "value");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !publisher.has_connections().unwrap() {
        assert!(
            Instant::now() < deadline,
            "native registration did not arrive"
        );
        thread::sleep(Duration::from_millis(10));
    }
    publisher.publish(data("value")).unwrap();
    let mut line = String::new();
    peer.1.read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "RECEIVED");
    writeln!(peer.0.stdin.as_mut().unwrap(), "DROP").unwrap();
    line.clear();
    peer.1.read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "DROPPED");
    let deadline = Instant::now() + Duration::from_secs(10);
    while publisher.has_connections().unwrap() {
        assert!(
            Instant::now() < deadline,
            "native END registration did not arrive"
        );
        thread::sleep(Duration::from_millis(10));
    }
    writeln!(peer.0.stdin.as_mut().unwrap(), "QUIT").unwrap();
    peer.success();
    node.cancel();
    assert_eq!(publisher.has_connections(), Err(Error::Shutdown));
}

// Node_TEST.cc CreatePubSubTwoThreads/CreateSubscriber/cb assertions:
// Copyright (C) 2014 Open Source Robotics Foundation.
fn original_threaded_publication(remapping: bool) {
    let mut config = config();
    if remapping {
        config.add_topic_remap("/foo", "/bar").unwrap();
    }
    let node = Node::new(config.clone()).unwrap();
    let publication = node.advertise("/foo", "gz.msgs.Int32").unwrap();
    let executed = std::sync::atomic::AtomicBool::new(false);
    thread::scope(|scope| {
        let executed = &executed;
        let subscriber = scope.spawn(move || {
            let node = Node::new(config).unwrap();
            let subscription = node.subscribe("/foo", "gz.msgs.Int32").unwrap();
            let message = subscription.recv_timeout(Duration::from_secs(10)).unwrap();
            assert_eq!(
                gz_msgs::int32::Int32::decode(message.data.as_slice())
                    .unwrap()
                    .data,
                5
            );
            executed.store(true, std::sync::atomic::Ordering::Release);
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        while !publication.has_connections().unwrap() {
            assert!(
                Instant::now() < deadline,
                "subscriber registration did not arrive"
            );
            thread::sleep(Duration::from_millis(10));
        }
        publication
            .publish(
                gz_msgs::int32::Int32 {
                    data: 5,
                    ..Default::default()
                }
                .encode_to_vec(),
            )
            .unwrap();
        subscriber.join().unwrap();
    });
    assert!(executed.load(std::sync::atomic::Ordering::Acquire));
}
#[test]
fn original_pub_sub_two_threads_same_topic() {
    original_threaded_publication(false);
}
#[test]
fn original_pub_sub_two_threads_same_topic_remap() {
    original_threaded_publication(true);
}
#[test]
fn original_scope_all() {
    original_threaded_publication(false);
}
#[test]
fn original_scope_all_remap() {
    original_threaded_publication(true);
}

#[test]
fn graceful_subscriber_shutdown_with_live_lease_withdraws_membership() {
    let config = config();
    let publisher_node = Node::new(config.clone()).unwrap();
    let publisher = publisher_node.advertise("/wire", TYPE).unwrap();
    let mut subscriber_node = Node::new(config).unwrap();
    let lease = subscriber_node.subscribe("/wire", TYPE).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !publisher.has_connections().unwrap() {
        assert!(Instant::now() < deadline, "registration did not arrive");
        thread::sleep(Duration::from_millis(10));
    }
    subscriber_node.shutdown();
    assert_eq!(lease.recv_timeout(Duration::ZERO), Err(Error::Shutdown));
    let deadline = Instant::now() + Duration::from_secs(10);
    while publisher.has_connections().unwrap() {
        assert!(Instant::now() < deadline, "graceful shutdown omitted END");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn local_inventory_matches_native_namespaced_alias_lifetimes() {
    let mut config = config();
    let mut native = Peer::start(&config, "local-inventory", "alias", "target");
    native.success();
    config.set_namespace("ns").unwrap();
    config.add_topic_remap("alias", "target").unwrap();
    let node = Node::new(config).unwrap();
    let publication = node.advertise("alias", TYPE).unwrap();
    let first = node.subscribe("alias", TYPE).unwrap();
    let second = node.subscribe("/ns/target", TYPE).unwrap();
    assert_eq!(node.advertised_topics().unwrap(), ["/ns/target"]);
    assert_eq!(node.subscribed_topics().unwrap(), ["/ns/target"]);
    drop(first);
    assert_eq!(node.subscribed_topics().unwrap(), ["/ns/target"]);
    drop(second);
    assert!(node.subscribed_topics().unwrap().is_empty());
    drop(publication);
    assert!(node.advertised_topics().unwrap().is_empty());
    let orphan = node.subscribe("/orphan", TYPE).unwrap();
    assert_eq!(node.subscribed_topics().unwrap(), ["/orphan"]);
    drop(orphan);
    assert!(node.subscribed_topics().unwrap().is_empty());
}

/*
 * Copyright (C) 2014 Open Source Robotics Foundation
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
// Original Node_TEST.cc ordinary/generic callback assertions.
fn received_int32_five(
    subscription: &gz_transport::Subscription,
    executed: &mut bool,
    counter: &mut usize,
) {
    let message = subscription.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(
        gz_msgs::int32::Int32::decode(message.data.as_slice())
            .unwrap()
            .data,
        5
    );
    *executed = true;
    *counter += 1;
}
fn int32_five() -> Vec<u8> {
    gz_msgs::int32::Int32 {
        data: 5,
        ..Default::default()
    }
    .encode_to_vec()
}
fn await_publication_membership(publication: &gz_transport::Publisher) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !publication.has_connections().unwrap() {
        assert!(Instant::now() < deadline, "subscriber did not register");
        thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn original_pub_without_advertise() {
    let mut config = config();
    assert_eq!(
        config.set_namespace("invalid namespace"),
        Err(Error::InvalidName)
    );
    assert!(config.namespace.is_empty());
    let first = Node::new(config.clone()).unwrap();
    let second = Node::new(config).unwrap();
    assert!(first.advertised_topics().unwrap().is_empty());
    assert!(first.subscribed_topics().unwrap().is_empty());
    assert!(first.advertised_services().unwrap().is_empty());
    let publication1 = first.advertise("/foo", "gz.msgs.Int32").unwrap();
    assert_eq!(first.advertised_topics().unwrap(), ["/foo"]);
    let publication2 = second.advertise("/foo", "gz.msgs.Int32").unwrap();
    assert_eq!(second.advertised_topics().unwrap(), ["/foo"]);
    let subscription = second.subscribe("/foo", "gz.msgs.Int32").unwrap();
    assert_eq!(second.subscribed_topics().unwrap(), ["/foo"]);
    await_publication_membership(&publication1);
    let (mut executed, mut counter) = (false, 0);
    publication1.publish(int32_five()).unwrap();
    received_int32_five(&subscription, &mut executed, &mut counter);
    publication2.publish(int32_five()).unwrap();
    received_int32_five(&subscription, &mut executed, &mut counter);
    assert!(executed);
    assert_eq!(counter, 2);
    second.service_type_signatures("/barrier").unwrap();
    assert_eq!(
        subscription.recv_timeout(Duration::ZERO),
        Err(Error::Timeout)
    );
}
#[test]
fn original_pub_sub_same_thread() {
    let node = Node::new(config()).unwrap();
    assert!(matches!(
        node.advertise("invalid topic", "gz.msgs.Int32"),
        Err(Error::InvalidName)
    ));
    let publication = node.advertise("/foo", "gz.msgs.Int32").unwrap();
    assert!(matches!(
        node.subscribe("invalid topic", "gz.msgs.Int32"),
        Err(Error::InvalidName)
    ));
    let subscription = node.subscribe("/foo", "gz.msgs.Int32").unwrap();
    await_publication_membership(&publication);
    for _ in 0..2 {
        let (mut executed, mut counter) = (false, 0);
        publication.publish(int32_five()).unwrap();
        received_int32_five(&subscription, &mut executed, &mut counter);
        assert!(executed);
        assert_eq!(counter, 1);
    }
}
#[test]
fn original_pub_sub_same_thread_generic_cb() {
    let node = Node::new(config()).unwrap();
    let publication = node.advertise("/foo", "gz.msgs.Int32").unwrap();
    let subscription = node.subscribe("/foo", "").unwrap();
    await_publication_membership(&publication);
    for _ in 0..2 {
        let mut executed = false;
        publication.publish(int32_five()).unwrap();
        received_generic_five(&subscription, &mut executed);
        assert!(executed);
    }
}

fn received_generic_five(subscription: &gz_transport::Subscription, executed: &mut bool) {
    let message = subscription.recv_timeout(Duration::from_secs(10)).unwrap();
    let descriptor = gz_msgs::descriptor(&message.type_name).unwrap();
    let dynamic =
        gz_msgs::prost_reflect::DynamicMessage::decode(descriptor, message.data.as_slice())
            .unwrap();
    let content = dynamic.to_text_format();
    assert!(content.contains(&5.to_string()));
    *executed = true;
    assert!(!message.topic_path().contains('@'));
    assert_eq!(message.topic_path(), "/foo");
}
