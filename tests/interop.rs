use gz_msgs::{prost::Message, stringmsg::StringMsg};
use gz_transport::{Config, Error, Node};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
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
struct Peer(Child);
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
            .stdout(Stdio::piped())
            .spawn()
            .expect("Build the required real C++ peer with scripts/build-cpp-peer.sh");
        let mut ready = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut ready)
            .unwrap();
        assert_eq!(ready.trim(), "READY");
        Self(child)
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
