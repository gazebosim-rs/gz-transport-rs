// Copyright (C) 2014 Open Source Robotics Foundation
// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// The first case preserves NodeTest.relay; the packet controls are supplemental.
use gz_msgs::{
    WireMessage,
    discovery::{
        Discovery,
        discovery::{
            DiscContents, Flags, Publisher, Type,
            publisher::{MessagePublisher, PubType, ServicePublisher},
        },
    },
    prost::Message,
};
use gz_transport::{Config, Error, Node};
use socket2::{Domain, Protocol, Socket, Type as SocketType};
use std::{
    io::{BufRead, BufReader, Write},
    net::{Ipv4Addr, SocketAddrV4, UdpSocket},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

fn config() -> (Config, [Socket; 2]) {
    let reserve = || {
        let socket = Socket::new(Domain::IPV4, SocketType::DGRAM, Some(Protocol::UDP)).unwrap();
        socket.set_reuse_address(true).unwrap();
        socket
            .bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into())
            .unwrap();
        // Keep the port allocated without consuming joined actors' multicast.
        #[cfg(target_os = "linux")]
        socket.set_multicast_all_v4(false).unwrap();
        socket
    };
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
    let config = Config {
        message_port: ports[0],
        service_port: ports[1],
        partition: uuid::Uuid::new_v4().to_string(),
        ..Default::default()
    };
    (config, reservations)
}

fn port(config: &Config, channel: usize) -> u16 {
    if channel == 0 {
        config.message_port
    } else {
        config.service_port
    }
}

fn observer(config: &Config, channel: usize, multicast: bool) -> UdpSocket {
    let address = if multicast {
        config.multicast
    } else {
        Ipv4Addr::new(127, 0, 0, 2)
    };
    let socket = Socket::new(Domain::IPV4, SocketType::DGRAM, Some(Protocol::UDP)).unwrap();
    socket.set_reuse_address(true).unwrap();
    socket
        .bind(&SocketAddrV4::new(address, port(config, channel)).into())
        .unwrap();
    if multicast {
        socket
            .join_multicast_v4(&address, &config.interface)
            .unwrap();
    }
    socket
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    socket.into()
}

fn send(config: &Config, channel: usize, body: &[u8], multicast: bool) {
    let socket = Socket::new(Domain::IPV4, SocketType::DGRAM, Some(Protocol::UDP)).unwrap();
    socket
        .bind(&SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 2), 0).into())
        .unwrap();
    socket.set_multicast_if_v4(&config.interface).unwrap();
    socket.set_multicast_loop_v4(true).unwrap();
    let mut frame = (body.len() as u16).to_ne_bytes().to_vec();
    frame.extend(body);
    let address = if multicast {
        config.multicast
    } else {
        Ipv4Addr::LOCALHOST
    };
    assert_eq!(
        socket
            .send_to(
                &frame,
                &SocketAddrV4::new(address, port(config, channel)).into()
            )
            .unwrap(),
        frame.len()
    );
}

fn receive_if(socket: &UdpSocket, predicate: impl Fn(&Discovery) -> bool) -> (Discovery, Vec<u8>) {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut frame = vec![0; 65507];
    while Instant::now() < deadline {
        let Ok(len) = socket.recv(&mut frame) else {
            continue;
        };
        assert!(len >= 2);
        assert_eq!(u16::from_ne_bytes([frame[0], frame[1]]) as usize, len - 2);
        let body = &frame[2..len];
        let decoded = WireMessage::<Discovery>::decode_compatible(body)
            .unwrap()
            .value()
            .clone();
        if predicate(&decoded) {
            return (decoded, body.to_vec());
        }
    }
    panic!("no matching discovery packet");
}

fn receive(socket: &UdpSocket, process: &str, relay: bool) -> (Discovery, Vec<u8>) {
    receive_if(socket, |message| {
        message.process_uuid == process
            && message.flags.as_ref().is_some_and(|flags| flags.relay) == relay
    })
}

fn packet(config: &Config, channel: usize, relay: bool) -> Discovery {
    Discovery {
        version: if config.statistics_wire { 110 } else { 10 },
        process_uuid: "relay-observed-process".into(),
        r#type: Type::Advertise.into(),
        flags: Some(Flags {
            relay,
            no_relay: false,
        }),
        disc_contents: Some(DiscContents::Pub(Publisher {
            topic: gz_transport::qualified_name(
                &config.partition,
                &config.namespace,
                &format!("/relay-{channel}"),
            )
            .unwrap(),
            address: "tcp://127.0.0.1:40001".into(),
            process_uuid: "relay-observed-process".into(),
            node_uuid: "relay-node".into(),
            scope: 2,
            pub_type: Some(if channel == 0 {
                PubType::MsgPub(MessagePublisher {
                    msg_type: "gz.msgs.Int32".into(),
                    ..Default::default()
                })
            } else {
                PubType::SrvPub(ServicePublisher {
                    request_type: "gz.msgs.Int32".into(),
                    response_type: "gz.msgs.Int32".into(),
                    ..Default::default()
                })
            }),
        })),
        ..Default::default()
    }
}

#[test]
fn original_relay_list_and_bounded_worker_owned_lifecycle() {
    let (config, _ports) = config();
    let node = Node::new(Config {
        capacity: 2,
        ..config
    })
    .unwrap();
    let address = Ipv4Addr::new(127, 0, 0, 123);
    let before = node.discovery_relays().unwrap();
    assert!(!before.contains(&address));
    node.add_discovery_relay(address).unwrap();
    let after = node.discovery_relays().unwrap();
    assert_eq!(after.len(), before.len() + 1);
    assert!(after.contains(&address));
    // Supplemental bounds, duplicate, invalid-address and shutdown controls.
    node.add_discovery_relay(address).unwrap();
    let second = Ipv4Addr::new(127, 0, 0, 124);
    node.add_discovery_relay(second).unwrap();
    assert_eq!(
        node.add_discovery_relay(Ipv4Addr::new(127, 0, 0, 125)),
        Err(Error::QueueFull)
    );
    assert_eq!(node.discovery_relays().unwrap(), vec![address, second]);
    for invalid in [
        Ipv4Addr::UNSPECIFIED,
        Ipv4Addr::BROADCAST,
        Ipv4Addr::new(224, 0, 0, 1),
    ] {
        assert_eq!(node.add_discovery_relay(invalid), Err(Error::InvalidConfig));
    }
    assert_eq!(node.statistics().queue_drops, 1);
    node.cancel();
    assert_eq!(node.discovery_relays(), Err(Error::Shutdown));
    assert_eq!(node.add_discovery_relay(address), Err(Error::Shutdown));
}

#[test]
fn rust_relay_uses_both_channels_and_retains_unknown_packet_bytes() {
    for statistics_wire in [false, true] {
        rust_relay_packets(statistics_wire);
    }
}

fn rust_relay_packets(statistics_wire: bool) {
    let (mut config, _ports) = config();
    config.statistics_wire = statistics_wire;
    let node = Node::new(config.clone()).unwrap();
    for channel in 0..2 {
        let multicast = observer(&config, channel, true);
        let unicast = observer(&config, channel, false);
        let mut original = packet(&config, channel, true);
        let mut body = original.encode_to_vec();
        // Noncanonical unknown varint and known version tag with wrong wire.
        let unknown = [0xa0, 6, 0x81, 0, 0x12, 1, 255];
        body.extend(unknown);
        original.flags = None;
        let mut immutable = original.encode_to_vec();
        immutable.extend(unknown);
        send(&config, channel, &body, false);
        let (copy, received) = receive(&multicast, "relay-observed-process", false);
        assert!(copy.flags.unwrap().no_relay);
        assert_eq!(&received[..immutable.len()], immutable);
        assert_eq!(copy.disc_contents, original.disc_contents);
        let deadline = Instant::now() + Duration::from_secs(2);
        let expected = format!("/relay-{channel}");
        loop {
            let names = if channel == 0 {
                node.known_topics().unwrap()
            } else {
                node.known_services().unwrap()
            };
            if names.contains(&expected) {
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(
            node.discovery_relays().unwrap(),
            vec![Ipv4Addr::new(127, 0, 0, 2)]
        );
        // Only Flags are rewritten; ordinary publisher fields remain exact.
        let mut ordinary_packet = packet(&config, channel, false);
        let ordinary = ordinary_packet.encode_to_vec();
        ordinary_packet.flags = None;
        let immutable = ordinary_packet.encode_to_vec();
        send(&config, channel, &ordinary, true);
        let (_, forwarded) = receive(&unicast, "relay-observed-process", true);
        assert_eq!(&forwarded[..immutable.len()], immutable);
        let (heartbeat, _) = receive_if(&unicast, |message| {
            message.r#type == i32::from(Type::Heartbeat)
                && message.flags.as_ref().is_some_and(|flags| flags.relay)
        });
        assert!(!heartbeat.process_uuid.is_empty());
        assert_ne!(heartbeat.process_uuid, "relay-observed-process");
    }
    assert_eq!(node.statistics().malformed_frames, 0);
}

struct Peer {
    child: Child,
    input: ChildStdin,
    output: Receiver<String>,
}
impl Peer {
    fn start(config: &Config, relay: Option<&str>) -> Self {
        let path = std::env::var_os("GZ_DISCOVERY_RELAY_PEER")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("artifacts/discovery-relay-peer")
            });
        let mut child = Command::new(path)
            .args([
                "network",
                &config.multicast.to_string(),
                &config.message_port.to_string(),
                &config.service_port.to_string(),
            ])
            .env("GZ_IP", config.interface.to_string())
            .env(
                "GZ_TRANSPORT_TOPIC_STATISTICS",
                if config.statistics_wire { "1" } else { "0" },
            )
            .env("GZ_RELAY", relay.unwrap_or(""))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                if tx.send(line.unwrap()).is_err() {
                    break;
                }
            }
        });
        let peer = Self {
            child,
            input,
            output: rx,
        };
        assert_eq!(peer.line(), "READY");
        peer
    }
    fn line(&self) -> String {
        self.output.recv_timeout(Duration::from_secs(3)).unwrap()
    }
    fn command(&mut self, command: &str) {
        writeln!(self.input, "{command}").unwrap();
        self.input.flush().unwrap();
    }
    fn finish(&mut self) {
        self.command("QUIT");
        assert!(self.child.wait().unwrap().success());
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn native_discovery_forwards_learns_and_replies_on_both_channels() {
    for statistics_wire in [false, true] {
        native_discovery_packets(statistics_wire);
    }
}

fn native_discovery_packets(statistics_wire: bool) {
    let (mut config, _ports) = config();
    config.statistics_wire = statistics_wire;
    let mut peer = Peer::start(&config, None);
    for channel in 0..2 {
        let multicast = observer(&config, channel, true);
        let unicast = observer(&config, channel, false);
        let body = packet(&config, channel, true).encode_to_vec();
        send(&config, channel, &body, false);
        let (copy, _) = receive(&multicast, "relay-observed-process", false);
        assert!(copy.flags.unwrap().no_relay);
        let ordinary = packet(&config, channel, false).encode_to_vec();
        send(&config, channel, &ordinary, true);
        let (copy, _) = receive(&unicast, "relay-observed-process", true);
        assert_eq!(
            copy.disc_contents,
            packet(&config, channel, false).disc_contents
        );
    }
    peer.command("RELAYS");
    assert_eq!(peer.line(), "message 127.0.0.2");
    assert_eq!(peer.line(), "service 127.0.0.2");
    assert_eq!(peer.line(), "END");
    let observers = [observer(&config, 0, false), observer(&config, 1, false)];
    peer.command("DISCOVER");
    assert_eq!(peer.line(), "DISCOVERED");
    for (channel, process) in ["native-message-relay", "native-service-relay"]
        .into_iter()
        .enumerate()
    {
        let (message, _) = receive(&observers[channel], process, true);
        assert_eq!(message.r#type, i32::from(Type::Subscribe));
    }
    peer.finish();
}

#[test]
fn actual_cpp_relays_are_reinjected_and_learned_by_the_rust_worker() {
    let (config, _ports) = config();
    // Native binds first, then Rust: unicast to the wildcard shared port reaches
    // the last binder on this Linux fixture. Multicast reaches both processes.
    // This is packet interoperability, not a two-host topology simulation.
    let mut peer = Peer::start(&config, Some("127.0.0.1"));
    let node = Node::new(config.clone()).unwrap();
    let observers = [observer(&config, 0, true), observer(&config, 1, true)];
    peer.command("DISCOVER");
    assert_eq!(peer.line(), "DISCOVERED");
    for (channel, process) in ["native-message-relay", "native-service-relay"]
        .into_iter()
        .enumerate()
    {
        let (message, _) = receive_if(&observers[channel], |message| {
            message.process_uuid == process
                && message
                    .flags
                    .as_ref()
                    .is_some_and(|flags| !flags.relay && flags.no_relay)
        });
        assert_eq!(message.r#type, i32::from(Type::Subscribe));
        let DiscContents::Sub(subscriber) = message.disc_contents.unwrap() else {
            panic!("native subscriber body missing")
        };
        assert_eq!(
            subscriber.topic,
            gz_transport::qualified_name(
                "relay",
                "",
                if channel == 0 { "/topic" } else { "/service" }
            )
            .unwrap()
        );
    }
    assert_eq!(node.discovery_relays().unwrap(), vec![Ipv4Addr::LOCALHOST]);
    assert_eq!(node.statistics().malformed_frames, 0);
    peer.finish();
}
