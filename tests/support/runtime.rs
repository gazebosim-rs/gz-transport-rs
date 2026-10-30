// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// TopicUtils assertion tables: Copyright (C) 2014 Open Source Robotics Foundation.
use super::*;

fn relay_state(capacity: usize) -> Worker {
    let (mut worker, _) = service_state(Config {
        capacity,
        message_port: 0,
        service_port: 0,
        partition: Uuid::new_v4().to_string(),
        ..Default::default()
    });
    worker.config.message_port = worker.udp[0].local_addr().unwrap().port();
    worker.config.service_port = worker.udp[1].local_addr().unwrap().port();
    worker
}

fn relay_observer(worker: &Worker, channel: usize, multicast: bool) -> UdpSocket {
    let address = if multicast {
        worker.config.multicast
    } else {
        Ipv4Addr::new(127, 0, 0, 2)
    };
    let socket = Socket::new(Domain::IPV4, SocketType::DGRAM, Some(Protocol::UDP)).unwrap();
    socket.set_reuse_address(true).unwrap();
    socket
        .bind(&SocketAddrV4::new(address, worker.udp[channel].local_addr().unwrap().port()).into())
        .unwrap();
    if multicast {
        socket
            .join_multicast_v4(&address, &worker.config.interface)
            .unwrap();
    }
    socket
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    socket.into()
}

fn relay_packet(worker: &Worker, channel: usize) -> Vec<u8> {
    let mut publisher = worker.wire_publisher(
        qualified_name(&worker.config.partition, &worker.config.namespace, "/relay").unwrap(),
        if channel == 0 {
            PubType::MsgPub(MessagePublisher {
                msg_type: "type".into(),
                ..Default::default()
            })
        } else {
            PubType::SrvPub(ServicePublisher {
                request_type: "type".into(),
                response_type: "type".into(),
                ..Default::default()
            })
        },
    );
    publisher.process_uuid = "remote-relay".into();
    publisher.node_uuid = "remote-node".into();
    protocol::encode(&protocol::advertisement("remote-relay", publisher)).unwrap()
}

#[test]
fn discovery_relay_forwards_both_channels_and_dispatches_only_no_relay_copy() {
    let mut worker = relay_state(2);
    let source = Ipv4Addr::new(127, 0, 0, 2);
    for channel in 0..2 {
        let multicast = relay_observer(&worker, channel, true);
        let unicast = relay_observer(&worker, channel, false);
        let frame = relay_packet(&worker, channel);
        let relayed = if channel == 0 {
            protocol::relay_frame(&frame, true).unwrap()
        } else {
            let mut both_flags = protocol::decode(&frame).unwrap();
            both_flags.flags = Some(gz_msgs::discovery::discovery::Flags {
                relay: true,
                no_relay: true,
            });
            protocol::encode(&both_flags).unwrap()
        };
        worker.receive_discovery(channel, &relayed, IpAddr::V4(source));
        assert_eq!(worker.peers.len(), channel);
        assert!(worker.discovery_relays[channel].contains(&source));
        let mut buffer = [0; 4096];
        let count = multicast.recv(&mut buffer).unwrap();
        assert_eq!(
            &buffer[..count],
            protocol::relay_frame(&relayed, false).unwrap()
        );
        worker.receive_discovery(channel, &buffer[..count], IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(worker.peers.len(), channel + 1);
        assert!(unicast.recv(&mut buffer).is_err());
        worker.receive_discovery(channel, &frame, IpAddr::V4(source));
        let count = unicast.recv(&mut buffer).unwrap();
        assert_eq!(
            &buffer[..count],
            protocol::relay_frame(&frame, true).unwrap()
        );
        assert_eq!(worker.peers.len(), channel + 1);
        worker.announce(
            channel,
            protocol::discovery(&worker.process, Type::Heartbeat, None),
        );
        let count = unicast.recv(&mut buffer).unwrap();
        let message = protocol::decode(&buffer[..count]).unwrap();
        assert_eq!(message.process_uuid, worker.process);
        assert!(message.flags.unwrap().relay);
    }
    assert_eq!(worker.sender.counters.snapshot(), Statistics::default());
}

#[test]
fn discovery_relay_bounds_learning_and_atomic_explicit_admission() {
    let mut worker = relay_state(1);
    let first = Ipv4Addr::new(127, 0, 0, 2);
    let other = Ipv4Addr::new(127, 0, 0, 3);
    let frame = protocol::relay_frame(
        &protocol::encode(&protocol::discovery("foreign", Type::Heartbeat, None)).unwrap(),
        true,
    )
    .unwrap();
    worker.receive_discovery(0, &frame, IpAddr::V4(first));
    worker.receive_discovery(0, &frame, IpAddr::V4(first));
    worker.receive_discovery(0, &frame, IpAddr::V4(other));
    assert_eq!(worker.discovery_relays[0], vec![first]);
    assert!(worker.discovery_relays[1].is_empty());
    let (tx, rx) = mpsc::sync_channel(1);
    worker.command(Command::AddDiscoveryRelay(other, tx));
    assert_eq!(rx.recv().unwrap(), Err(Error::QueueFull));
    assert!(worker.discovery_relays[1].is_empty());
    let (tx, rx) = mpsc::sync_channel(1);
    worker.command(Command::AddDiscoveryRelay(first, tx));
    assert_eq!(rx.recv().unwrap(), Ok(()));
    assert_eq!(worker.discovery_relays[1], vec![first]);
    let (tx, rx) = mpsc::sync_channel(1);
    worker.command(Command::DiscoveryRelays(tx));
    assert_eq!(rx.recv().unwrap(), vec![first]);
    // Invalid and own-process input must neither learn a source nor emit a copy.
    let own = protocol::relay_frame(
        &protocol::encode(&protocol::discovery(&worker.process, Type::Heartbeat, None)).unwrap(),
        true,
    )
    .unwrap();
    worker.receive_discovery(1, &own, IpAddr::V4(other));
    worker.receive_discovery(1, &[255], IpAddr::V4(other));
    let mut wrong = protocol::discovery("foreign", Type::Heartbeat, None);
    wrong.version = 110;
    worker.receive_discovery(1, &protocol::encode(&wrong).unwrap(), IpAddr::V4(other));
    assert_eq!(worker.discovery_relays[1], vec![first]);
    let counters = worker.sender.counters.snapshot();
    assert_eq!(counters.queue_drops, 2);
    assert_eq!(counters.malformed_frames, 2);
    assert_eq!(counters.send_failures, 0);
}

#[test]
fn discovery_relay_send_failures_keep_local_dispatch_and_bound_final_wire_size() {
    let mut worker = relay_state(3);
    let observer = relay_observer(&worker, 0, false);
    // With SO_BROADCAST disabled, this first destination deterministically fails.
    // Native stops the unicast loop; the later valid peer receives no packet.
    worker.discovery_relays[0] = vec![Ipv4Addr::BROADCAST, Ipv4Addr::new(127, 0, 0, 2)];
    worker.receive_discovery(
        0,
        &relay_packet(&worker, 0),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
    );
    assert_eq!(worker.peers.len(), 1);
    assert_eq!(worker.sender.counters.snapshot().send_failures, 1);
    let mut buffer = [0; 4096];
    assert!(observer.recv(&mut buffer).is_err());
    let mut large = protocol::discovery("x", Type::Heartbeat, None);
    large.flags = Some(gz_msgs::discovery::discovery::Flags {
        relay: true,
        no_relay: false,
    });
    large.process_uuid = "x".repeat(protocol::MAX_DATAGRAM - 32);
    let available = protocol::MAX_DATAGRAM - protocol::encode(&large).unwrap().len();
    large.process_uuid.push_str(&"x".repeat(available));
    let frame = protocol::encode(&large).unwrap();
    assert_eq!(frame.len(), protocol::MAX_DATAGRAM);
    let source = Ipv4Addr::new(127, 0, 0, 3);
    let multicast = relay_observer(&worker, 0, true);
    worker.receive_discovery(0, &frame, IpAddr::V4(source));
    let mut received = vec![0; protocol::MAX_DATAGRAM];
    assert_eq!(
        multicast.recv(&mut received).unwrap(),
        protocol::MAX_DATAGRAM
    );
    assert!(!protocol::decode(&received).unwrap().flags.unwrap().relay);
    assert!(worker.discovery_relays[0].contains(&source));
    assert_eq!(worker.sender.counters.snapshot().send_failures, 1);
    large.flags = None;
    let available = protocol::MAX_DATAGRAM - protocol::encode(&large).unwrap().len();
    large.process_uuid.push_str(&"x".repeat(available));
    worker.relay_unicast(0, &protocol::encode(&large).unwrap());
    let counters = worker.sender.counters.snapshot();
    assert_eq!(counters.send_failures, 2);
    assert_eq!(counters.queue_drops, 0);
    assert_eq!(counters.malformed_frames, 0);
}

#[test]
fn discovery_relay_api_uses_existing_command_deadlines_without_revoking_admission() {
    let (mut worker, commands) = service_state(Config {
        capacity: 2,
        message_port: 0,
        service_port: 0,
        reply_timeout: Duration::from_millis(10),
        ..Default::default()
    });
    let node = service_registry_node(&worker);
    let address = Ipv4Addr::new(127, 0, 0, 2);
    assert_eq!(node.add_discovery_relay(address), Err(Error::Timeout));
    assert_eq!(node.discovery_relays(), Err(Error::Timeout));
    assert_eq!(node.discovery_relays(), Err(Error::QueueFull));
    assert!(worker.discovery_relays.iter().all(Vec::is_empty));
    // Timeout closes only the reply receiver. The admitted mutation can still
    // execute later, and a disconnected reply cannot stop worker progress.
    worker.command(commands.try_recv().unwrap());
    worker.command(commands.try_recv().unwrap());
    assert_eq!(worker.discovery_relays, [vec![address], vec![address]]);
    assert_eq!(worker.sender.counters.snapshot().queue_drops, 1);
}

#[test]
fn publication_rate_boundaries_keep_epoch_fractional_and_sub_nanosecond_periods() {
    let epoch = Duration::ZERO;
    assert!(PublicationThrottle::new(u64::MAX).is_none());
    let zero = PublicationThrottle::new(0).unwrap();
    assert!(!zero.ready_at(epoch, epoch));
    assert!(!zero.ready_at(epoch, Duration::from_secs(1_000_000)));
    let one = PublicationThrottle::new(1).unwrap();
    assert!(!one.ready_at(epoch, Duration::from_nanos(999_999_999)));
    assert!(one.ready_at(epoch, Duration::from_secs(1)));
    let fractional = PublicationThrottle::new(3).unwrap();
    assert!(!fractional.ready_at(epoch, Duration::from_nanos(333_333_333)));
    assert!(fractional.ready_at(epoch, Duration::from_nanos(333_333_334)));
    let last = Duration::from_secs(2);
    assert!(!fractional.ready_at(last, last));
    assert!(!fractional.ready_at(last, last + Duration::from_nanos(333_333_333)));
    assert!(fractional.ready_at(last, last + Duration::from_nanos(333_333_334)));
    let exact = PublicationThrottle::new(10).unwrap();
    assert!(!exact.ready_at(last, last + Duration::from_nanos(99_999_999)));
    assert!(exact.ready_at(last, last + Duration::from_millis(100)));
    for rate in [1_000_000_000, 1_000_000_001, u64::MAX - 1] {
        let fast = PublicationThrottle::new(rate).unwrap();
        assert!(!fast.ready_at(last, last));
        assert!(fast.ready_at(last, last + Duration::from_nanos(1)));
    }
    assert!(!fractional.ready_at(last, Duration::from_secs(1)));
}

#[test]
fn publication_rate_queries_do_not_consume_intervals_and_poison_recovers() {
    let throttle = PublicationThrottle::new(1).unwrap();
    assert!(throttle.ready().unwrap());
    assert!(throttle.ready().unwrap());
    assert!(throttle.admit().unwrap());
    assert!(!throttle.ready().unwrap());
    assert!(!throttle.admit().unwrap());
    thread::scope(|scope| {
        assert!(
            scope
                .spawn(|| {
                    let _guard = throttle.last.lock().unwrap();
                    panic!("exercise rate-lock recovery");
                })
                .join()
                .is_err()
        );
    });
    assert!(!throttle.ready().unwrap());
    assert!(!throttle.admit().unwrap());
    *throttle.last.lock().unwrap_err().into_inner() = Duration::ZERO;
    assert!(throttle.ready().unwrap());
    assert!(throttle.admit().unwrap());
}

#[cfg(unix)]
#[test]
fn publication_clock_has_native_epoch_and_explicit_errors() {
    let first = publication_time().unwrap();
    let second = publication_clock(nix::time::ClockId::CLOCK_MONOTONIC).unwrap();
    assert!(second >= first);
    assert!(matches!(
        publication_clock(nix::time::ClockId::from_raw(-1)),
        Err(Error::Native(_))
    ));
    let peer = std::env::var_os("GZ_CPP_PEER")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("artifacts/cpp-peer")
        });
    let before = publication_time().unwrap().as_nanos();
    let native = std::process::Command::new(peer)
        .args(["publication-clock", "unused", "unused"])
        .output()
        .unwrap();
    let after = publication_time().unwrap().as_nanos();
    assert!(native.status.success(), "{:?}", native);
    let clocks: Vec<u128> = std::str::from_utf8(&native.stdout)
        .unwrap()
        .split_whitespace()
        .map(|clock| clock.parse().unwrap())
        .collect();
    assert_eq!(clocks.len(), 3);
    assert!(
        before <= clocks[0]
            && clocks[0] <= clocks[1]
            && clocks[1] <= clocks[2]
            && clocks[2] <= after
    );
}

#[test]
fn actual_publication_advertisements_carry_rate_options() {
    let (mut worker, _) = state(4);
    let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    worker.config.multicast = Ipv4Addr::LOCALHOST;
    worker.config.message_port = receiver.local_addr().unwrap().port();
    let mut buffer = vec![0; protocol::MAX_DATAGRAM];
    for rate in [u64::MAX, 0, 3, u64::MAX - 1] {
        let mut options = AdvertiseOptions::default();
        options.set_msgs_per_sec(rate);
        worker.command(Command::Advertise(
            format!("/rate-{rate}"),
            "gz.msgs.Int32".into(),
            options,
            Arc::new(AtomicBool::new(true)),
            Arc::new(AtomicU8::new(0)),
        ));
        let size = receiver.recv(&mut buffer).unwrap();
        let advertisement = protocol::decode(&buffer[..size]).unwrap();
        assert_eq!(advertisement.r#type, Type::Advertise as i32);
        let Some(DiscContents::Pub(publisher)) = advertisement.disc_contents else {
            panic!("missing publisher");
        };
        assert_eq!(publisher.scope, Scope::All as i32);
        assert_eq!(publisher.topic, format!("/rate-{rate}"));
        let Some(PubType::MsgPub(message)) = publisher.pub_type else {
            panic!("missing message rate");
        };
        assert_eq!(message.msg_type, "gz.msgs.Int32");
        assert_eq!(message.msgs_per_sec, rate);
        assert_eq!(message.throttled, rate != u64::MAX);
    }
}
trait TestDiscovery {
    fn discovered(&mut self, channel: usize, message: Discovery);
}

#[test]
fn known_lists_filter_partition_kind_age_and_malformed_paths() {
    let (mut worker, _) = state(16);
    let partition = worker.config.partition.clone();
    worker.config.partition = format!("/{partition}/");
    let topic = |partition: &str, path: &str| qualified_name(partition, "", path).unwrap();
    let message = || PubType::MsgPub(MessagePublisher::default());
    let service = || {
        PubType::SrvPub(ServicePublisher {
            socket_id: "socket".into(),
            request_type: "request".into(),
            response_type: "response".into(),
        })
    };
    let local_topic = worker.wire_publisher(topic(&partition, "/local"), message());
    let local_service = worker.wire_publisher(topic(&partition, "/local-service"), service());
    worker.advertisements.push((
        local_topic,
        Arc::new(AtomicBool::new(true)),
        Arc::new(AtomicU8::new(0)),
    ));
    worker.services.push(ServiceState {
        publisher: local_service,
        tx: Arc::new(mpsc::sync_channel(1).0),
        active: Arc::new(AtomicBool::new(true)),
    });
    {
        let mut serial = 0;
        let mut insert = |channel: usize, name: String, kind: PubType, stale: bool| {
            serial += 1;
            let mut publisher = worker.wire_publisher(name, kind);
            publisher.process_uuid = format!("remote-{serial}");
            publisher.node_uuid = format!("node-{serial}");
            let key = PeerKey {
                channel,
                process: publisher.process_uuid.clone(),
                node: publisher.node_uuid.clone(),
                topic: publisher.topic.clone(),
                address: publisher.address.clone(),
            };
            let stamp = Instant::now() - Duration::from_secs(if stale { 4 } else { 0 });
            worker.peers.insert(key, (publisher, stamp));
        };
        insert(0, topic(&partition, "/remote"), message(), false);
        insert(0, topic(&partition, "/local"), message(), false);
        insert(1, topic(&partition, "/remote-service"), service(), false);
        insert(0, topic("other", "/foreign"), message(), false);
        insert(1, topic("other", "/foreign-service"), service(), false);
        insert(0, format!("@/{partition}@missing-slash"), message(), false);
        insert(0, topic(&partition, "/stale"), message(), true);
        insert(1, topic(&partition, "/wrong-kind"), message(), false);
        insert(0, topic(&partition, "/wrong-kind"), service(), false);
    }
    assert_eq!(worker.known_names(0), ["/local", "/remote"]);
    assert_eq!(worker.known_names(1), ["/local-service", "/remote-service"]);
    worker.config.partition.clear();
    let empty_partition = worker.wire_publisher(topic("", "/empty"), message());
    worker.advertisements.push((
        empty_partition,
        Arc::new(AtomicBool::new(true)),
        Arc::new(AtomicU8::new(0)),
    ));
    assert_eq!(worker.known_names(0), ["/empty"]);
    assert!(worker.known_names(1).is_empty());
}

#[test]
fn subscriber_interest_replies_refresh_and_retire_by_identity() {
    let (mut worker, _) = state(1);
    let name = qualified_name(&worker.config.partition, "", "/remote-only").unwrap();
    let reply = |worker: &Worker, topic: &str, process: &str, node: &str| {
        let mut publisher = worker.subscriber_publisher(topic, "gz.msgs.StringMsg");
        publisher.process_uuid = process.into();
        publisher.node_uuid = node.into();
        protocol::discovery(
            process,
            Type::SubscribersRep,
            Some(DiscContents::Pub(publisher)),
        )
    };
    let first = reply(&worker, &name, "remote", "node");
    worker.discovered(0, first.clone());
    assert_eq!(worker.known_topic_names(), ["/remote-only"]);
    let key = SubscriberInterestKey {
        process: "remote".into(),
        node: "node".into(),
        topic: name.clone(),
        address: worker.address.clone(),
        type_name: "gz.msgs.StringMsg".into(),
    };
    *worker.subscriber_interests.get_mut(&key).unwrap() = Instant::now() - Duration::from_secs(2);
    worker.discovered(0, first);
    assert_eq!(worker.subscriber_interests.len(), 1);
    assert!(worker.subscriber_interests[&key].elapsed() < Duration::from_secs(1));

    let other = qualified_name(&worker.config.partition, "", "/other").unwrap();
    worker.discovered(0, reply(&worker, &other, "second", "node"));
    assert_eq!(worker.known_topic_names(), ["/remote-only"]);
    assert_eq!(worker.sender.counters.snapshot().queue_drops, 1);
    worker.discovered(0, reply(&worker, &other, "remote", "wrong-node"));
    assert_eq!(worker.subscriber_interests.len(), 1);

    let mut wrong_kind = worker.wire_publisher(
        other.clone(),
        PubType::SrvPub(ServicePublisher {
            socket_id: "socket".into(),
            request_type: "request".into(),
            response_type: "response".into(),
        }),
    );
    wrong_kind.process_uuid = "wrong-kind".into();
    worker.discovered(
        0,
        protocol::discovery(
            "wrong-kind",
            Type::SubscribersRep,
            Some(DiscContents::Pub(wrong_kind)),
        ),
    );
    worker.discovered(0, reply(&worker, "@/foreign@/other", "foreign", "node"));
    assert_eq!(worker.subscriber_interests.len(), 1);

    let mut wrong_end = worker.subscriber_publisher(&name, "gz.msgs.StringMsg");
    wrong_end.process_uuid = "remote".into();
    wrong_end.node_uuid = "wrong-node".into();
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::EndConnection,
            Some(DiscContents::Pub(wrong_end.clone())),
        ),
    );
    assert_eq!(worker.subscriber_interests.len(), 1);
    wrong_end.node_uuid = "node".into();
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::EndConnection,
            Some(DiscContents::Pub(wrong_end)),
        ),
    );
    assert!(worker.subscriber_interests.is_empty());

    worker.discovered(0, reply(&worker, &name, "remote", "node"));
    *worker.subscriber_interests.get_mut(&key).unwrap() = Instant::now() - Duration::from_secs(4);
    assert!(worker.known_topic_names().is_empty());
    worker.discovered(0, reply(&worker, &other, "second", "node"));
    assert_eq!(worker.known_topic_names(), ["/other"]);
    assert_eq!(worker.subscriber_interests.len(), 1);
    worker.discovered(0, protocol::discovery("second", Type::Bye, None));
    assert!(worker.known_topic_names().is_empty());
}

#[test]
fn topic_info_remote_types_keep_endpoint_identity_and_end_all_variants() {
    let (mut worker, _) = state(5);
    let name = qualified_name(&worker.config.partition, "", "/types").unwrap();
    let reply = |worker: &Worker, node: &str, type_name: &str| {
        let mut publisher = worker.subscriber_publisher(&name, type_name);
        publisher.process_uuid = "remote".into();
        publisher.node_uuid = node.into();
        protocol::discovery(
            "remote",
            Type::SubscribersRep,
            Some(DiscContents::Pub(publisher)),
        )
    };
    worker.discovered(0, reply(&worker, "first", "gz.msgs.Int32"));
    worker.discovered(0, reply(&worker, "first", "gz.msgs.StringMsg"));
    worker.discovered(0, reply(&worker, "second", "gz.msgs.Int32"));
    assert_eq!(
        worker.topic_endpoint_types(&name).subscribers,
        ["gz.msgs.Int32", "gz.msgs.Int32", "gz.msgs.StringMsg"]
    );
    let first = SubscriberInterestKey {
        process: "remote".into(),
        node: "first".into(),
        topic: name.clone(),
        address: worker.address.clone(),
        type_name: "gz.msgs.Int32".into(),
    };
    *worker.subscriber_interests.get_mut(&first).unwrap() = Instant::now() - Duration::from_secs(4);
    assert_eq!(
        worker.topic_endpoint_types(&name).subscribers,
        ["gz.msgs.Int32", "gz.msgs.StringMsg"]
    );
    let mut end = worker.subscriber_publisher(&name, "");
    end.process_uuid = "remote".into();
    end.node_uuid = "first".into();
    worker.discovered(
        0,
        protocol::discovery("remote", Type::EndConnection, Some(DiscContents::Pub(end))),
    );
    assert_eq!(
        worker.topic_endpoint_types(&name).subscribers,
        ["gz.msgs.Int32"]
    );
    assert_eq!(worker.subscriber_interests.len(), 1);
}

#[test]
fn publisher_recreation_withdraws_old_before_new_advertisement() {
    let (mut worker, _) = state(3);
    let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_millis(30)))
        .unwrap();
    worker.config.multicast = Ipv4Addr::LOCALHOST;
    worker.config.message_port = receiver.local_addr().unwrap().port();
    let old = Arc::new(AtomicBool::new(true));
    worker.command(Command::Advertise(
        "/topic".into(),
        "old".into(),
        AdvertiseOptions::default(),
        old.clone(),
        Arc::new(AtomicU8::new(0)),
    ));
    old.store(false, Ordering::Release);
    let retired = Arc::new(AtomicBool::new(false));
    worker.command(Command::Advertise(
        "/topic".into(),
        "never-advertised".into(),
        AdvertiseOptions::default(),
        retired,
        Arc::new(AtomicU8::new(0)),
    ));
    worker.command(Command::Advertise(
        "/topic".into(),
        "new".into(),
        AdvertiseOptions::default(),
        Arc::new(AtomicBool::new(true)),
        Arc::new(AtomicU8::new(0)),
    ));
    worker.cleanup();
    let mut trace = Vec::new();
    let mut buffer = vec![0; protocol::MAX_DATAGRAM];
    while let Ok(len) = receiver.recv(&mut buffer) {
        let message = protocol::decode(&buffer[..len]).unwrap();
        let Some(DiscContents::Pub(p)) = message.disc_contents else {
            panic!("missing publisher")
        };
        let Some(PubType::MsgPub(info)) = p.pub_type else {
            panic!("wrong publisher kind")
        };
        trace.push((message.r#type, info.msg_type));
    }
    assert_eq!(
        trace,
        [
            (Type::Advertise as i32, "old".into()),
            (Type::Unadvertise as i32, "old".into()),
            (Type::Advertise as i32, "new".into())
        ]
    );
    assert_eq!(worker.advertisements.len(), 1);
}

#[test]
fn publisher_ownership_is_concurrent_and_failed_admission_releases_it() {
    let (worker, commands) = state(2);
    let node = Node {
        sender: worker.sender.clone(),
        lifetime: Arc::new(WorkerLifetime {
            stop: worker.sender.stop.clone(),
            worker: Mutex::new(None),
        }),
        owner_dead: worker.owner_dead.clone(),
        local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
        config: worker.config.clone(),
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: worker.sender.identity.get().unwrap().services.clone(),
        subscription_dispatcher: Mutex::new(Weak::new()),
    };
    let results = thread::scope(|scope| {
        let first = scope.spawn(|| node.advertise("/topic", "one"));
        let second = scope.spawn(|| node.advertise("topic", "two"));
        [first.join().unwrap(), second.join().unwrap()]
    });
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Err(Error::AlreadyAdvertised)))
            .count(),
        1
    );
    drop(results);
    drop(commands.recv().unwrap());
    let first = node.advertise("/first", "one").unwrap();
    let second = node.advertise("/second", "two").unwrap();
    drop(first);
    assert!(matches!(
        node.advertise("/first", "three"),
        Err(Error::QueueFull)
    ));
    drop(commands.recv().unwrap());
    let replacement = node.advertise("/first", "three").unwrap();
    assert!(matches!(
        node.advertise("/first", "three"),
        Err(Error::AlreadyAdvertised)
    ));
    drop((replacement, second));
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert!(node.topics.lock().unwrap().len() <= node.config.capacity);
}

#[test]
fn publication_scope_initial_response_and_withdrawal_follow_native() {
    for scope in [Scope::Process, Scope::Host, Scope::All] {
        let (mut worker, _) = state(8);
        let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        receiver
            .set_read_timeout(Some(Duration::from_millis(30)))
            .unwrap();
        worker.config.multicast = Ipv4Addr::LOCALHOST;
        worker.config.message_port = receiver.local_addr().unwrap().port();
        let active = Arc::new(AtomicBool::new(true));
        worker.command(Command::Advertise(
            "/scope".into(),
            "type".into(),
            AdvertiseOptions {
                scope,
                ..Default::default()
            },
            active.clone(),
            Arc::new(AtomicU8::new(0)),
        ));
        let read = || {
            let mut buffer = vec![0; protocol::MAX_DATAGRAM];
            let mut packets = Vec::new();
            while let Ok(len) = receiver.recv(&mut buffer) {
                let message = protocol::decode(&buffer[..len]).unwrap();
                let Some(DiscContents::Pub(publisher)) = &message.disc_contents else {
                    panic!("missing scoped publisher")
                };
                assert_eq!(publisher.scope, i32::from(scope));
                packets.push(message.r#type);
            }
            packets
        };
        assert_eq!(
            read(),
            if scope == Scope::Process {
                vec![]
            } else {
                vec![Type::Advertise as i32]
            }
        );
        for (source, local) in [
            (Ipv4Addr::new(192, 0, 2, 1), false),
            (Ipv4Addr::LOCALHOST, true),
        ] {
            worker.discovery(
                0,
                protocol::discovery(
                    "remote",
                    Type::Subscribe,
                    Some(DiscContents::Sub(Subscriber {
                        topic: "/scope".into(),
                    })),
                ),
                IpAddr::V4(source),
            );
            assert_eq!(
                read(),
                if scope == Scope::All || scope == Scope::Host && local {
                    vec![Type::Advertise as i32]
                } else {
                    vec![]
                }
            );
        }
        active.store(false, Ordering::Release);
        worker.cleanup();
        assert!(worker.advertisements.is_empty());
        assert_eq!(
            read(),
            if scope == Scope::Process {
                vec![]
            } else {
                vec![Type::Unadvertise as i32]
            }
        );
    }
}

#[test]
fn service_withdrawal_releases_capacity_once_and_rolls_back_failed_admission() {
    let (mut worker, commands) = state(1);
    let node = Node {
        sender: worker.sender.clone(),
        lifetime: Arc::new(WorkerLifetime {
            stop: worker.sender.stop.clone(),
            worker: Mutex::new(None),
        }),
        owner_dead: worker.owner_dead.clone(),
        local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
        config: worker.config.clone(),
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: worker.sender.identity.get().unwrap().services.clone(),
        subscription_dispatcher: Mutex::new(Weak::new()),
    };
    assert!(node.advertised_services().unwrap().is_empty());
    assert_eq!(
        node.unadvertise_service("invalid service"),
        Err(Error::InvalidName)
    );
    node.unadvertise_service("/absent").unwrap();
    let old = node.serve("/old", "req", "rep").unwrap();
    assert_eq!(node.advertised_services().unwrap(), ["/old"]);
    node.unadvertise_service("old").unwrap();
    node.unadvertise_service("old").unwrap();
    assert!(node.advertised_services().unwrap().is_empty());
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert!(matches!(
        node.serve("/new", "req", "rep"),
        Err(Error::QueueFull)
    ));
    assert!(node.advertised_services().unwrap().is_empty());
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    worker.command(commands.recv().unwrap()); // Retired before registration: no advertisement.
    assert!(worker.services.is_empty());
    assert!(matches!(
        old.recv_timeout(Duration::ZERO),
        Err(Error::Shutdown)
    ));
    let new = node.serve("/new", "req", "rep").unwrap();
    drop(old);
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 1);
    drop(new);
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert!(node.advertised_services().unwrap().is_empty());
    node.cancel();
    assert_eq!(node.unadvertise_service("/new"), Err(Error::Shutdown));
    assert_eq!(node.advertised_services(), Err(Error::Shutdown));
}

#[test]
fn concurrent_service_duplicates_admit_one_receiver_and_preserve_the_original() {
    let (mut worker, commands) = state(2);
    let node = Node {
        sender: worker.sender.clone(),
        lifetime: Arc::new(WorkerLifetime {
            stop: worker.sender.stop.clone(),
            worker: Mutex::new(None),
        }),
        owner_dead: worker.owner_dead.clone(),
        local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
        config: worker.config.clone(),
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: worker.sender.identity.get().unwrap().services.clone(),
        subscription_dispatcher: Mutex::new(Weak::new()),
    };
    let results = thread::scope(|scope| {
        let first = scope.spawn(|| node.serve("/topic", "req", "rep"));
        let second = scope.spawn(|| node.serve("topic", "req", "other"));
        [first.join().unwrap(), second.join().unwrap()]
    });
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Err(Error::AlreadyAdvertised)))
            .count(),
        1
    );
    let service = results.into_iter().find_map(Result::ok).unwrap();
    worker.command(commands.recv().unwrap());
    assert!(commands.try_recv().is_err());
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 1);
    assert_eq!(worker.services.len(), 1);
    let Some(PubType::SrvPub(types)) = &worker.services[0].publisher.pub_type else {
        panic!("wrong kind")
    };
    let selected = types.response_type.clone();
    let (mut request, result) = pending(Instant::now() + Duration::from_secs(1));
    request.topic = node.topic("/topic").unwrap();
    request.response_type = selected;
    worker.command(Command::Request(request));
    let received = service.recv_timeout(Duration::ZERO).unwrap();
    assert_eq!(received.data, [1]);
    received.respond(vec![9], true).unwrap();
    let response = worker.response_rx.try_recv().unwrap();
    worker.command(response);
    assert_eq!(result.recv().unwrap(), Ok((vec![9], true)));
    assert!(matches!(
        node.serve("/topic", "different", "different"),
        Err(Error::AlreadyAdvertised)
    ));
    drop(service);
    let _replacement = node.serve("/topic", "different", "different").unwrap();
}

#[test]
fn service_recreation_orders_withdrawal_and_retains_already_admitted_requests() {
    let (mut worker, commands) = state(2);
    let node = Node {
        sender: worker.sender.clone(),
        lifetime: Arc::new(WorkerLifetime {
            stop: worker.sender.stop.clone(),
            worker: Mutex::new(None),
        }),
        owner_dead: worker.owner_dead.clone(),
        local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
        config: worker.config.clone(),
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: worker.sender.identity.get().unwrap().services.clone(),
        subscription_dispatcher: Mutex::new(Weak::new()),
    };
    let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_millis(30)))
        .unwrap();
    worker.config.multicast = Ipv4Addr::LOCALHOST;
    worker.config.service_port = receiver.local_addr().unwrap().port();
    let old = node.serve("/topic", "old", EMPTY_TYPE).unwrap();
    worker.command(commands.recv().unwrap());
    let (mut request, result) = pending(Instant::now() + Duration::from_secs(1));
    request.topic = node.topic("/topic").unwrap();
    request.request_type = "old".into();
    request.response_type = EMPTY_TYPE.into();
    worker.command(Command::Request(request));
    assert_eq!(result.recv().unwrap(), Ok((vec![], true)));
    node.unadvertise_service("/topic").unwrap();
    let replacement = node.serve("/topic", "new", EMPTY_TYPE).unwrap();
    worker.command(commands.recv().unwrap());
    worker.cleanup();
    let mut trace = Vec::new();
    let mut buffer = vec![0; protocol::MAX_DATAGRAM];
    while let Ok(len) = receiver.recv(&mut buffer) {
        let message = protocol::decode(&buffer[..len]).unwrap();
        if message.r#type == Type::Subscribe as i32 {
            continue;
        }
        let Some(DiscContents::Pub(p)) = message.disc_contents else {
            panic!("missing publisher")
        };
        let Some(PubType::SrvPub(info)) = p.pub_type else {
            panic!("wrong kind")
        };
        trace.push((message.r#type, info.request_type));
    }
    assert_eq!(
        trace,
        [
            (Type::Advertise as i32, "old".into()),
            (Type::Unadvertise as i32, "old".into()),
            (Type::Advertise as i32, "new".into())
        ]
    );
    old.recv_timeout(Duration::ZERO)
        .unwrap()
        .respond(vec![], true)
        .unwrap();
    assert!(matches!(
        old.recv_timeout(Duration::ZERO),
        Err(Error::Shutdown)
    ));
    drop(old);
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 1);
    drop(replacement);
}

#[test]
fn service_drop_and_explicit_withdrawal_race_without_double_release() {
    let node = Node::new(Config {
        capacity: 1,
        ..Default::default()
    })
    .unwrap();
    let service = node.serve("/race", "req", "rep").unwrap();
    // Observe worker admission before racing retirement, so an old queued
    // command cannot consume the capacity needed by the final recreation.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match node.service_type_signatures("/race") {
            Err(Error::QueueFull) => {
                assert!(
                    Instant::now() < deadline,
                    "service admission did not complete"
                );
                thread::yield_now();
            }
            result => {
                assert_eq!(
                    result.unwrap(),
                    [ServiceTypes {
                        request_type: "req".into(),
                        response_type: "rep".into(),
                    }]
                );
                break;
            }
        }
    }
    let barrier = std::sync::Barrier::new(2);
    thread::scope(|scope| {
        scope.spawn(|| {
            barrier.wait();
            drop(service);
        });
        barrier.wait();
        node.unadvertise_service("/race").unwrap();
    });
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert!(node.advertised_services().unwrap().is_empty());
    let _new = node.serve("/new", "req", "rep").unwrap();
}

#[test]
fn service_retirement_race_before_worker_admission_preserves_queue_bounds() {
    let (mut worker, commands) = state(1);
    let node = Node {
        sender: worker.sender.clone(),
        lifetime: Arc::new(WorkerLifetime {
            stop: worker.sender.stop.clone(),
            worker: Mutex::new(None),
        }),
        owner_dead: worker.owner_dead.clone(),
        local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
        config: worker.config.clone(),
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: worker.sender.identity.get().unwrap().services.clone(),
        subscription_dispatcher: Mutex::new(Weak::new()),
    };
    let service = node.serve("/race", "req", "rep").unwrap();
    let barrier = std::sync::Barrier::new(2);
    thread::scope(|scope| {
        scope.spawn(|| {
            barrier.wait();
            drop(service);
        });
        barrier.wait();
        node.unadvertise_service("/race").unwrap();
    });
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert!(node.advertised_services().unwrap().is_empty());
    assert!(matches!(
        node.serve("/new", "req", "rep"),
        Err(Error::QueueFull)
    ));
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    worker.command(commands.recv().unwrap());
    assert!(
        worker.services.is_empty(),
        "retired queued service must not advertise"
    );
    let new = node.serve("/new", "req", "rep").unwrap();
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 1);
    worker.command(commands.recv().unwrap());
    assert_eq!(worker.services.len(), 1);
    drop(new);
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
}

#[test]
fn poisoned_service_registry_fails_explicitly() {
    let node = Node::new(Config::default()).unwrap();
    thread::scope(|scope| {
        assert!(
            scope
                .spawn(|| {
                    let _guard = node.services.lock().unwrap();
                    panic!("injected registry owner panic");
                })
                .join()
                .is_err()
        );
    });
    assert!(
        matches!(node.serve("/topic", "req", "rep"), Err(Error::Native(message)) if message.contains("poisoned"))
    );
    assert!(
        matches!(node.advertised_services(), Err(Error::Native(message)) if message.contains("poisoned"))
    );
    assert!(
        matches!(node.unadvertise_service("/topic"), Err(Error::Native(message)) if message.contains("poisoned"))
    );
}
impl TestDiscovery for Worker {
    fn discovered(&mut self, channel: usize, message: Discovery) {
        self.discovery(channel, message, self.config.interface.into());
    }
}

fn state(capacity: usize) -> (Worker, Receiver<Command>) {
    service_state(Config {
        capacity,
        partition: Uuid::new_v4().to_string(),
        ..Default::default()
    })
}
fn service_state(config: Config) -> (Worker, Receiver<Command>) {
    let capacity = config.capacity;
    let (tx, rx) = mpsc::sync_channel(capacity);
    let (response_tx, response_rx) = mpsc::sync_channel(capacity.min(64));
    let sender = Sender {
        tx,
        response_tx,
        stop: Arc::new(AtomicBool::new(false)),
        slots: Arc::new(AtomicUsize::new(0)),
        callback_threads: Arc::new(AtomicUsize::new(0)),
        callback_requests: Arc::new(AtomicUsize::new(0)),
        capacity,
        counters: Arc::new(Counters::default()),
        identity: Arc::new(OnceLock::new()),
        publication_sequences: config
            .statistics_wire
            .then(|| Arc::new(Mutex::new(HashMap::new()))),
    };
    (
        Worker::new(
            config,
            sender,
            Arc::new(AtomicBool::new(false)),
            response_rx,
        )
        .unwrap(),
        rx,
    )
}

#[test]
fn service_response_admission_is_bounded_and_independent_of_publication_queue() {
    let (worker, ordinary_rx) = state(1);
    let publication = || Command::Publish("topic".into(), "gz.msgs.Int32".into(), vec![5], None);
    assert_eq!(worker.sender.send(publication()), Ok(()));
    assert_eq!(worker.sender.send(publication()), Err(Error::QueueFull));

    let respond = || Request {
        data: Vec::new(),
        sender: worker.sender.clone(),
        route: vec![b"id".to_vec()],
        max: 16,
        owner_dead: Arc::new(AtomicBool::new(false)),
    };
    assert_eq!(respond().respond(vec![1], true), Ok(()));
    assert_eq!(respond().respond(vec![2], true), Err(Error::QueueFull));
    assert!(matches!(ordinary_rx.try_recv(), Ok(Command::Publish(..))));
    assert!(matches!(
        worker.response_rx.try_recv(),
        Ok(Command::Respond(route, data, true)) if route == [b"id".to_vec()] && data == [1]
    ));
    assert_eq!(worker.sender.counters.snapshot().queue_drops, 2);
    worker.sender.stop.store(true, Ordering::Release);
    assert_eq!(respond().respond(vec![3], true), Err(Error::Shutdown));
}

#[test]
fn local_echo_suppression_requires_recent_payload_and_expires_on_address_reuse() {
    let (worker, _) = state(1);
    let topic = qualified_name(&worker.config.partition, "", "/reuse").unwrap();
    let active = Arc::new(AtomicBool::new(true));
    let publication = LocalPublication {
        domain: MessageDomain::from_config(&worker.config),
        topic: "/reuse".into(),
        key: PeerKey {
            channel: 0,
            process: worker.process.clone(),
            node: worker.node.clone(),
            topic: topic.clone(),
            address: worker.address.clone(),
        },
        type_name: "gz.msgs.Int32".into(),
        publication: PublicationHandle {
            active: Arc::downgrade(&active),
            sequence: None,
            recent_frames: Arc::new(Mutex::new(VecDeque::new())),
        },
        stopped: Arc::downgrade(&worker.sender.stop),
        retired_at: None,
    };
    {
        let mut local = local_publications().lock().unwrap();
        local.push(publication);
    }
    worker.remember_local_publication(&topic, "gz.msgs.Int32", &[4]);
    worker.remember_local_publication(&topic, "gz.msgs.Int32", &[5]);
    assert!(!worker.local_wire_echo(&topic, &worker.address, "gz.msgs.Int32", &[4]));
    assert!(worker.local_wire_echo(&topic, &worker.address, "gz.msgs.Int32", &[5]));
    assert!(!worker.local_wire_echo(&topic, &worker.address, "gz.msgs.Int32", &[6]));
    assert!(!worker.local_wire_echo(&topic, &worker.address, "other.Type", &[5]));
    active.store(false, Ordering::Release);
    // A retired local address cannot suppress a later unrelated publisher's
    // different payload, even before the bounded tombstone expires.
    assert!(!worker.local_wire_echo(&topic, &worker.address, "gz.msgs.Int32", &[6]));
    local_publications()
        .lock()
        .unwrap()
        .iter_mut()
        .find(|entry| entry.key.topic == topic && entry.key.address == worker.address)
        .unwrap()
        .retired_at = Some(Instant::now() - Duration::from_secs(4));
    assert!(!worker.local_wire_echo(&topic, &worker.address, "gz.msgs.Int32", &[5]));
}

#[test]
fn local_publication_expiry_removes_only_the_old_prefix() {
    let (worker, _) = state(4);
    let topic = qualified_name(&worker.config.partition, "", "/ordered").unwrap();
    let active = Arc::new(AtomicBool::new(true));
    let now = Instant::now();
    let mut publication = LocalPublication {
        domain: MessageDomain::from_config(&worker.config),
        topic: "/ordered".into(),
        key: PeerKey {
            channel: 0,
            process: worker.process.clone(),
            node: worker.node.clone(),
            topic,
            address: worker.address.clone(),
        },
        type_name: "gz.msgs.Int32".into(),
        publication: PublicationHandle {
            active: Arc::downgrade(&active),
            sequence: None,
            recent_frames: Arc::new(Mutex::new(VecDeque::from([
                (frame_fingerprint(&[1]), 1, now - Duration::from_secs(4)),
                (frame_fingerprint(&[2]), 1, now - Duration::from_secs(2)),
                (frame_fingerprint(&[3]), 1, now),
            ]))),
        },
        stopped: Arc::downgrade(&worker.sender.stop),
        retired_at: None,
    };
    assert!(publication.retain_for_peer_expiry());
    let recent = publication.publication.recent_frames.lock().unwrap();
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].0, frame_fingerprint(&[2]));
    assert_eq!(recent[1].0, frame_fingerprint(&[3]));
}

#[test]
fn publication_handle_records_before_wire_and_cannot_write_after_lease_retirement() {
    let (mut worker, _) = state(4);
    let old_active = Arc::new(AtomicBool::new(true));
    let old = PublicationHandle {
        active: Arc::downgrade(&old_active),
        sequence: None,
        recent_frames: Arc::new(Mutex::new(VecDeque::new())),
    };
    let topic = qualified_name(&worker.config.partition, "", "/handle").unwrap();
    worker.command(Command::Publish(
        topic.clone(),
        "gz.msgs.Int32".into(),
        vec![5],
        Some(old.clone()),
    ));
    assert_eq!(old.recent_frames.lock().unwrap().len(), 1);
    assert_eq!(
        old.recent_frames.lock().unwrap()[0].0,
        frame_fingerprint(&[5])
    );

    old_active.store(false, Ordering::Release);
    let replacement_active = Arc::new(AtomicBool::new(true));
    let replacement = PublicationHandle {
        active: Arc::downgrade(&replacement_active),
        sequence: None,
        recent_frames: Arc::new(Mutex::new(VecDeque::new())),
    };
    worker.command(Command::Publish(
        topic,
        "gz.msgs.Int32".into(),
        vec![6],
        Some(old.clone()),
    ));
    assert_eq!(old.recent_frames.lock().unwrap().len(), 1);
    assert!(replacement.recent_frames.lock().unwrap().is_empty());
}

#[test]
fn poisoned_publisher_history_counts_routing_error_and_suppresses_ambiguous_echo() {
    let (mut worker, _) = state(4);
    let topic = qualified_name(&worker.config.partition, "", "/history_poison").unwrap();
    let active = Arc::new(AtomicBool::new(true));
    let publication = PublicationHandle {
        active: Arc::downgrade(&active),
        sequence: None,
        recent_frames: Arc::new(Mutex::new(VecDeque::new())),
    };
    local_publications().lock().unwrap().push(LocalPublication {
        domain: MessageDomain::from_config(&worker.config),
        topic: "/history_poison".into(),
        key: PeerKey {
            channel: 0,
            process: worker.process.clone(),
            node: worker.node.clone(),
            topic: topic.clone(),
            address: worker.address.clone(),
        },
        type_name: "gz.msgs.Int32".into(),
        publication: publication.clone(),
        stopped: Arc::downgrade(&worker.sender.stop),
        retired_at: None,
    });
    let poisoned = publication.recent_frames.clone();
    assert!(
        thread::spawn(move || {
            let _guard = poisoned.lock().unwrap();
            panic!("isolated history poison");
        })
        .join()
        .is_err()
    );
    let before = worker.sender.counters.snapshot();
    worker.command(Command::Publish(
        topic.clone(),
        "gz.msgs.Int32".into(),
        vec![5],
        Some(publication),
    ));
    assert_eq!(
        worker.sender.counters.snapshot().routing_errors,
        before.routing_errors + 1
    );
    assert!(worker.local_wire_echo(&topic, &worker.address, "gz.msgs.Int32", &[5]));
    assert_eq!(
        worker.sender.counters.snapshot().routing_errors,
        before.routing_errors + 2
    );
}

#[test]
fn poisoned_process_local_route_rejects_admission_and_counts_publish_degradation() {
    const CHILD: &str = "GZ_DIRECT_ORIGIN_POISON_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("runtime::tests::poisoned_process_local_route_rejects_admission_and_counts_publish_degradation")
            .env(CHILD, "1")
            .env("RUST_TEST_THREADS", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        return;
    }
    let node = Node::new(Config {
        partition: Uuid::new_v4().to_string(),
        ..Default::default()
    })
    .unwrap();
    let publisher = node.advertise("/poison", "gz.msgs.Int32").unwrap();
    let (worker, _) = state(2);
    let poison = thread::spawn(|| {
        let _guard = local_interests().lock().unwrap();
        panic!("isolated routing-registry poison");
    });
    assert!(poison.join().is_err());
    assert!(matches!(
        node.subscribe("/poison", "gz.msgs.Int32"),
        Err(Error::Native(message)) if message == "process-local subscriber registry poisoned"
    ));
    assert!(node.subscribed_topics().unwrap().is_empty());
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 1);
    let publication_poison = thread::spawn(|| {
        let _guard = local_publications().lock().unwrap();
        panic!("isolated publication-registry poison");
    });
    assert!(publication_poison.join().is_err());
    let routing_before = worker.sender.counters.snapshot().routing_errors;
    worker.remember_local_publication("@/poison@/poison", "gz.msgs.Int32", &[5]);
    assert!(!worker.local_wire_echo("@/poison@/poison", &worker.address, "gz.msgs.Int32", &[5]));
    assert_eq!(
        worker.sender.counters.snapshot().routing_errors,
        routing_before + 2
    );
    publisher.publish(vec![5]).unwrap();
    publisher.has_connections().unwrap();
    let stats = node.statistics();
    assert!(stats.routing_errors > 0);
    assert_eq!(stats.send_failures, 0);
}

#[test]
fn owner_retirement_cancels_pending_and_queued_requests() {
    let (mut worker, _) = state(2);
    let (request, pending_reply) = pending(Instant::now() + Duration::from_secs(5));
    worker.command(Command::Request(request));
    assert_eq!(worker.pending.len(), 1);
    worker.owner_dead.store(true, Ordering::Release);
    worker.retire_owner_requests();
    assert!(worker.pending.is_empty());
    assert!(matches!(
        pending_reply.try_recv(),
        Err(mpsc::TryRecvError::Disconnected)
    ));

    let (queued, queued_reply) = pending(Instant::now() + Duration::from_secs(5));
    worker.command(Command::Request(queued));
    assert_eq!(queued_reply.try_recv(), Ok(Err(Error::Shutdown)));
    assert!(worker.pending.is_empty());
}

#[test]
fn last_publisher_releases_worker_lifetime_after_node_drop() {
    for _ in 0..8 {
        let node = Node::new(Config {
            partition: Uuid::new_v4().to_string(),
            ..Default::default()
        })
        .unwrap();
        let worker = Arc::downgrade(&node.lifetime);
        let publisher = node.advertise("/survivor", "gz.msgs.Int32").unwrap();
        drop(node);
        assert!(worker.upgrade().is_some());
        let start = Instant::now();
        drop(publisher);
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(worker.upgrade().is_none());
    }
}
type ReplyReceiver = Receiver<Result<(Vec<u8>, bool), Error>>;
fn pending(deadline: Instant) -> (Pending, ReplyReceiver) {
    let (tx, rx) = mpsc::sync_channel(1);
    (
        Pending {
            topic: "/service".into(),
            request_type: "req".into(),
            response_type: "rep".into(),
            data: vec![1],
            deadline,
            reply: tx,
            sent: false,
        },
        rx,
    )
}
#[test]
fn frames_and_errors() {
    let message = protocol::discovery("process", Type::Heartbeat, None);
    let frame = protocol::encode(&message).unwrap();
    assert_eq!(protocol::decode(&frame).unwrap(), message);
    assert_eq!(
        u16::from_ne_bytes([frame[0], frame[1]]) as usize,
        frame.len() - 2
    );
    for frame in [vec![], vec![0], vec![1, 0], vec![1, 0, 255], vec![0; 65508]] {
        assert_eq!(protocol::decode(&frame), Err(Error::InvalidFrame));
    }
    let mut bad = message.clone();
    bad.version = 9;
    assert_eq!(
        protocol::decode(&protocol::encode(&bad).unwrap()),
        Err(Error::InvalidFrame)
    );
    bad.process_uuid = "x".repeat(65507);
    assert_eq!(protocol::encode(&bad), Err(Error::InvalidFrame));
    assert_eq!(protocol::text(&[255]), Err(Error::InvalidFrame));
    assert_eq!(Error::InvalidFrame.to_string(), "InvalidFrame");
    assert!(
        Error::from(zmq::Error::EINVAL)
            .to_string()
            .contains("Native")
    );
    assert!(
        Error::from(std::io::Error::other("failure"))
            .to_string()
            .contains("failure")
    );
    let error: &dyn std::error::Error = &Error::Timeout;
    assert!(error.source().is_none());
}
#[test]
fn upstream_fully_qualified_name_cross_product() {
    // Faithful TopicUtilsTest.testFullyQualifiedName input/expectation table.
    let long = "a".repeat(65536);
    let good = "a".repeat(65532);
    let partitions = vec![
        ("@partition", false, String::new()),
        ("@partition/@", false, String::new()),
        ("@@", false, String::new()),
        (&long, false, String::new()),
        ("partition", true, "@/partition@".into()),
        ("", true, "@@".into()),
        ("partition/", true, "@/partition@".into()),
        (&good, true, format!("@/{good}@")),
    ];
    let namespaces = vec![
        ("~ns", false, String::new()),
        (&long, false, String::new()),
        ("", true, "/".into()),
        ("abc", true, "/abc/".into()),
        (&good, true, format!("/{good}/")),
    ];
    let topics = vec![
        ("~/def", false, ""),
        ("~/def/", false, ""),
        ("~def", false, ""),
        ("~def/", false, ""),
        (&long, false, ""),
        ("/def", true, "/def"),
        ("def/", true, "def"),
        ("def/ghi", true, "def/ghi"),
        ("def/ghi/", true, "def/ghi"),
        (&good, true, &good),
    ];
    for (p, pvalid, prefix) in &partitions {
        for (ns, nsvalid, nsprefix) in &namespaces {
            for (t, tvalid, suffix) in &topics {
                let expected = format!(
                    "{prefix}{}{suffix}",
                    if t.starts_with('/') { "" } else { nsprefix }
                );
                let actual = qualified_name(p, ns, t);
                if *pvalid && *nsvalid && *tvalid && expected.len() <= 65535 {
                    assert_eq!(actual, Ok(expected));
                } else {
                    assert_eq!(actual, Err(Error::InvalidName));
                }
            }
        }
    }
    for invalid in [
        "",
        " ",
        "~a",
        "topic1 ",
        "abc//def",
        "ab~cd",
        "/",
        "~/",
        "~",
        "@partition",
        "topic:=",
    ] {
        assert!(qualified_name("", "", invalid).is_err());
    }
    for invalid in [
        "/",
        " ",
        "ns ",
        "abc//def",
        "ab~cd",
        "~/abcde",
        "~abcde",
        "@namespace",
        "namespace:=",
    ] {
        assert!(qualified_name(invalid, "", "topic").is_err());
        assert!(qualified_name("", invalid, "topic").is_err());
    }
    assert_eq!(qualified_name("/p/", "/ns/", "/a/"), Ok("@/p@/a".into()));
    assert_eq!(qualified_name("", "", "a\tb"), Ok("@@/a\tb".into()));
}
#[test]
fn bounds_and_invalid_configuration() {
    for config in [
        Config {
            capacity: 0,
            ..Default::default()
        },
        Config {
            capacity: i32::MAX as usize + 1,
            ..Default::default()
        },
        Config {
            max_message_bytes: 0,
            ..Default::default()
        },
        Config {
            multicast: Ipv4Addr::LOCALHOST,
            ..Default::default()
        },
        Config {
            interface: Ipv4Addr::UNSPECIFIED,
            ..Default::default()
        },
        Config {
            message_port: 10318,
            ..Default::default()
        },
        Config {
            message_port: 0,
            ..Default::default()
        },
        Config {
            service_port: 0,
            ..Default::default()
        },
    ] {
        assert!(matches!(Node::new(config), Err(Error::InvalidConfig)));
    }
    assert!(matches!(
        Node::new(Config {
            partition: "@".into(),
            ..Default::default()
        }),
        Err(Error::InvalidName)
    ));
    let (worker, rx) = state(1);
    let sender = worker.sender.clone();
    sender
        .send(Command::Publish("a".into(), "b".into(), vec![], None))
        .unwrap();
    assert_eq!(
        sender.send(Command::Publish("a".into(), "b".into(), vec![], None)),
        Err(Error::QueueFull)
    );
    drop(rx);
    assert_eq!(
        sender.send(Command::Publish("a".into(), "b".into(), vec![], None)),
        Err(Error::Shutdown)
    );
    sender.stop.store(true, Ordering::Release);
    assert_eq!(
        sender.send(Command::Publish("a".into(), "b".into(), vec![], None)),
        Err(Error::Shutdown)
    );
    let mut node = Node::new(Config {
        capacity: 1,
        max_message_bytes: 2,
        ..Default::default()
    })
    .unwrap();
    let publisher = node.advertise("topic", "type").unwrap();
    assert!(matches!(node.subscribe("a", "type"), Err(Error::QueueFull)));
    assert_eq!(publisher.publish(vec![0; 3]), Err(Error::InvalidFrame));
    assert_eq!(
        node.request("a", "a", "b", vec![0; 3], Duration::ZERO),
        Err(Error::InvalidFrame)
    );
    drop(publisher);
    thread::sleep(Duration::from_millis(10));
    let sub = node.subscribe("a", "type").unwrap();
    drop(sub);
    thread::sleep(Duration::from_millis(10));
    let service = node.serve("service", "req", "rep").unwrap();
    assert!(matches!(
        service.recv_timeout(Duration::ZERO),
        Err(Error::Timeout)
    ));
    drop(service);
    node.shutdown();
    assert!(matches!(
        node.serve("service", "a", "b"),
        Err(Error::Shutdown)
    ));
    assert!(matches!(
        node.advertise("topic", "type"),
        Err(Error::Shutdown)
    ));
    assert!(matches!(
        node.subscribe("topic", "type"),
        Err(Error::Shutdown)
    ));
    assert_eq!(
        node.request("a", "a", "b", vec![], Duration::ZERO),
        Err(Error::Shutdown)
    );
    let (worker, _) = state(1);
    let request = Request {
        data: vec![],
        sender: worker.sender.clone(),
        route: vec![b"local-request-id".to_vec()],
        max: 1,
        owner_dead: worker.owner_dead.clone(),
    };
    assert_eq!(request.respond(vec![0; 2], true), Err(Error::InvalidFrame));
    worker.owner_dead.store(true, Ordering::Release);
    let oversized_after_drop = Request {
        data: vec![],
        sender: worker.sender.clone(),
        route: vec![b"local-request-id".to_vec()],
        max: 1,
        owner_dead: worker.owner_dead.clone(),
    };
    assert_eq!(
        oversized_after_drop.respond(vec![0; 2], true),
        Err(Error::InvalidFrame)
    );
    let bounded_after_drop = Request {
        data: vec![],
        sender: worker.sender.clone(),
        route: vec![b"local-request-id".to_vec()],
        max: 1,
        owner_dead: worker.owner_dead.clone(),
    };
    assert_eq!(
        bounded_after_drop.respond(vec![0], true),
        Err(Error::Shutdown)
    );
}
#[test]
fn discovery_lifecycle_and_capacity() {
    let (mut worker, _) = state(2);
    let mut publisher = worker.wire_publisher(
        "/topic".into(),
        PubType::MsgPub(MessagePublisher {
            msg_type: "type".into(),
            ..Default::default()
        }),
    );
    publisher.process_uuid = "remote".into();
    publisher.node_uuid = "remote-node".into();
    let advertise = protocol::advertisement("remote", publisher.clone());
    worker.discovered(
        0,
        protocol::advertisement(&worker.process, publisher.clone()),
    );
    assert!(worker.peers.is_empty());
    let mut process = publisher.clone();
    process.scope = Scope::Process.into();
    worker.discovered(0, protocol::advertisement("remote", process));
    assert!(worker.peers.is_empty());
    worker.discovered(0, advertise.clone());
    assert_eq!(worker.peers.len(), 1);
    worker.discovered(0, protocol::discovery("remote", Type::Heartbeat, None));
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::Unadvertise,
            Some(DiscContents::Pub(publisher.clone())),
        ),
    );
    assert!(worker.peers.is_empty());
    worker.discovered(0, advertise.clone());
    worker.discovered(0, protocol::discovery("remote", Type::Bye, None));
    assert!(worker.peers.is_empty());
    worker.discovered(0, advertise);
    for i in 0..10 {
        let mut p = publisher.clone();
        p.node_uuid = i.to_string();
        worker.discovered(0, protocol::advertisement("remote", p));
    }
    assert_eq!(worker.peers.len(), 2);
    let active = Arc::new(AtomicBool::new(true));
    let (tx, _) = mpsc::sync_channel(1);
    worker.command(Command::Subscribe(
        "/topic".into(),
        "type".into(),
        tx.into(),
        active.clone(),
        SubscribeOptions::default(),
        None,
    ));
    assert_eq!(worker.subscriptions_connected.len(), 1);
    worker.command(Command::Advertise(
        "/topic".into(),
        "type".into(),
        AdvertiseOptions::default(),
        active.clone(),
        Arc::new(AtomicU8::new(0)),
    ));
    let (tx, _) = mpsc::sync_channel(1);
    worker.command(Command::Serve(
        "/service".into(),
        "req".into(),
        "rep".into(),
        Scope::All,
        Arc::new(tx),
        active.clone(),
    ));
    for channel in 0..2 {
        for topic in ["/topic", "/service", "/missing"] {
            worker.discovered(
                channel,
                protocol::discovery(
                    "remote",
                    Type::Subscribe,
                    Some(DiscContents::Sub(Subscriber {
                        topic: topic.into(),
                    })),
                ),
            );
        }
    }
    let (p1, _) = pending(Instant::now() + Duration::from_secs(1));
    let (p2, _) = pending(Instant::now() + Duration::from_secs(1));
    let (p3, r3) = pending(Instant::now() + Duration::from_secs(1));
    worker.command(Command::Request(p1));
    worker.command(Command::Request(p2));
    worker.command(Command::Request(p3));
    assert_eq!(r3.recv().unwrap(), Err(Error::QueueFull));
    for p in worker.pending.values_mut() {
        p.deadline = Instant::now();
    }
    worker.send_requests();
    assert!(worker.pending.is_empty());
    worker.command(Command::Respond(vec![], vec![], false));
    worker.connected.insert("tcp://127.0.0.1:1".into());
    worker.response_connections.insert(
        "tcp://127.0.0.1:1".into(),
        Instant::now() - Duration::from_secs(6),
    );
    active.store(false, Ordering::Release);
    worker.cleanup();
    assert!(worker.advertisements.is_empty());
    assert!(worker.services.is_empty());
    assert!(worker.subscriptions.is_empty());
    assert!(worker.subscriptions_connected.is_empty());
    assert!(worker.connected.is_empty());
    assert!(worker.response_connections.is_empty());
}

#[test]
fn malformed_multipart_is_drained_and_bounded() {
    let context = zmq::Context::new();
    let sender = context.socket(zmq::PAIR).unwrap();
    let receiver = context.socket(zmq::PAIR).unwrap();
    let address = format!("inproc://{}", Uuid::new_v4());
    receiver.bind(&address).unwrap();
    sender.connect(&address).unwrap();
    assert!(receive(&receiver, 4, &Counters::default(), || false).is_none());
    for count in [1, 3, 5, 1000] {
        sender.send_multipart(vec!["x"; count], 0).unwrap();
        assert!(receive(&receiver, 4, &Counters::default(), || false).is_none());
    }
    sender.send_multipart(["a", "b", "c", "d"], 0).unwrap();
    assert_eq!(
        receive(&receiver, 4, &Counters::default(), || false).unwrap(),
        [b"a", b"b", b"c", b"d"]
    );
}

#[test]
fn invalid_names_native_errors_and_deadlines() {
    assert!(matches!(
        Node::new(Config {
            max_message_bytes: usize::MAX,
            ..Default::default()
        }),
        Err(Error::InvalidConfig)
    ));
    let absent = Ipv4Addr::new(203, 0, 113, 1);
    assert!(matches!(
        Node::new(Config {
            interface: absent,
            ..Default::default()
        }),
        Err(Error::Native(_))
    ));
    assert!(
        udp(
            &Config {
                interface: absent,
                ..Default::default()
            },
            0
        )
        .is_err()
    );
    let occupied = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
    assert!(udp(&Config::default(), occupied.local_addr().unwrap().port()).is_err());
    let context = zmq::Context::new();
    assert!(
        socket(
            &context,
            zmq::PAIR,
            &Config {
                send_high_watermark: usize::MAX,
                ..Default::default()
            }
        )
        .is_err()
    );
    let node = Node::new(Config {
        capacity: 1,
        ..Default::default()
    })
    .unwrap();
    assert!(matches!(
        node.advertise("", "type"),
        Err(Error::InvalidName)
    ));
    assert!(matches!(
        node.subscribe("", "type"),
        Err(Error::InvalidName)
    ));
    assert!(matches!(
        node.serve("", "req", "rep"),
        Err(Error::InvalidName)
    ));
    assert_eq!(
        node.request("", "req", "rep", vec![], Duration::ZERO),
        Err(Error::InvalidName)
    );
    assert_eq!(
        node.request("topic", "req", "rep", vec![], Duration::MAX),
        Err(Error::InvalidConfig)
    );
    let _publisher = node.advertise("topic", "type").unwrap();
    assert!(matches!(
        node.advertise("other", "type"),
        Err(Error::QueueFull)
    ));
    assert!(matches!(
        node.serve("topic", "req", "rep"),
        Err(Error::QueueFull)
    ));
}

#[test]
fn local_request_overload_and_reply_routes() {
    let (mut worker, _) = state(2);
    let active = Arc::new(AtomicBool::new(true));
    let (tx, rx) = mpsc::sync_channel(1);
    worker.command(Command::Serve(
        "/service".into(),
        "req".into(),
        "rep".into(),
        Scope::All,
        Arc::new(tx),
        active,
    ));
    let (p1, r1) = pending(Instant::now() + Duration::from_secs(1));
    worker.command(Command::Request(p1));
    let (p2, r2) = pending(Instant::now() + Duration::from_secs(1));
    worker.command(Command::Request(p2));
    assert_eq!(r2.recv().unwrap(), Err(Error::QueueFull));
    let incoming = rx.recv().unwrap();
    worker.command(Command::Respond(incoming.route, vec![9], false));
    assert_eq!(r1.recv().unwrap(), Ok((vec![9], false)));
    worker.command(Command::Respond(vec![b"unknown".to_vec()], vec![], true));
    worker.command(Command::Respond(vec![vec![255]], vec![], true));
    let route = vec![b"invalid protocol".to_vec(), vec![], vec![], vec![], vec![]];
    worker.command(Command::Respond(route, vec![], true));
    assert!(worker.replies.is_empty());
    let route = vec![
        b"tcp://127.0.0.1:1".to_vec(),
        vec![],
        vec![],
        vec![],
        vec![],
    ];
    worker.command(Command::Respond(route.clone(), vec![], false));
    worker.command(Command::Respond(route, vec![], true));
    assert_eq!(worker.replies.len(), 2);
    assert_eq!(worker.response_connections.len(), 1);
    worker.command(Command::Respond(
        vec![vec![255], vec![], vec![], vec![], vec![]],
        vec![],
        true,
    ));
    assert_eq!(worker.replies.len(), 2);
    worker.replies.clear();
    worker
        .response_connections
        .insert("tcp://127.0.0.1:2".into(), Instant::now());
    worker.command(Command::Respond(
        vec![
            b"tcp://127.0.0.1:3".to_vec(),
            vec![],
            vec![],
            vec![],
            vec![],
        ],
        vec![],
        true,
    ));
    assert!(worker.replies.is_empty());
}

#[test]
fn unmatched_request_uses_later_typed_local_service_once() {
    let (mut worker, _) = state(4);
    let (request, reply) = pending(Instant::now() + Duration::from_secs(1));
    worker.command(Command::Request(request));
    assert_eq!(worker.pending.len(), 1);

    let (wrong_tx, wrong_rx) = mpsc::sync_channel(1);
    worker.command(Command::Serve(
        "/service".into(),
        "wrong".into(),
        "rep".into(),
        Scope::All,
        Arc::new(wrong_tx),
        Arc::new(AtomicBool::new(true)),
    ));
    worker.send_requests();
    assert!(matches!(
        wrong_rx.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert_eq!(worker.pending.len(), 1);

    let (tx, rx) = mpsc::sync_channel(1);
    worker.command(Command::Serve(
        "/service".into(),
        "req".into(),
        "rep".into(),
        Scope::All,
        Arc::new(tx),
        Arc::new(AtomicBool::new(true)),
    ));
    worker.send_requests();
    let incoming = rx.try_recv().unwrap();
    assert_eq!(incoming.data, [1]);
    worker.send_requests();
    assert!(matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
    worker.command(Command::Respond(incoming.route, vec![5], true));
    assert_eq!(reply.try_recv().unwrap(), Ok((vec![5], true)));
    assert!(worker.pending.is_empty());
}

#[test]
fn late_local_service_retains_oneway_and_queue_bounds() {
    let (mut worker, _) = state(2);
    let (mut request, reply) = pending(Instant::now() + Duration::from_secs(1));
    request.response_type = EMPTY_TYPE.into();
    worker.command(Command::Request(request));
    let (tx, rx) = mpsc::sync_channel(1);
    worker.command(Command::Serve(
        "/service".into(),
        "req".into(),
        EMPTY_TYPE.into(),
        Scope::All,
        Arc::new(tx),
        Arc::new(AtomicBool::new(true)),
    ));
    worker.send_requests();
    assert!(!rx.try_recv().unwrap().expects_response());
    assert_eq!(reply.try_recv().unwrap(), Ok((Vec::new(), true)));
    assert!(worker.pending.is_empty());

    let (mut worker, _) = state(2);
    let (request, reply) = pending(Instant::now() + Duration::from_secs(1));
    worker.command(Command::Request(request));
    let (tx, _rx) = mpsc::sync_channel(0);
    worker.command(Command::Serve(
        "/service".into(),
        "req".into(),
        "rep".into(),
        Scope::All,
        Arc::new(tx),
        Arc::new(AtomicBool::new(true)),
    ));
    worker.send_requests();
    assert_eq!(reply.try_recv().unwrap(), Err(Error::QueueFull));
    assert!(worker.pending.is_empty());
    assert_eq!(worker.sender.counters.snapshot().queue_drops, 1);

    let (mut worker, _) = state(2);
    let (request, _reply) = pending(Instant::now() + Duration::from_secs(1));
    worker.command(Command::Request(request));
    worker.pending.values_mut().next().unwrap().sent = true;
    let (tx, rx) = mpsc::sync_channel(1);
    worker.command(Command::Serve(
        "/service".into(),
        "req".into(),
        "rep".into(),
        Scope::All,
        Arc::new(tx),
        Arc::new(AtomicBool::new(true)),
    ));
    worker.send_requests();
    assert!(matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
    assert_eq!(worker.pending.len(), 1);
}

#[test]
fn service_frames_validate_type_node_topic_and_request_correlation() {
    let (mut worker, _) = state(10);
    let active = Arc::new(AtomicBool::new(true));
    let (tx, rx) = mpsc::sync_channel(1);
    worker.command(Command::Serve(
        "/service".into(),
        "req".into(),
        "rep".into(),
        Scope::All,
        Arc::new(tx),
        active.clone(),
    ));
    let context = zmq::Context::new();
    let request = context.socket(zmq::DEALER).unwrap();
    request.connect(&worker.service_address).unwrap();
    for (topic, req, rep) in [
        ("/wrong", "req", "rep"),
        ("/service", "wrong", "rep"),
        ("/service", "req", "wrong"),
        ("/service", "req", "rep"),
        ("/service", "req", "rep"),
    ] {
        request
            .send_multipart(
                [
                    topic,
                    "tcp://127.0.0.1:1",
                    "response-id",
                    "node",
                    "request-id",
                    "data",
                    req,
                    rep,
                ],
                0,
            )
            .unwrap();
        assert_ne!(worker.router.poll(zmq::POLLIN, 1000).unwrap(), 0);
        worker.receive_requests();
    }
    assert_eq!(worker.sender.counters.snapshot().queue_drops, 1);
    let incoming = rx.try_recv().unwrap();
    assert_eq!(incoming.data, b"data");
    assert_eq!(incoming.route[4], b"request-id");
    assert!(rx.try_recv().is_err());
    active.store(false, Ordering::Release);
    request
        .send_multipart(
            [
                "/service",
                "tcp://127.0.0.1:1",
                "response-id",
                "node",
                "request-id",
                "data",
                "req",
                "rep",
            ],
            0,
        )
        .unwrap();
    assert_ne!(worker.router.poll(zmq::POLLIN, 1000).unwrap(), 0);
    worker.receive_requests();
    assert!(rx.try_recv().is_err());
    let responses = context.socket(zmq::ROUTER).unwrap();
    responses.set_router_mandatory(true).unwrap();
    responses.bind("tcp://127.0.0.1:*").unwrap();
    worker
        .responses
        .connect(&responses.get_last_endpoint().unwrap().unwrap())
        .unwrap();
    let (p, reply) = pending(Instant::now() + Duration::from_secs(5));
    worker.pending.insert("request-id".into(), p);
    for (topic, node, id) in [
        ("/service", "wrong", b"request-id".as_slice()),
        ("/wrong", worker.node.as_str(), b"request-id".as_slice()),
        ("/service", worker.node.as_str(), b"unknown".as_slice()),
        ("/service", worker.node.as_str(), &[255]),
    ]
    .map(|(t, n, i)| (t.to_string(), n.to_string(), i.to_vec()))
    {
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if responses
                .send_multipart(
                    [
                        worker.response_id.as_bytes(),
                        topic.as_bytes(),
                        node.as_bytes(),
                        &id,
                        b"response",
                        b"0",
                    ],
                    zmq::DONTWAIT,
                )
                .is_ok()
            {
                break;
            }
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        assert_ne!(worker.responses.poll(zmq::POLLIN, 1000).unwrap(), 0);
        worker.receive_responses();
        assert_eq!(worker.pending.len(), 1);
    }
    responses
        .send_multipart(
            [
                worker.response_id.as_bytes(),
                b"/service",
                worker.node.as_bytes(),
                b"request-id",
                b"response",
                b"0",
            ],
            0,
        )
        .unwrap();
    assert_ne!(worker.responses.poll(zmq::POLLIN, 1000).unwrap(), 0);
    worker.receive_responses();
    assert!(worker.pending.is_empty());
    assert_eq!(reply.recv().unwrap(), Ok((b"response".to_vec(), false)));
}

#[test]
fn delayed_reply_route_retries_until_ready_and_expires() {
    let (mut worker, _) = state(2);
    worker.config.reply_timeout = Duration::from_millis(500);
    let port = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let address = format!("tcp://127.0.0.1:{port}");
    let route = vec![
        address.as_bytes().to_vec(),
        b"late-receiver".to_vec(),
        b"topic".to_vec(),
        b"node".to_vec(),
        b"request".to_vec(),
    ];
    worker.command(Command::Respond(route, vec![42], true));
    worker.send_replies();
    assert_eq!(worker.replies.len(), 1);
    assert!(worker.sender.counters.snapshot().send_failures > 0);
    thread::sleep(Duration::from_millis(100));
    let context = zmq::Context::new();
    let receiver = context.socket(zmq::ROUTER).unwrap();
    receiver.set_identity(b"late-receiver").unwrap();
    receiver.bind(&address).unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    while receiver.poll(zmq::POLLIN, 0).unwrap() == 0 {
        worker.send_replies();
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    let frames = receiver.recv_multipart(0).unwrap();
    assert_eq!(
        &frames[1..],
        &[
            b"topic".to_vec(),
            b"node".to_vec(),
            b"request".to_vec(),
            vec![42],
            b"1".to_vec()
        ]
    );
    assert!(worker.replies.is_empty());
    assert_eq!(worker.sender.counters.snapshot().reply_timeouts, 0);
    worker.config.reply_timeout = Duration::from_millis(5);
    worker.command(Command::Respond(
        vec![
            address.into_bytes(),
            b"missing-identity".to_vec(),
            b"topic".to_vec(),
            b"node".to_vec(),
            b"request".to_vec(),
        ],
        vec![],
        true,
    ));
    thread::sleep(Duration::from_millis(10));
    worker.send_replies();
    assert!(worker.replies.is_empty());
    assert_eq!(worker.sender.counters.snapshot().reply_timeouts, 1);
}
#[test]
fn queue_saturation_is_observable_in_local_counters() {
    let (mut worker, _) = state(2);
    let (tx, rx) = mpsc::sync_channel(1);
    let active = Arc::new(AtomicBool::new(true));
    worker.command(Command::Subscribe(
        "topic".into(),
        "type".into(),
        tx.into(),
        active,
        SubscribeOptions::default(),
        None,
    ));
    let message = Message {
        topic: "topic".into(),
        type_name: "type".into(),
        data: vec![1],
    };
    worker.deliver(&message, true);
    worker.deliver(&message, true);
    assert_eq!(rx.try_recv().unwrap().into_message(), message);
    assert!(rx.try_recv().is_err());
    assert_eq!(worker.sender.counters.snapshot().queue_drops, 1);
    let counters = Counters::default();
    let context = zmq::Context::new();
    let sender = context.socket(zmq::PAIR).unwrap();
    let receiver = context.socket(zmq::PAIR).unwrap();
    let address = format!("inproc://{}", Uuid::new_v4());
    receiver.bind(&address).unwrap();
    sender.connect(&address).unwrap();
    sender.send_multipart(["bad"], 0).unwrap();
    assert!(receive(&receiver, 4, &counters, || false).is_none());
    assert_eq!(counters.snapshot().malformed_frames, 1);
    let node = Node::new(Config::default()).unwrap();
    assert_eq!(node.statistics(), Statistics::default());
}

#[test]
fn malformed_publications_and_discovery_errors_are_counted() {
    let (mut worker, _) = state(5);
    let (tx, rx) = mpsc::sync_channel(2);
    worker.subscriptions.push(SubscriberState {
        topic: "topic".into(),
        type_name: "type".into(),
        tx: tx.into(),
        active: Arc::new(AtomicBool::new(true)),
        options: SubscribeOptions::default(),
        throttle: None,
    });
    let context = zmq::Context::new();
    let publisher = context.socket(zmq::XPUB).unwrap();
    publisher.bind("tcp://127.0.0.1:*").unwrap();
    worker.subscriber.set_subscribe(b"").unwrap();
    worker
        .subscriber
        .connect(&publisher.get_last_endpoint().unwrap().unwrap())
        .unwrap();
    assert_ne!(publisher.poll(zmq::POLLIN, 1000).unwrap(), 0);
    publisher.recv_msg(0).unwrap();
    for frames in [
        vec![b"topic".as_slice(), b"address", b"data", &[255]],
        vec![b"topic".as_slice(), b"malformed"],
        vec![b"topic".as_slice(), b"address", b"data", b"type"],
    ] {
        publisher.send_multipart(frames, 0).unwrap();
        assert_ne!(worker.subscriber.poll(zmq::POLLIN, 1000).unwrap(), 0);
        worker.receive_publications();
    }
    assert_eq!(rx.try_recv().unwrap().data, b"data");
    assert_eq!(worker.sender.counters.snapshot().malformed_frames, 2);
    worker.announce(
        0,
        protocol::discovery(&"x".repeat(65507), Type::Heartbeat, None),
    );
    assert_eq!(worker.sender.counters.snapshot().malformed_frames, 3);
    worker.config.multicast = Ipv4Addr::BROADCAST;
    worker.announce(0, protocol::discovery("process", Type::Heartbeat, None));
    assert_eq!(worker.sender.counters.snapshot().send_failures, 1);
    worker.command(Command::Respond(
        vec![vec![255], vec![], vec![], vec![], vec![]],
        vec![],
        true,
    ));
    assert_eq!(worker.sender.counters.snapshot().malformed_frames, 4);
}

#[test]
fn pending_capacity_and_shared_topic_filter_lifetime() {
    let (mut worker, _) = state(2);
    let (p1, _) = pending(Instant::now() + Duration::from_secs(1));
    let (p2, _) = pending(Instant::now() + Duration::from_secs(1));
    let (p3, r3) = pending(Instant::now() + Duration::from_secs(1));
    worker.command(Command::Request(p1));
    worker.command(Command::Request(p2));
    worker.command(Command::Request(p3));
    assert_eq!(worker.pending.len(), 2);
    assert_eq!(r3.recv().unwrap(), Err(Error::QueueFull));
    assert_eq!(worker.sender.counters.snapshot().queue_drops, 1);
    let old = Arc::new(AtomicBool::new(false));
    let (tx, _) = mpsc::sync_channel(1);
    worker.subscriptions.push(SubscriberState {
        topic: "topic".into(),
        type_name: "type".into(),
        tx: tx.into(),
        active: old,
        options: SubscribeOptions::default(),
        throttle: None,
    });
    let (tx, rx) = mpsc::sync_channel(1);
    worker.subscriptions.push(SubscriberState {
        topic: "topic".into(),
        type_name: "type".into(),
        tx: tx.into(),
        active: Arc::new(AtomicBool::new(true)),
        options: SubscribeOptions::default(),
        throttle: None,
    });
    worker.cleanup();
    assert_eq!(worker.subscriptions.len(), 1);
    worker.deliver(
        &Message {
            topic: "topic".into(),
            type_name: "type".into(),
            data: vec![7],
        },
        true,
    );
    assert_eq!(rx.try_recv().unwrap().data, vec![7]);
    for timeout in [Duration::ZERO, Duration::MAX] {
        assert!(matches!(
            Node::new(Config {
                reply_timeout: timeout,
                ..Default::default()
            }),
            Err(Error::InvalidConfig)
        ));
    }
}

#[test]
fn typed_discovery_rejects_wrong_publisher_kind_and_unknown_scope() {
    let (mut worker, _) = state(2);
    let mut service = worker.wire_publisher(
        "topic".into(),
        PubType::SrvPub(ServicePublisher {
            socket_id: "service".into(),
            request_type: "req".into(),
            response_type: "rep".into(),
        }),
    );
    service.process_uuid = "remote".into();
    worker.register_subscriber(&service);
    assert!(worker.subscriptions_connected.is_empty());
    service.scope = 500;
    worker.discovered(1, protocol::advertisement("remote", service));
    assert!(worker.peers.is_empty());
}

#[test]
fn compatible_discovery_preserves_runtime_admission_and_unknown_scope_rejection() {
    use gz_msgs::prost::Message;
    for (scope, peers) in [(Scope::All as i32, 1), (500, 0)] {
        let (mut worker, _) = state(2);
        let mut publication = worker.wire_publisher(
            "/wire".into(),
            PubType::MsgPub(MessagePublisher {
                msg_type: "gz.msgs.StringMsg".into(),
                ..Default::default()
            }),
        );
        publication.process_uuid = "remote-process".into();
        publication.node_uuid = "remote-node".into();
        publication.scope = scope;
        let expected = protocol::advertisement("remote-process", publication);
        let mut body = expected.encode_to_vec();
        // A wrong-wire occurrence of process_uuid must not erase the known string.
        body.extend([0x18, 1]);
        assert!(Discovery::decode(body.as_slice()).is_err());
        let mut frame = (body.len() as u16).to_ne_bytes().to_vec();
        frame.extend(body);
        let decoded = protocol::decode(&frame).unwrap();
        assert_eq!(decoded, expected);
        worker.discovered(0, decoded);
        assert_eq!(worker.peers.len(), peers);
        assert_eq!(worker.sender.counters.snapshot(), Statistics::default());
    }
}

#[test]
fn oneway_wire_provider_does_not_enqueue_or_connect_a_reply() {
    let (mut worker, commands) = state(2);
    let (tx, rx) = mpsc::sync_channel(1);
    worker.command(Command::Serve(
        "/oneway".into(),
        "req".into(),
        EMPTY_TYPE.into(),
        Scope::All,
        Arc::new(tx),
        Arc::new(AtomicBool::new(true)),
    ));
    let context = zmq::Context::new();
    let requester = context.socket(zmq::DEALER).unwrap();
    requester.connect(&worker.service_address).unwrap();
    requester
        .send_multipart(
            [
                b"/oneway".as_slice(),
                b"tcp://127.0.0.1:1",
                b"receiver",
                b"node",
                b"id",
                b"payload",
                b"req",
                EMPTY_TYPE.as_bytes(),
            ],
            0,
        )
        .unwrap();
    assert_ne!(worker.router.poll(zmq::POLLIN, 1000).unwrap(), 0);
    worker.receive_requests();
    let request = rx.try_recv().unwrap();
    assert_eq!(request.data, b"payload");
    assert!(!request.expects_response());
    request.respond(b"discarded".to_vec(), false).unwrap();
    assert!(matches!(
        commands.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert!(worker.replies.is_empty());
    assert!(worker.response_connections.is_empty());
    assert_eq!(requester.poll(zmq::POLLIN, 10).unwrap(), 0);
}

#[test]
fn expired_local_requests_never_enter_the_service_queue() {
    for response_type in [EMPTY_TYPE, "rep"] {
        let (mut worker, _) = state(2);
        let (tx, rx) = mpsc::sync_channel(1);
        worker.command(Command::Serve(
            "/service".into(),
            "req".into(),
            response_type.into(),
            Scope::All,
            Arc::new(tx),
            Arc::new(AtomicBool::new(true)),
        ));
        let (mut pending, reply) = pending(Instant::now());
        pending.response_type = response_type.into();
        worker.command(Command::Request(pending));
        assert_eq!(reply.try_recv().unwrap(), Err(Error::Timeout));
        assert!(matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
        assert!(worker.pending.is_empty());
        assert_eq!(worker.sender.counters.snapshot().queue_drops, 0);
    }
}

#[test]
fn cancellation_interrupts_malformed_multipart_without_counting_a_wire_error() {
    let context = zmq::Context::new();
    let sender = context.socket(zmq::PAIR).unwrap();
    let receiver = context.socket(zmq::PAIR).unwrap();
    let address = format!("inproc://{}", Uuid::new_v4());
    receiver.bind(&address).unwrap();
    sender.connect(&address).unwrap();
    for consumed in [0, 2, 5] {
        let counters = Counters::default();
        sender.send_multipart(vec!["part"; 1000], 0).unwrap();
        assert_ne!(receiver.poll(zmq::POLLIN, 1000).unwrap(), 0);
        let mut checks = 0;
        assert!(
            receive(&receiver, 4, &counters, || {
                checks += 1;
                checks > consumed
            })
            .is_none()
        );
        assert_eq!(checks, consumed + 1);
        assert_eq!(counters.snapshot(), Statistics::default());
        // The production owner now closes its socket. Inspect the unread tail
        // here to prove cancellation did not drain an arbitrarily long message.
        assert_eq!(
            receiver.recv_multipart(zmq::DONTWAIT).unwrap().len(),
            1000 - consumed
        );
    }
}

#[cfg(unix)]
#[test]
fn native_local_interface_order_flags_deduplication_and_fallback() {
    fn record(index: Option<u32>, address: &str, multicast: bool, up: bool) -> LocalInterface {
        LocalInterface {
            index,
            address: address.parse().unwrap(),
            multicast,
            up,
        }
    }
    assert_eq!(eligible_interfaces(vec![]), vec![Ipv4Addr::LOCALHOST]);
    let selected = eligible_interfaces(vec![
        record(None, "192.0.2.1", true, true),
        record(Some(1), "::1", true, true),
        record(Some(1), "192.0.2.2", true, true),
        record(Some(2), "127.0.0.1", true, true),
        record(Some(3), "192.0.2.3", false, true),
        record(Some(4), "192.0.2.4", true, false),
        record(Some(5), "192.0.2.5", true, true),
        record(Some(6), "192.0.2.5", true, true),
        record(Some(7), "192.0.2.7", true, true),
    ]);
    assert_eq!(
        selected,
        vec![
            "192.0.2.5".parse::<Ipv4Addr>().unwrap(),
            "192.0.2.7".parse().unwrap()
        ]
    );
    assert!(!local_interfaces().unwrap().is_empty());
}

#[test]
fn host_discovery_advertise_and_withdraw_follow_native_source_membership() {
    let (mut worker, _) = state(10);
    worker.host_interfaces = vec!["192.0.2.5".parse().unwrap()];
    let mut publisher = worker.wire_publisher(
        "/host".into(),
        PubType::SrvPub(ServicePublisher {
            socket_id: "server".into(),
            request_type: "req".into(),
            response_type: "rep".into(),
        }),
    );
    publisher.process_uuid = "remote".into();
    publisher.scope = Scope::Host.into();
    for (source, expected) in [
        ("203.0.113.8", false),
        ("::1", false),
        ("127.0.0.2", true),
        ("192.0.2.5", true),
    ] {
        worker.peers.clear();
        worker.discovery(
            1,
            protocol::advertisement("remote", publisher.clone()),
            source.parse().unwrap(),
        );
        assert_eq!(!worker.peers.is_empty(), expected);
    }
    let withdrawal = protocol::discovery(
        "remote",
        Type::Unadvertise,
        Some(DiscContents::Pub(publisher.clone())),
    );
    worker.discovery(1, withdrawal.clone(), "203.0.113.8".parse().unwrap());
    assert_eq!(worker.peers.len(), 1);
    worker.discovery(1, withdrawal, "127.10.0.8".parse().unwrap());
    assert!(worker.peers.is_empty());
    publisher.scope = Scope::All.into();
    worker.discovery(
        1,
        protocol::advertisement("remote", publisher),
        "203.0.113.8".parse().unwrap(),
    );
    assert_eq!(worker.peers.len(), 1);
}

#[test]
fn subscriber_listing_uses_message_channel_live_leases_and_native_wildcard() {
    let (mut worker, _) = state(4);
    let dead = Arc::new(AtomicBool::new(false));
    for (topic, type_name, active) in [
        (
            "/typed",
            "gz.msgs.StringMsg",
            Arc::new(AtomicBool::new(true)),
        ),
        ("/wildcard", "", Arc::new(AtomicBool::new(true))),
        ("/dead", "gz.msgs.StringMsg", dead),
    ] {
        let (tx, _) = mpsc::sync_channel(1);
        worker.command(Command::Subscribe(
            topic.into(),
            type_name.into(),
            tx.into(),
            active,
            SubscribeOptions::default(),
            None,
        ));
    }
    let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_millis(30)))
        .unwrap();
    worker.config.multicast = Ipv4Addr::LOCALHOST;
    worker.config.message_port = receiver.local_addr().unwrap().port();
    let request = protocol::discovery("remote", Type::SubscribersReq, None);
    worker.discovered(1, request.clone());
    worker.discovered(
        0,
        protocol::discovery(&worker.process.clone(), Type::SubscribersReq, None),
    );
    let mut buffer = vec![0; protocol::MAX_DATAGRAM];
    assert!(receiver.recv(&mut buffer).is_err());
    worker.discovered(0, request);
    let mut replies = Vec::new();
    for _ in 0..2 {
        let len = receiver.recv(&mut buffer).unwrap();
        let message = protocol::decode(&buffer[..len]).unwrap();
        assert_eq!(message.r#type, Type::SubscribersRep as i32);
        assert_eq!(message.process_uuid, worker.process);
        let Some(DiscContents::Pub(publisher)) = message.disc_contents else {
            panic!("missing publisher")
        };
        assert_eq!(publisher.address, worker.address);
        assert_eq!(publisher.node_uuid, worker.node);
        assert_eq!(publisher.process_uuid, worker.process);
        assert_eq!(publisher.scope, Scope::All as i32);
        let Some(PubType::MsgPub(info)) = publisher.pub_type else {
            panic!("wrong publisher kind")
        };
        assert!(info.ctrl.is_empty());
        replies.push((publisher.topic, info.msg_type));
    }
    replies.sort();
    assert_eq!(
        replies,
        [
            ("/typed".into(), "gz.msgs.StringMsg".into()),
            ("/wildcard".into(), "google.protobuf.Message".into())
        ]
    );
    assert!(receiver.recv(&mut buffer).is_err());
}

#[test]
fn subscriber_withdrawal_waits_for_last_lease_and_emits_one_generic_end() {
    let (mut worker, _) = state(2);
    let first = Arc::new(AtomicBool::new(true));
    let last = Arc::new(AtomicBool::new(true));
    for active in [first.clone(), last.clone()] {
        let (tx, _) = mpsc::sync_channel(1);
        worker.command(Command::Subscribe(
            "/shared".into(),
            "gz.msgs.StringMsg".into(),
            tx.into(),
            active,
            SubscribeOptions::default(),
            None,
        ));
    }
    let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_millis(30)))
        .unwrap();
    worker.config.multicast = Ipv4Addr::LOCALHOST;
    worker.config.message_port = receiver.local_addr().unwrap().port();
    first.store(false, Ordering::Release);
    worker.cleanup();
    let mut buffer = vec![0; protocol::MAX_DATAGRAM];
    assert!(receiver.recv(&mut buffer).is_err());
    assert_eq!(worker.subscriptions.len(), 1);
    last.store(false, Ordering::Release);
    worker.cleanup();
    assert!(worker.subscriptions.is_empty());
    let len = receiver.recv(&mut buffer).unwrap();
    let message = protocol::decode(&buffer[..len]).unwrap();
    assert_eq!(message.r#type, Type::EndConnection as i32);
    assert_eq!(
        message.disc_contents,
        Some(DiscContents::Pub(
            worker.subscriber_publisher("/shared", "")
        ))
    );
    worker.cleanup();
    assert!(receiver.recv(&mut buffer).is_err());
}

#[test]
fn upstream_as_valid_topic_preserves_all_original_inputs_and_assertions() {
    for unmodified in [
        "/abc",
        "abc/de",
        "a",
        "ABC/",
        "/abc",
        "/abc/d",
        "/abc/d/e",
        "a(bc)d-e_f=h+i.j",
    ] {
        let result = crate::as_valid_topic(unmodified);
        assert_eq!(result, unmodified);
        assert!(!result.is_empty() && crate::names::valid(&result));
    }
    for (input, expected) in [
        ("a b  c", "a_b__c"),
        ("a@b@c", "abc"),
        ("a:=b:=c", "abc"),
        ("a//b/c", "ab/c"),
        ("a~b~c", "abc"),
    ] {
        let result = crate::as_valid_topic(input);
        assert_eq!(result, expected);
        assert!(!result.is_empty() && crate::names::valid(&result));
    }
    for input in ["", "@@@", "~@~"] {
        let result = crate::as_valid_topic(input);
        assert!(result.is_empty());
        assert!(result.is_empty() || !crate::names::valid(&result));
    }
}

#[test]
fn message_and_service_discovery_keep_separate_native_namespaces() {
    let (mut worker, _) = state(4);
    let mut publication = worker.wire_publisher(
        "/dual".into(),
        PubType::MsgPub(MessagePublisher {
            msg_type: "gz.msgs.StringMsg".into(),
            ..Default::default()
        }),
    );
    publication.process_uuid = "remote-process".into();
    publication.node_uuid = "remote-node".into();
    let mut service = publication.clone();
    service.address = "tcp://127.0.0.1:45001".into();
    service.pub_type = Some(PubType::SrvPub(ServicePublisher {
        request_type: "gz.msgs.StringMsg".into(),
        response_type: "gz.msgs.StringMsg".into(),
        ..Default::default()
    }));
    worker.discovered(
        0,
        protocol::advertisement("remote-process", publication.clone()),
    );
    worker.discovered(
        1,
        protocol::advertisement("remote-process", service.clone()),
    );
    assert_eq!(
        worker.peers.len(),
        2,
        "native discovery stores message and service publishers independently"
    );
    let mut other_address = publication.clone();
    other_address.address = "tcp://127.0.0.1:45002".into();
    worker.discovered(0, protocol::advertisement("remote-process", other_address));
    assert_eq!(worker.peers.len(), 3);
    let mut duplicate = publication.clone();
    duplicate.pub_type = Some(PubType::MsgPub(MessagePublisher {
        msg_type: "changed-descriptor".into(),
        ..Default::default()
    }));
    worker.discovered(0, protocol::advertisement("remote-process", duplicate));
    assert_eq!(worker.peers.len(), 3);
    let original_types = worker.peers.values().filter(|(p, _)| {
        matches!(&p.pub_type, Some(PubType::MsgPub(m)) if m.msg_type == "gz.msgs.StringMsg")
    }).count();
    assert_eq!(original_types, 2);
    worker.discovered(
        0,
        protocol::discovery(
            "remote-process",
            Type::Unadvertise,
            Some(DiscContents::Pub(publication)),
        ),
    );
    assert_eq!(worker.peers.len(), 1); // Native withdrawal removes every address for this node/topic on its channel.
    assert_eq!(worker.peers.values().next().unwrap().0, service);
    worker.discovered(
        1,
        protocol::discovery(
            "remote-process",
            Type::Unadvertise,
            Some(DiscContents::Pub(service)),
        ),
    );
    assert!(worker.peers.is_empty());
}

#[test]
fn peer_activity_and_bye_only_affect_their_native_discovery_channel() {
    let (mut worker, _) = state(4);
    for channel in 0..2 {
        let mut p = worker.wire_publisher(
            "/shared".into(),
            if channel == 0 {
                PubType::MsgPub(MessagePublisher::default())
            } else {
                PubType::SrvPub(ServicePublisher::default())
            },
        );
        p.process_uuid = "remote".into();
        worker.discovered(channel, protocol::advertisement("remote", p));
    }
    let mut unrelated = worker.wire_publisher(
        "/shared".into(),
        PubType::MsgPub(MessagePublisher::default()),
    );
    unrelated.process_uuid = "unrelated-process".into();
    worker.discovered(0, protocol::advertisement("unrelated-process", unrelated));
    assert_eq!(worker.peers.len(), 3);
    for kind in [
        Type::Heartbeat,
        Type::Subscribe,
        Type::SubscribersReq,
        Type::NewConnection,
        Type::EndConnection,
    ] {
        for channel in 0..2 {
            let old = Instant::now() - Duration::from_secs(5);
            for (_, stamp) in worker.peers.values_mut() {
                *stamp = old;
            }
            worker.discovered(channel, protocol::discovery("remote", kind, None));
            for (key, (_, stamp)) in &worker.peers {
                assert_eq!(
                    *stamp > old,
                    key.channel == channel && key.process == "remote"
                );
            }
        }
    }
    worker.discovered(0, protocol::discovery("remote", Type::Bye, None));
    assert_eq!(worker.peers.len(), 2);
    assert!(
        worker
            .peers
            .keys()
            .any(|key| key.channel == 1 && key.process == "remote")
    );
    worker.discovered(1, protocol::discovery("remote", Type::Bye, None));
    assert_eq!(worker.peers.len(), 1);
    assert_eq!(
        worker.peers.keys().next().unwrap().process,
        "unrelated-process"
    );
    worker.discovered(0, protocol::discovery("unrelated-process", Type::Bye, None));
    assert!(worker.peers.is_empty());
}

#[test]
fn discovered_endpoints_only_connect_and_stay_alive_on_their_native_channel() {
    let (mut worker, _) = state(4);
    let mut service = worker.wire_publisher(
        "/service".into(),
        PubType::SrvPub(ServicePublisher {
            socket_id: worker.router_id.clone(),
            request_type: "req".into(),
            response_type: "rep".into(),
        }),
    );
    service.process_uuid = "remote".into();
    worker.discovered(0, protocol::advertisement("remote", service.clone()));
    let (request, _) = pending(Instant::now() + Duration::from_secs(5));
    worker.command(Command::Request(request));
    worker.send_requests();
    assert!(worker.connected.is_empty());
    worker.discovered(1, protocol::advertisement("remote", service.clone()));
    worker.send_requests();
    assert_eq!(worker.connected.len(), 1);
    let mut publication = worker.wire_publisher(
        "/message".into(),
        PubType::MsgPub(MessagePublisher {
            msg_type: "type".into(),
            ..Default::default()
        }),
    );
    publication.process_uuid = "remote".into();
    worker.discovered(1, protocol::advertisement("remote", publication.clone()));
    let (tx, _) = mpsc::sync_channel(1);
    worker.command(Command::Subscribe(
        "/message".into(),
        "type".into(),
        tx.into(),
        Arc::new(AtomicBool::new(true)),
        SubscribeOptions::default(),
        None,
    ));
    assert!(worker.subscriptions_connected.is_empty());
    worker.discovered(0, protocol::advertisement("remote", publication.clone()));
    assert_eq!(worker.subscriptions_connected.len(), 1);
    worker.discovered(
        1,
        protocol::discovery(
            "remote",
            Type::Unadvertise,
            Some(DiscContents::Pub(service)),
        ),
    );
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::Unadvertise,
            Some(DiscContents::Pub(publication)),
        ),
    );
    worker.cleanup();
    assert_eq!(worker.peers.len(), 2); // Wrong-channel metadata cannot retain either native connection.
    assert!(worker.connected.is_empty());
    assert!(worker.subscriptions_connected.is_empty());
}

#[test]
fn withdrawal_preserves_other_process_node_and_topic_identities() {
    let (mut worker, _) = state(4);
    let mut publication = worker.wire_publisher(
        "/original".into(),
        PubType::MsgPub(MessagePublisher::default()),
    );
    publication.process_uuid = "remote".into();
    publication.node_uuid = "original-node".into();
    for p in [
        publication.clone(),
        WirePublisher {
            process_uuid: "unrelated-process".into(),
            ..publication.clone()
        },
        WirePublisher {
            node_uuid: "unrelated-node".into(),
            ..publication.clone()
        },
        WirePublisher {
            topic: "/unrelated-topic".into(),
            ..publication.clone()
        },
    ] {
        worker.discovered(0, protocol::advertisement(&p.process_uuid.clone(), p));
    }
    assert_eq!(worker.peers.len(), 4);
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::Unadvertise,
            Some(DiscContents::Pub(publication)),
        ),
    );
    assert_eq!(worker.peers.len(), 3);
    assert!(worker.peers.keys().all(|key| key.process != "remote"
        || key.node != "original-node"
        || key.topic != "/original"));
}

#[test]
fn unsupported_discovery_opcode_refreshes_activity_without_admitting_a_publisher() {
    let (mut worker, _) = state(2);
    let mut p = worker.wire_publisher(
        "/known".into(),
        PubType::MsgPub(MessagePublisher::default()),
    );
    p.process_uuid = "remote".into();
    worker.discovered(0, protocol::advertisement("remote", p.clone()));
    p.topic = "/must-not-admit".into();
    let mut unsupported = protocol::advertisement("remote", p);
    unsupported.r#type = i32::MAX;
    for channel in 0..2 {
        let old = Instant::now() - Duration::from_secs(5);
        worker.peers.values_mut().next().unwrap().1 = old;
        worker.discovered(channel, unsupported.clone());
        assert_eq!(worker.peers.len(), 1);
        let (known, stamp) = worker.peers.values().next().unwrap();
        assert_eq!(known.topic, "/known");
        assert_eq!(*stamp > old, channel == 0);
    }
}

#[test]
fn duplicate_or_capacity_rejected_advertisements_cannot_register_new_metadata() {
    let (mut worker, _) = state(1);
    let (tx, _) = mpsc::sync_channel(1);
    worker.command(Command::Subscribe(
        "/topic".into(),
        "correct".into(),
        tx.into(),
        Arc::new(AtomicBool::new(true)),
        SubscribeOptions::default(),
        None,
    ));
    let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_millis(30)))
        .unwrap();
    worker.config.multicast = Ipv4Addr::LOCALHOST;
    worker.config.message_port = receiver.local_addr().unwrap().port();
    let mut first = worker.wire_publisher(
        "/topic".into(),
        PubType::MsgPub(MessagePublisher {
            msg_type: "wrong".into(),
            ..Default::default()
        }),
    );
    first.process_uuid = "remote".into();
    worker.discovered(0, protocol::advertisement("remote", first.clone()));
    let mut corrected = first.clone();
    corrected.pub_type = Some(PubType::MsgPub(MessagePublisher {
        msg_type: "correct".into(),
        ..Default::default()
    }));
    worker.discovered(0, protocol::advertisement("remote", corrected.clone()));
    assert!(worker.subscriptions_connected.is_empty());
    assert_eq!(worker.peers.values().next().unwrap().0, first);
    let mut buffer = vec![0; protocol::MAX_DATAGRAM];
    assert!(receiver.recv(&mut buffer).is_err());
    let mut rejected = corrected.clone();
    rejected.node_uuid = "capacity-rejected".into();
    worker.discovered(0, protocol::advertisement("remote", rejected));
    assert_eq!(worker.peers.len(), 1);
    assert!(worker.subscriptions_connected.is_empty());
    assert!(receiver.recv(&mut buffer).is_err());
    worker.discovered(
        0,
        protocol::discovery("remote", Type::Unadvertise, Some(DiscContents::Pub(first))),
    );
    worker.discovered(0, protocol::advertisement("remote", corrected));
    assert_eq!(worker.subscriptions_connected.len(), 1);
    let len = receiver.recv(&mut buffer).unwrap();
    let registration = protocol::decode(&buffer[..len]).unwrap();
    assert_eq!(registration.r#type, Type::NewConnection as i32);
    let Some(DiscContents::Pub(p)) = registration.disc_contents else {
        panic!("missing registration")
    };
    let Some(PubType::MsgPub(m)) = p.pub_type else {
        panic!("wrong publisher kind")
    };
    assert_eq!(m.msg_type, "correct");
    assert!(receiver.recv(&mut buffer).is_err());
}

#[test]
fn service_signature_snapshot_filters_lifetimes_channels_and_kinds() {
    let (mut worker, _) = state(16);
    let active = Arc::new(AtomicBool::new(true));
    let (requests, _) = mpsc::sync_channel(1);
    worker.command(Command::Serve(
        "/service".into(),
        "req".into(),
        "rep".into(),
        Scope::All,
        Arc::new(requests),
        active.clone(),
    ));
    let local = worker.services[0].publisher.clone();
    for (index, channel, topic, request, age) in [
        (0, 1, "/service", "req", 0), // Duplicate type pair, distinct provider.
        (1, 1, "/service", "other", 0),
        (2, 0, "/service", "message-channel", 0),
        (3, 1, "/other", "wrong-topic", 0),
        (4, 1, "/service", "expired", 4),
    ] {
        let mut publisher = local.clone();
        publisher.topic = topic.into();
        publisher.process_uuid = format!("remote-{index}");
        publisher.pub_type = Some(PubType::SrvPub(ServicePublisher {
            request_type: request.into(),
            response_type: "rep".into(),
            ..Default::default()
        }));
        worker.peers.insert(
            PeerKey {
                channel,
                process: publisher.process_uuid.clone(),
                node: publisher.node_uuid.clone(),
                topic: publisher.topic.clone(),
                address: publisher.address.clone(),
            },
            (publisher, Instant::now() - Duration::from_secs(age)),
        );
    }
    let mut wrong = local;
    wrong.process_uuid = "wrong-kind".into();
    wrong.pub_type = Some(PubType::MsgPub(MessagePublisher::default()));
    worker.discovered(1, protocol::advertisement("wrong-kind", wrong));
    let types = |request: &str| ServiceTypes {
        request_type: request.into(),
        response_type: "rep".into(),
    };
    assert_eq!(
        worker.service_signatures("/service"),
        [types("other"), types("req")]
    );
    assert_eq!(
        worker.service_provider_types("/service"),
        [types("other"), types("req"), types("req")]
    );
    active.store(false, Ordering::Release);
    // The duplicate remote signature still exists after local withdrawal.
    assert_eq!(
        worker.service_signatures("/service"),
        [types("other"), types("req")]
    );
    assert_eq!(
        worker.service_provider_types("/service"),
        [types("other"), types("req")]
    );
    worker.discovered(1, protocol::discovery("remote-0", Type::Bye, None));
    assert_eq!(worker.service_signatures("/service"), [types("other")]);
    worker.discovered(1, protocol::discovery("remote-1", Type::Bye, None));
    assert!(worker.service_signatures("/service").is_empty());
    assert!(worker.service_provider_types("/service").is_empty());
    let (reply, received) = mpsc::sync_channel(1);
    worker.command(Command::ServiceProviderTypes("/service".into(), reply));
    assert_eq!(received.recv().unwrap(), []);
    let (reply, received) = mpsc::sync_channel(1);
    drop(received);
    worker.command(Command::ServiceProviderTypes("/service".into(), reply));
    let (reply, received) = mpsc::sync_channel(1);
    worker.command(Command::ServiceSignatures("/service".into(), reply));
    assert_eq!(received.recv().unwrap(), []);
    let (reply, received) = mpsc::sync_channel(1);
    drop(received); // A timed out caller cannot block or retain a worker query.
    worker.command(Command::ServiceSignatures("/service".into(), reply));
}

#[test]
fn service_signature_coordination_is_bounded_and_explicit_types_bypass_it() {
    let (worker, commands) = state(1);
    let node = Node {
        sender: worker.sender.clone(),
        lifetime: Arc::new(WorkerLifetime {
            stop: worker.sender.stop.clone(),
            worker: Mutex::new(None),
        }),
        owner_dead: worker.owner_dead.clone(),
        local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
        config: Config {
            reply_timeout: Duration::from_millis(5),
            ..worker.config.clone()
        },
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: worker.sender.identity.get().unwrap().services.clone(),
        subscription_dispatcher: Mutex::new(Weak::new()),
    };
    assert_eq!(
        node.service_type_signatures("invalid service"),
        Err(Error::InvalidName)
    );
    assert_eq!(
        node.resolve_service_types("invalid service", "", "", Duration::ZERO),
        Err(Error::InvalidName)
    );
    assert_eq!(
        node.resolve_service_types("/service", "", "", Duration::MAX),
        Err(Error::InvalidConfig)
    );
    assert_eq!(
        node.service_type_signatures("/service"),
        Err(Error::Timeout)
    );
    assert_eq!(
        node.service_type_signatures("/service"),
        Err(Error::QueueFull)
    );
    drop(commands.recv().unwrap());
    let commands = thread::scope(|scope| {
        let responder = scope.spawn(move || {
            drop(commands.recv().unwrap());
            commands
        });
        assert_eq!(
            node.service_type_signatures("/service"),
            Err(Error::Shutdown)
        );
        responder.join().unwrap()
    });
    drop(commands);
    assert_eq!(
        node.service_type_signatures("/service"),
        Err(Error::Shutdown)
    );
    node.cancel();
    assert_eq!(
        node.service_type_signatures("/service"),
        Err(Error::Shutdown)
    );
    assert_eq!(
        node.resolve_service_types("/service", "", "", Duration::ZERO),
        Err(Error::Shutdown)
    );
    assert_eq!(
        node.resolve_service_types("invalid service", "custom.Req", "custom.Rep", Duration::MAX),
        Ok(ServiceTypeResolution::Resolved(ServiceTypes {
            request_type: "custom.Req".into(),
            response_type: "custom.Rep".into(),
        }))
    );
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert_eq!(node.statistics().queue_drops, 1);
}

#[test]
fn remapping_precedes_qualification_and_keeps_admission_ownership_bounded() {
    let (worker, commands) = state(1);
    let mut config = worker.config.clone();
    config.namespace = "ns".into();
    config.add_topic_remap("alias", "relative").unwrap();
    let node = Node {
        sender: worker.sender.clone(),
        lifetime: Arc::new(WorkerLifetime {
            stop: worker.sender.stop.clone(),
            worker: Mutex::new(None),
        }),
        owner_dead: worker.owner_dead.clone(),
        local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
        config,
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: worker.sender.identity.get().unwrap().services.clone(),
        subscription_dispatcher: Mutex::new(Weak::new()),
    };
    let publisher = node.advertise("alias", "type").unwrap();
    assert_eq!(
        publisher.topic,
        format!("@/{}@/ns/relative", node.config.partition)
    );
    assert!(matches!(
        node.advertise("relative", "type"),
        Err(Error::AlreadyAdvertised)
    ));
    drop(publisher);
    assert!(matches!(
        node.advertise("alias", "type"),
        Err(Error::QueueFull)
    ));
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    drop(commands.recv().unwrap());
    let publisher = node.advertise("alias", "type").unwrap();
    drop(publisher);
    drop(commands.recv().unwrap());
    node.cancel();
    assert!(matches!(
        node.advertise("alias", "type"),
        Err(Error::Shutdown)
    ));
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    let mut long = node.config.clone();
    long.topic_remaps.clear();
    long.add_topic_remap("alias", &"a".repeat(65535)).unwrap();
    let long = Node {
        sender: node.sender.clone(),
        lifetime: Arc::new(WorkerLifetime {
            stop: worker.sender.stop.clone(),
            worker: Mutex::new(None),
        }),
        owner_dead: worker.owner_dead.clone(),
        local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
        config: long,
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: worker.sender.identity.get().unwrap().services.clone(),
        subscription_dispatcher: Mutex::new(Weak::new()),
    };
    assert!(matches!(
        long.advertise("alias", "type"),
        Err(Error::InvalidName)
    ));
}

#[test]
fn remembered_subscribers_preserve_native_control_identity_and_lifecycle() {
    let (mut worker, _) = state(4);
    let active = Arc::new(AtomicBool::new(true));
    let (tx, _) = mpsc::sync_channel(1);
    worker.command(Command::Subscribe(
        "/topic".into(),
        "wrong".into(),
        tx.into(),
        active.clone(),
        SubscribeOptions::default(),
        None,
    ));
    assert!(!worker.has_subscribers("/topic", "T"));
    worker.subscriptions[0].type_name.clear();
    assert!(worker.has_subscribers("/topic", "T"));
    active.store(false, Ordering::Release);
    assert!(!worker.has_subscribers("/topic", "T"));
    worker.subscriptions[0].type_name = "T".into();
    active.store(true, Ordering::Release);
    assert!(worker.has_subscribers("/topic", "T"));
    active.store(false, Ordering::Release);
    let mut remote = worker.wire_publisher(
        "/topic".into(),
        PubType::MsgPub(MessagePublisher {
            msg_type: "T".into(),
            ctrl: "wrong".into(),
            ..Default::default()
        }),
    );
    remote.process_uuid = "remote".into();
    remote.node_uuid = "subscriber".into();
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::NewConnection,
            Some(DiscContents::Pub(remote.clone())),
        ),
    );
    assert!(worker.remote_subscribers.is_empty());
    let Some(PubType::MsgPub(m)) = &mut remote.pub_type else {
        unreachable!()
    };
    m.ctrl = worker.process.clone();
    for kind in [Type::SubscribersRep, Type::Advertise] {
        worker.discovered(
            0,
            protocol::discovery("remote", kind, Some(DiscContents::Pub(remote.clone()))),
        );
        assert!(!worker.has_subscribers("/topic", "T"));
    }
    worker.discovered(
        1,
        protocol::discovery(
            "remote",
            Type::NewConnection,
            Some(DiscContents::Pub(remote.clone())),
        ),
    );
    assert!(worker.remote_subscribers.is_empty());
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::NewConnection,
            Some(DiscContents::Pub(remote.clone())),
        ),
    );
    assert!(worker.has_subscribers("/topic", "T"));
    assert!(!worker.has_subscribers("/other", "T"));
    assert!(!worker.has_subscribers("/topic", "wrong"));
    let mut duplicate = remote.clone();
    let Some(PubType::MsgPub(m)) = &mut duplicate.pub_type else {
        unreachable!()
    };
    m.msg_type = "changed".into();
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::NewConnection,
            Some(DiscContents::Pub(duplicate)),
        ),
    );
    assert_eq!(worker.remote_subscribers.len(), 1);
    assert!(worker.has_subscribers("/topic", "T"));
    assert!(!worker.has_subscribers("/topic", "changed"));
    for (process, node, topic) in [
        ("other", "subscriber", "/topic"),
        ("remote", "other", "/topic"),
        ("remote", "subscriber", "/other"),
    ] {
        let mut other = remote.clone();
        other.process_uuid = process.into();
        other.node_uuid = node.into();
        other.topic = topic.into();
        worker.discovered(
            0,
            protocol::discovery(process, Type::NewConnection, Some(DiscContents::Pub(other))),
        );
    }
    assert_eq!(worker.remote_subscribers.len(), 4);
    worker.discovered(0, protocol::discovery("remote", Type::Bye, None));
    worker.cleanup();
    assert_eq!(worker.remote_subscribers.len(), 4);
    let mut wrong_end = remote.clone();
    let Some(PubType::MsgPub(m)) = &mut wrong_end.pub_type else {
        unreachable!()
    };
    m.ctrl = "wrong-destination".into();
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::EndConnection,
            Some(DiscContents::Pub(wrong_end)),
        ),
    );
    worker.discovered(
        1,
        protocol::discovery(
            "remote",
            Type::EndConnection,
            Some(DiscContents::Pub(remote.clone())),
        ),
    );
    assert_eq!(worker.remote_subscribers.len(), 4);
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::Unadvertise,
            Some(DiscContents::Pub(remote.clone())),
        ),
    );
    assert_eq!(worker.remote_subscribers.len(), 3);
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::NewConnection,
            Some(DiscContents::Pub(remote.clone())),
        ),
    );
    let (tx, rx) = mpsc::sync_channel(1);
    worker.command(Command::HasConnections("/topic".into(), "T".into(), tx));
    assert!(rx.recv().unwrap());
    let (tx, rx) = mpsc::sync_channel(1);
    drop(rx);
    worker.command(Command::HasConnections("/topic".into(), "T".into(), tx));
    remote.pub_type = None;
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::EndConnection,
            Some(DiscContents::Pub(remote)),
        ),
    );
    assert_eq!(worker.remote_subscribers.len(), 3);
}

#[test]
fn subscriber_registration_bounds_and_native_default_projection() {
    let (mut worker, _) = state(1);
    let mut remote = worker.wire_publisher(
        "/topic".into(),
        PubType::MsgPub(MessagePublisher {
            msg_type: "google.protobuf.Message".into(),
            ctrl: worker.process.clone(),
            ..Default::default()
        }),
    );
    remote.process_uuid.clear();
    remote.node_uuid.clear();
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::NewConnection,
            Some(DiscContents::Pub(remote.clone())),
        ),
    );
    assert!(worker.has_subscribers("/topic", "any.type"));
    let mut overflow = remote.clone();
    overflow.address.push_str("-other");
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::NewConnection,
            Some(DiscContents::Pub(overflow)),
        ),
    );
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::NewConnection,
            Some(DiscContents::Pub(remote.clone())),
        ),
    );
    assert_eq!(worker.remote_subscribers.len(), 1);
    assert_eq!(worker.sender.counters.snapshot().queue_drops, 1);
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::Unadvertise,
            Some(DiscContents::Pub(remote.clone())),
        ),
    );
    assert_eq!(
        worker.remote_subscribers.len(),
        1,
        "empty node is a process-only native disconnection"
    );
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::EndConnection,
            Some(DiscContents::Sub(Subscriber {
                topic: "/topic".into(),
            })),
        ),
    );
    assert!(worker.remote_subscribers.is_empty());
    remote.topic.clear();
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::NewConnection,
            Some(DiscContents::Pub(remote.clone())),
        ),
    );
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::Unadvertise,
            Some(DiscContents::Pub(remote.clone())),
        ),
    );
    assert_eq!(worker.remote_subscribers.len(), 1);
    worker.discovered(0, protocol::discovery("remote", Type::EndConnection, None));
    assert!(worker.remote_subscribers.is_empty());
    remote.topic = "/topic".into();
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::NewConnection,
            Some(DiscContents::Pub(remote.clone())),
        ),
    );
    remote.pub_type = Some(PubType::SrvPub(ServicePublisher::default()));
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::EndConnection,
            Some(DiscContents::Pub(remote.clone())),
        ),
    );
    assert!(worker.remote_subscribers.is_empty());
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::NewConnection,
            Some(DiscContents::Pub(remote)),
        ),
    );
    assert!(worker.remote_subscribers.is_empty());
}

#[test]
fn publisher_connection_coordination_has_no_unbounded_pending_state() {
    let (mut worker, commands) = state(1);
    let node = Node {
        sender: worker.sender.clone(),
        lifetime: Arc::new(WorkerLifetime {
            stop: worker.sender.stop.clone(),
            worker: Mutex::new(None),
        }),
        owner_dead: worker.owner_dead.clone(),
        local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
        config: Config {
            reply_timeout: Duration::from_millis(5),
            ..worker.config.clone()
        },
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: worker.sender.identity.get().unwrap().services.clone(),
        subscription_dispatcher: Mutex::new(Weak::new()),
    };
    let publisher = node.advertise("/topic", "T").unwrap();
    assert_eq!(publisher.connection_snapshot(), Ok(false));
    assert_eq!(publisher.has_connections(), Err(Error::QueueFull));
    assert_eq!(publisher.connection_snapshot(), Ok(false));
    worker.command(commands.recv().unwrap());
    assert_eq!(publisher.has_connections(), Err(Error::Timeout));
    assert_eq!(publisher.has_connections(), Err(Error::QueueFull));
    drop(commands.recv().unwrap());
    let commands = thread::scope(|scope| {
        let responder = scope.spawn(move || {
            drop(commands.recv().unwrap());
            commands
        });
        assert_eq!(publisher.has_connections(), Err(Error::Shutdown));
        responder.join().unwrap()
    });
    drop(commands);
    assert_eq!(publisher.has_connections(), Err(Error::Shutdown));
    node.cancel();
    assert_eq!(publisher.connection_snapshot(), Err(Error::Shutdown));
    assert_eq!(publisher.has_connections(), Err(Error::Shutdown));
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 1);
    drop(publisher);
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert_eq!(node.statistics().queue_drops, 2);
}

#[test]
fn publisher_snapshot_uses_remembered_remote_registration_controls() {
    let (mut worker, commands) = state(8);
    let node = Node {
        sender: worker.sender.clone(),
        lifetime: Arc::new(WorkerLifetime {
            stop: worker.sender.stop.clone(),
            worker: Mutex::new(None),
        }),
        owner_dead: worker.owner_dead.clone(),
        local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
        config: worker.config.clone(),
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: worker.sender.identity.get().unwrap().services.clone(),
        subscription_dispatcher: Mutex::new(Weak::new()),
    };
    let publisher = node.advertise("/wire", "T").unwrap();
    assert_eq!(publisher.connection_snapshot(), Ok(false));
    worker.command(commands.recv().unwrap());

    let remote = |node_id: &str, type_name: &str, ctrl: &str| {
        let mut p = worker.subscriber_publisher(&publisher.topic, type_name);
        p.process_uuid = "remote".into();
        p.node_uuid = node_id.into();
        if let Some(PubType::MsgPub(info)) = &mut p.pub_type {
            info.ctrl = ctrl.into();
        }
        p
    };
    let wrong_ctrl = remote("wrong-ctrl", "T", "other");
    let wrong_type = remote("wrong-type", "other.T", &worker.process);
    let mut wrong_partition = remote("wrong-partition", "T", &worker.process);
    wrong_partition.topic = "@other@/wire".into();
    let matching = remote("matching", "T", &worker.process);
    let wildcard = remote("wildcard", "google.protobuf.Message", &worker.process);
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::NewConnection,
            Some(DiscContents::Pub(wrong_ctrl)),
        ),
    );
    assert_eq!(publisher.connection_snapshot(), Ok(false));
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::NewConnection,
            Some(DiscContents::Pub(wrong_type)),
        ),
    );
    assert_eq!(publisher.connection_snapshot(), Ok(false));
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::NewConnection,
            Some(DiscContents::Pub(wrong_partition)),
        ),
    );
    assert_eq!(publisher.connection_snapshot(), Ok(false));
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::NewConnection,
            Some(DiscContents::Pub(matching.clone())),
        ),
    );
    assert_eq!(publisher.connection_snapshot(), Ok(true));
    worker.discovered(0, protocol::discovery("remote", Type::Bye, None));
    assert_eq!(publisher.connection_snapshot(), Ok(true));
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::EndConnection,
            Some(DiscContents::Pub(matching.clone())),
        ),
    );
    assert_eq!(publisher.connection_snapshot(), Ok(false));

    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::NewConnection,
            Some(DiscContents::Pub(wildcard.clone())),
        ),
    );
    assert_eq!(publisher.connection_snapshot(), Ok(true));
    worker.discovered(
        0,
        protocol::discovery(
            "remote",
            Type::Unadvertise,
            Some(DiscContents::Pub(wildcard)),
        ),
    );
    assert_eq!(publisher.connection_snapshot(), Ok(false));
    drop(node);
    assert_eq!(publisher.connection_snapshot(), Ok(false));
}

#[test]
fn publisher_snapshot_sees_admitted_local_interest_before_worker_commands() {
    let (mut worker, commands) = state(4);
    let node = Node {
        sender: worker.sender.clone(),
        lifetime: Arc::new(WorkerLifetime {
            stop: worker.sender.stop.clone(),
            worker: Mutex::new(None),
        }),
        owner_dead: worker.owner_dead.clone(),
        local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
        config: worker.config.clone(),
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: worker.sender.identity.get().unwrap().services.clone(),
        subscription_dispatcher: Mutex::new(Weak::new()),
    };
    let publisher = node.advertise("/topic", "T").unwrap();
    let subscription = node.subscribe("/topic", "T").unwrap();
    assert_eq!(publisher.connection_snapshot(), Ok(true));
    worker.command(commands.recv().unwrap());
    assert_eq!(publisher.connection_snapshot(), Ok(true));
    drop(subscription);
    assert_eq!(publisher.connection_snapshot(), Ok(false));
    worker.command(commands.recv().unwrap());
    assert_eq!(publisher.connection_snapshot(), Ok(false));
}

#[test]
fn graceful_shutdown_sends_one_end_per_known_topic_before_bye() {
    let (mut worker, commands) = state(4);
    let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_millis(30)))
        .unwrap();
    worker.config.multicast = Ipv4Addr::LOCALHOST;
    worker.config.message_port = receiver.local_addr().unwrap().port();
    let mut retire = Vec::new();
    for (topic, keep) in [("/one", true), ("/one", false), ("/two", false)] {
        let (tx, _) = mpsc::sync_channel(1);
        let active = Arc::new(AtomicBool::new(true));
        worker.command(Command::Subscribe(
            topic.into(),
            "T".into(),
            tx.into(),
            active.clone(),
            SubscribeOptions::default(),
            None,
        ));
        if !keep {
            retire.push(active);
        }
    }
    // All topics were admitted; retire two leases immediately before shutdown.
    for active in retire {
        active.store(false, Ordering::Release);
    }
    let mut buffer = vec![0; protocol::MAX_DATAGRAM];
    while receiver.recv(&mut buffer).is_ok() {}
    let process = worker.process.clone();
    let stop = worker.sender.stop.clone();
    stop.store(true, Ordering::Release);
    worker.run(commands, stop);
    let mut trace = Vec::new();
    while let Ok(len) = receiver.recv(&mut buffer) {
        let message = protocol::decode(&buffer[..len]).unwrap();
        assert_eq!(message.process_uuid, process);
        if let Some(DiscContents::Pub(p)) = message.disc_contents {
            assert_eq!(p.process_uuid, process);
            assert!(matches!(&p.pub_type,Some(PubType::MsgPub(m)) if m.ctrl.is_empty()));
            trace.push((message.r#type, p.topic));
        } else {
            trace.push((message.r#type, String::new()));
        }
    }
    assert_eq!(
        trace,
        [
            (Type::EndConnection as i32, "/one".into()),
            (Type::EndConnection as i32, "/two".into()),
            (Type::Bye as i32, String::new())
        ]
    );
}

#[test]
fn final_publisher_retirement_sends_unadvertise_before_bye() {
    let (mut worker, commands) = state(1);
    let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_millis(30)))
        .unwrap();
    worker.config.multicast = Ipv4Addr::LOCALHOST;
    worker.config.message_port = receiver.local_addr().unwrap().port();
    let active = Arc::new(AtomicBool::new(true));
    worker.command(Command::Advertise(
        "/survivor".into(),
        "gz.msgs.Int32".into(),
        AdvertiseOptions::default(),
        active.clone(),
        Arc::new(AtomicU8::new(0)),
    ));
    let mut buffer = vec![0; protocol::MAX_DATAGRAM];
    while receiver.recv(&mut buffer).is_ok() {}
    active.store(false, Ordering::Release);
    let stop = worker.sender.stop.clone();
    stop.store(true, Ordering::Release);
    worker.run(commands, stop);
    let mut types = Vec::new();
    while let Ok(len) = receiver.recv(&mut buffer) {
        let message = protocol::decode(&buffer[..len]).unwrap();
        if let Some(DiscContents::Pub(publication)) = message.disc_contents {
            assert_eq!(publication.topic, "/survivor");
        }
        types.push(message.r#type);
    }
    assert_eq!(types, [Type::Unadvertise as i32, Type::Bye as i32]);
}

#[test]
fn local_topic_inventory_tracks_live_leases_bounds_and_failed_admission() {
    let (mut worker, commands) = state(3);
    let mut config = worker.config.clone();
    config.namespace = "ns".into();
    config.add_topic_remap("alias", "mapped").unwrap();
    let node = Node {
        sender: worker.sender.clone(),
        lifetime: Arc::new(WorkerLifetime {
            stop: worker.sender.stop.clone(),
            worker: Mutex::new(None),
        }),
        owner_dead: worker.owner_dead.clone(),
        local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
        config,
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: worker.sender.identity.get().unwrap().services.clone(),
        subscription_dispatcher: Mutex::new(Weak::new()),
    };
    assert!(node.advertised_topics().unwrap().is_empty());
    assert!(node.subscribed_topics().unwrap().is_empty());
    let publication = node.advertise("alias", "T").unwrap();
    assert_eq!(node.advertised_topics().unwrap(), ["/ns/mapped"]);
    worker.command(commands.recv().unwrap());
    let first = node.subscribe("alias", "T").unwrap();
    worker.command(commands.recv().unwrap());
    let second = node.subscribe("/ns/mapped", "different").unwrap();
    worker.command(commands.recv().unwrap());
    assert_eq!(node.subscribed_topics().unwrap(), ["/ns/mapped"]);
    assert!(matches!(
        node.subscribe("/full", "T"),
        Err(Error::QueueFull)
    ));
    drop(first);
    assert_eq!(node.subscribed_topics().unwrap(), ["/ns/mapped"]);
    drop(second);
    assert!(node.subscribed_topics().unwrap().is_empty());
    drop(publication);
    assert!(node.advertised_topics().unwrap().is_empty());
    for _ in 0..100 {
        let subscription = node.subscribe("/stream", "T").unwrap();
        worker.command(commands.recv().unwrap());
        assert!(worker.subscriptions.len() <= node.config.capacity);
        assert!(node.subscriptions.lock().unwrap().len() <= node.config.capacity);
        drop(subscription);
    }
    worker.cleanup();
    assert!(worker.subscriptions.is_empty());
    let retired = node.subscribe("/retired", "T").unwrap();
    drop(retired);
    worker.command(commands.recv().unwrap());
    assert!(worker.subscriptions.is_empty());
    assert!(node.subscribed_topics().unwrap().is_empty());
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    let mut publications = Vec::new();
    for topic in ["/z", "/a", "/middle"] {
        publications.push(node.advertise(topic, "T").unwrap());
        drop(commands.recv().unwrap());
    }
    assert_eq!(node.advertised_topics().unwrap(), ["/a", "/middle", "/z"]);
    node.cancel();
    assert_eq!(node.advertised_topics(), Err(Error::Shutdown));
    assert_eq!(node.subscribed_topics(), Err(Error::Shutdown));
    drop(publications);
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
}

#[test]
fn subscription_admission_withdraws_old_before_new_control_registration() {
    let (mut worker, _) = state(4);
    let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_millis(30)))
        .unwrap();
    worker.config.multicast = Ipv4Addr::LOCALHOST;
    worker.config.message_port = receiver.local_addr().unwrap().port();
    let old = Arc::new(AtomicBool::new(true));
    let (tx, _) = mpsc::sync_channel(1);
    worker.command(Command::Subscribe(
        "/topic".into(),
        "old".into(),
        tx.into(),
        old.clone(),
        SubscribeOptions::default(),
        None,
    ));
    for (index, type_name) in [(0, "old"), (1, "new")] {
        let mut p = worker.wire_publisher(
            "/topic".into(),
            PubType::MsgPub(MessagePublisher {
                msg_type: type_name.into(),
                ..Default::default()
            }),
        );
        p.process_uuid = format!("remote-{index}");
        p.address = format!("tcp://127.0.0.1:{}", 45000 + index);
        worker.discovered(0, protocol::advertisement(&p.process_uuid.clone(), p));
    }
    let mut buffer = vec![0; protocol::MAX_DATAGRAM];
    while receiver.recv(&mut buffer).is_ok() {}
    old.store(false, Ordering::Release);
    let (tx, _) = mpsc::sync_channel(1);
    worker.command(Command::Subscribe(
        "/topic".into(),
        "new".into(),
        tx.into(),
        Arc::new(AtomicBool::new(true)),
        SubscribeOptions::default(),
        None,
    ));
    let mut trace = Vec::new();
    while let Ok(len) = receiver.recv(&mut buffer) {
        let m = protocol::decode(&buffer[..len]).unwrap();
        if let Some(DiscContents::Pub(p)) = m.disc_contents {
            let Some(PubType::MsgPub(t)) = p.pub_type else {
                panic!("wrong kind")
            };
            trace.push((m.r#type, t.msg_type));
        }
    }
    assert_eq!(
        trace,
        [
            (Type::EndConnection as i32, "google.protobuf.Message".into()),
            (Type::NewConnection as i32, "new".into())
        ]
    );
    assert_eq!(worker.subscriptions.len(), 1);
}

#[test]
fn local_inventory_registry_poison_and_rollback_are_explicit() {
    let (worker, commands) = state(1);
    let node = Node {
        sender: worker.sender.clone(),
        lifetime: Arc::new(WorkerLifetime {
            stop: worker.sender.stop.clone(),
            worker: Mutex::new(None),
        }),
        owner_dead: worker.owner_dead.clone(),
        local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
        config: worker.config.clone(),
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: worker.sender.identity.get().unwrap().services.clone(),
        subscription_dispatcher: Mutex::new(Weak::new()),
    };
    let subscription = node.subscribe("/one", "T").unwrap();
    drop(subscription);
    assert!(matches!(node.subscribe("/two", "T"), Err(Error::QueueFull)));
    assert!(node.subscribed_topics().unwrap().is_empty());
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    drop(commands.recv().unwrap());
    thread::scope(|scope| {
        assert!(
            scope
                .spawn(|| {
                    let _guard = node.subscriptions.lock().unwrap();
                    panic!("poison")
                })
                .join()
                .is_err()
        );
    });
    assert!(
        matches!(node.subscribed_topics(),Err(Error::Native(m)) if m=="subscription registry poisoned")
    );
    assert!(
        matches!(node.subscribe("/two","T"),Err(Error::Native(m)) if m=="subscription registry poisoned")
    );
    assert!(
        matches!(node.unsubscribe("/two"),Err(Error::Native(m)) if m=="subscription registry poisoned")
    );
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    thread::scope(|scope| {
        assert!(
            scope
                .spawn(|| {
                    let _guard = node.topics.lock().unwrap();
                    panic!("poison")
                })
                .join()
                .is_err()
        );
    });
    assert!(
        matches!(node.advertised_topics(),Err(Error::Native(m)) if m=="publisher registry poisoned")
    );
}

#[test]
fn duplicate_receivers_share_one_socket_filter_until_final_retirement() {
    let (mut worker, _) = state(4);
    let context = zmq::Context::new();
    let publisher = context.socket(zmq::XPUB).unwrap();
    publisher.set_linger(0).unwrap();
    publisher.bind("tcp://127.0.0.1:*").unwrap();
    let endpoint = publisher.get_last_endpoint().unwrap().unwrap();
    worker.subscriber.connect(&endpoint).unwrap();
    let topic = "/filter_lifetime";
    let control = |enabled| [vec![u8::from(enabled)], topic.as_bytes().to_vec()].concat();
    let mut active = Vec::new();
    for descriptor in ["First", "Second"] {
        let flag = Arc::new(AtomicBool::new(true));
        let (tx, _) = mpsc::sync_channel(1);
        worker.command(Command::Subscribe(
            topic.into(),
            descriptor.into(),
            tx.into(),
            flag.clone(),
            SubscribeOptions::default(),
            None,
        ));
        active.push(flag);
    }
    assert_eq!(worker.subscriptions.len(), 2);
    assert_eq!(worker.subscriptions[0].type_name, "First");
    assert_eq!(worker.subscriptions[1].type_name, "Second");
    assert!(publisher.poll(zmq::POLLIN, 1000).unwrap() > 0);
    assert_eq!(publisher.recv_bytes(0).unwrap(), control(true));
    active[0].store(false, Ordering::Release);
    worker.cleanup();
    assert_eq!(worker.subscriptions.len(), 1);
    assert_eq!(worker.subscriptions[0].type_name, "Second");
    assert_eq!(publisher.poll(zmq::POLLIN, 100).unwrap(), 0);
    publisher
        .send_multipart([topic.as_bytes(), b"Second", b"still live"], 0)
        .unwrap();
    assert!(worker.subscriber.poll(zmq::POLLIN, 1000).unwrap() > 0);
    assert_eq!(
        worker.subscriber.recv_multipart(0).unwrap()[2],
        b"still live"
    );
    active[1].store(false, Ordering::Release);
    worker.cleanup();
    assert!(worker.subscriptions.is_empty());
    // XPUB acknowledges actual filter removal before the negative delivery check.
    assert!(publisher.poll(zmq::POLLIN, 1000).unwrap() > 0);
    assert_eq!(publisher.recv_bytes(0).unwrap(), control(false));
    publisher
        .send_multipart([topic.as_bytes(), b"Second", b"retired"], 0)
        .unwrap();
    assert_eq!(worker.subscriber.poll(zmq::POLLIN, 100).unwrap(), 0);
}

#[test]
fn provider_record_query_coordination_is_bounded_without_registration_permits() {
    let (worker, commands) = state(1);
    let node = Node {
        sender: worker.sender.clone(),
        lifetime: Arc::new(WorkerLifetime {
            stop: worker.sender.stop.clone(),
            worker: Mutex::new(None),
        }),
        owner_dead: worker.owner_dead.clone(),
        local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
        config: Config {
            reply_timeout: Duration::from_millis(5),
            ..worker.config.clone()
        },
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: worker.sender.identity.get().unwrap().services.clone(),
        subscription_dispatcher: Mutex::new(Weak::new()),
    };
    assert_eq!(
        node.service_provider_types("invalid service"),
        Err(Error::InvalidName)
    );
    assert_eq!(node.service_provider_types("/service"), Err(Error::Timeout));
    assert_eq!(
        node.service_provider_types("/service"),
        Err(Error::QueueFull)
    );
    assert!(matches!(
        commands.recv().unwrap(),
        Command::ServiceProviderTypes(_, _)
    ));
    let commands = thread::scope(|scope| {
        let responder = scope.spawn(move || {
            drop(commands.recv().unwrap());
            commands
        });
        assert_eq!(
            node.service_provider_types("/service"),
            Err(Error::Shutdown)
        );
        responder.join().unwrap()
    });
    drop(commands);
    assert_eq!(
        node.service_provider_types("/service"),
        Err(Error::Shutdown)
    );
    node.cancel();
    assert_eq!(
        node.service_provider_types("/service"),
        Err(Error::Shutdown)
    );
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert_eq!(node.statistics().queue_drops, 1);
}

#[test]
fn unsubscribe_retires_all_local_receivers_without_double_slot_release() {
    let mut config = Config {
        capacity: 16,
        ..Default::default()
    };
    config.add_topic_remap("/alias", "/canonical").unwrap();
    let node = Node::new(config).unwrap();
    let first = node.subscribe("/alias", "first").unwrap();
    let second = node.subscribe("/alias", "second").unwrap();
    let other = node.subscribe("/other", "first").unwrap();
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 3);
    assert_eq!(node.unsubscribe("invalid topic"), Err(Error::InvalidName));
    node.unsubscribe("/absent").unwrap();
    node.unsubscribe("/alias").unwrap();
    assert_eq!(node.subscribed_topics().unwrap(), ["/canonical", "/other"]);
    node.unsubscribe("/canonical").unwrap();
    assert_eq!(node.subscribed_topics().unwrap(), ["/other"]);
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 1);
    assert!(!first._lease.active.load(Ordering::Acquire));
    assert!(!second._lease.active.load(Ordering::Acquire));
    assert!(other._lease.active.load(Ordering::Acquire));
    node.unsubscribe("/canonical").unwrap();
    drop((first, second));
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 1);
    let replacement = node.subscribe("/alias", "first").unwrap();
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 2);
    node.unsubscribe("/canonical").unwrap();
    drop(replacement);
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 1);
    let dropped = node.subscribe("/dropped", "first").unwrap();
    drop(dropped);
    node.unsubscribe("/dropped").unwrap();
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 1);
    node.cancel();
    assert_eq!(node.unsubscribe("/other"), Err(Error::Shutdown));
    drop(other);
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
}

#[test]
fn unsubscribe_keeps_owned_buffers_and_retires_filters_before_recreation() {
    let (mut worker, commands) = state(8);
    let node = Node {
        sender: worker.sender.clone(),
        lifetime: Arc::new(WorkerLifetime {
            stop: worker.sender.stop.clone(),
            worker: Mutex::new(None),
        }),
        owner_dead: worker.owner_dead.clone(),
        local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
        config: worker.config.clone(),
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: worker.sender.identity.get().unwrap().services.clone(),
        subscription_dispatcher: Mutex::new(Weak::new()),
    };
    let context = zmq::Context::new();
    let publisher = context.socket(zmq::XPUB).unwrap();
    publisher.set_linger(0).unwrap();
    publisher.bind("tcp://127.0.0.1:*").unwrap();
    worker
        .subscriber
        .connect(&publisher.get_last_endpoint().unwrap().unwrap())
        .unwrap();
    let first = node.subscribe("/topic", "old").unwrap();
    worker.command(commands.recv().unwrap());
    let topic = worker.subscriptions[0].topic.clone();
    let control = |enabled| [vec![u8::from(enabled)], topic.as_bytes().to_vec()].concat();
    assert!(publisher.poll(zmq::POLLIN, 1000).unwrap() > 0);
    assert_eq!(publisher.recv_bytes(0).unwrap(), control(true));
    worker.deliver(
        &Message {
            topic: topic.clone(),
            type_name: "old".into(),
            data: vec![1],
        },
        true,
    );
    node.unsubscribe("/topic").unwrap();
    worker.deliver(
        &Message {
            topic: topic.clone(),
            type_name: "old".into(),
            data: vec![2],
        },
        true,
    );
    assert_eq!(first.recv_timeout(Duration::ZERO).unwrap().data, [1]);
    assert!(matches!(
        first.recv_timeout(Duration::ZERO),
        Err(Error::Timeout)
    ));
    worker.command(commands.recv().unwrap());
    worker.cleanup();
    assert!(publisher.poll(zmq::POLLIN, 1000).unwrap() > 0);
    assert_eq!(publisher.recv_bytes(0).unwrap(), control(false));
    assert!(matches!(
        first.recv_timeout(Duration::ZERO),
        Err(Error::Shutdown)
    ));
    publisher
        .send_multipart([topic.as_bytes(), b"old", b"retired"], 0)
        .unwrap();
    assert_eq!(worker.subscriber.poll(zmq::POLLIN, 100).unwrap(), 0);
    let pending = node.subscribe("/topic", "pending").unwrap();
    node.unsubscribe("/topic").unwrap();
    let replacement = node.subscribe("/topic", "new").unwrap();
    worker.command(commands.recv().unwrap());
    worker.command(commands.recv().unwrap());
    worker.command(commands.recv().unwrap());
    assert_eq!(worker.subscriptions.len(), 1);
    assert_eq!(worker.subscriptions[0].type_name, "new");
    assert!(matches!(
        pending.recv_timeout(Duration::ZERO),
        Err(Error::Shutdown)
    ));
    assert!(publisher.poll(zmq::POLLIN, 1000).unwrap() > 0);
    assert_eq!(publisher.recv_bytes(0).unwrap(), control(true));
    worker.deliver(
        &Message {
            topic: topic.clone(),
            type_name: "new".into(),
            data: vec![3],
        },
        true,
    );
    assert_eq!(replacement.recv_timeout(Duration::ZERO).unwrap().data, [3]);
    node.unsubscribe("/topic").unwrap();
    worker.command(commands.recv().unwrap());
    worker.cleanup();
    assert!(publisher.poll(zmq::POLLIN, 1000).unwrap() > 0);
    assert_eq!(publisher.recv_bytes(0).unwrap(), control(false));
    drop((first, pending, replacement));
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
}

#[test]
fn unsubscribe_admission_failure_preserves_leases_and_absent_calls_refresh_end() {
    let (mut worker, commands) = state(1);
    let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_millis(30)))
        .unwrap();
    worker.config.multicast = Ipv4Addr::LOCALHOST;
    worker.config.message_port = receiver.local_addr().unwrap().port();
    let node = Node {
        sender: worker.sender.clone(),
        lifetime: Arc::new(WorkerLifetime {
            stop: worker.sender.stop.clone(),
            worker: Mutex::new(None),
        }),
        owner_dead: worker.owner_dead.clone(),
        local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
        config: worker.config.clone(),
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: worker.sender.identity.get().unwrap().services.clone(),
        subscription_dispatcher: Mutex::new(Weak::new()),
    };
    let subscription = node.subscribe("/topic", "T").unwrap();
    assert_eq!(node.unsubscribe("/topic"), Err(Error::QueueFull));
    assert!(subscription._lease.active.load(Ordering::Acquire));
    assert_eq!(node.subscribed_topics().unwrap(), ["/topic"]);
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 1);
    worker.command(commands.recv().unwrap());
    let mut buffer = vec![0; protocol::MAX_DATAGRAM];
    while receiver.recv(&mut buffer).is_ok() {}
    node.unsubscribe("/topic").unwrap();
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    worker.command(commands.recv().unwrap());
    assert!(worker.subscriptions.is_empty());
    while receiver.recv(&mut buffer).is_ok() {}
    for name in ["/topic", "/absent"] {
        node.unsubscribe(name).unwrap();
        worker.command(commands.recv().unwrap());
        let len = receiver.recv(&mut buffer).unwrap();
        let message = protocol::decode(&buffer[..len]).unwrap();
        assert_eq!(message.r#type, Type::EndConnection as i32);
        let Some(DiscContents::Pub(p)) = message.disc_contents else {
            panic!("missing END record")
        };
        assert_eq!(p.topic, node.topic(name).unwrap());
        let Some(PubType::MsgPub(metadata)) = p.pub_type else {
            panic!("missing generic metadata")
        };
        assert!(metadata.ctrl.is_empty());
        assert_eq!(metadata.msg_type, "google.protobuf.Message");
        assert!(receiver.recv(&mut buffer).is_err());
    }
    drop(subscription);
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
}

#[test]
fn typed_publication_preserves_binding_size_queue_and_shutdown_errors() {
    use gz_msgs::prost::Message as _;
    use gz_msgs::{int32::Int32, vector3d::Vector3d};
    let (mut worker, commands) = state(1);
    let node = Node {
        sender: worker.sender.clone(),
        lifetime: Arc::new(WorkerLifetime {
            stop: worker.sender.stop.clone(),
            worker: Mutex::new(None),
        }),
        owner_dead: worker.owner_dead.clone(),
        local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
        config: worker.config.clone(),
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: worker.sender.identity.get().unwrap().services.clone(),
        subscription_dispatcher: Mutex::new(Weak::new()),
    };
    let publisher = node.advertise_typed::<Int32>("/typed").unwrap();
    let wrong = Vector3d {
        x: 1.,
        y: 2.,
        z: 3.,
        ..Default::default()
    };
    let right = Int32 {
        data: 1,
        ..Default::default()
    };
    assert_eq!(publisher.publish_typed(&wrong), Err(Error::InvalidFrame));
    assert_eq!(node.statistics().queue_drops, 0);
    assert_eq!(publisher.publish_typed(&right), Err(Error::QueueFull));
    worker.command(commands.recv().unwrap());
    publisher.publish_typed(&right).unwrap();
    let Command::Publish(_, name, bytes, _) = commands.recv().unwrap() else {
        panic!("missing typed publication")
    };
    assert_eq!(name, "gz.msgs.Int32");
    assert_eq!(Int32::decode(bytes.as_slice()).unwrap().data, 1);
    node.cancel();
    assert_eq!(publisher.publish_typed(&right), Err(Error::Shutdown));
    let (worker, _commands) = state(1);
    let node = Node {
        sender: worker.sender.clone(),
        lifetime: Arc::new(WorkerLifetime {
            stop: worker.sender.stop.clone(),
            worker: Mutex::new(None),
        }),
        owner_dead: worker.owner_dead.clone(),
        local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
        config: Config {
            max_message_bytes: 1,
            ..worker.config.clone()
        },
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: worker.sender.identity.get().unwrap().services.clone(),
        subscription_dispatcher: Mutex::new(Weak::new()),
    };
    let publisher = node.advertise_typed::<Int32>("/size").unwrap();
    assert_eq!(publisher.publish_typed(&right), Err(Error::InvalidFrame));
    assert_eq!(node.statistics().queue_drops, 0);
}

#[test]
fn message_metadata_preserves_opaque_bytes_and_outlives_runtime() {
    let payload = vec![0, 255, 128, 18, 0];
    let received = {
        let node = Node::new(Config {
            partition: "/metadata/".into(),
            namespace: "/space/".into(),
            ..Config::default()
        })
        .unwrap();
        let publisher = node.advertise("topic", "opaque.Type").unwrap();
        let old = node.subscribe("topic", "opaque.Type").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !publisher.has_connections().unwrap() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
        publisher.publish(payload.clone()).unwrap();
        let received = old.recv_with_info_timeout(Duration::from_secs(1)).unwrap();
        node.unsubscribe("topic").unwrap();
        let replacement = node.subscribe("topic", "opaque.Type").unwrap();
        publisher.publish(vec![2]).unwrap();
        let next = replacement
            .recv_with_info_timeout(Duration::from_secs(1))
            .unwrap();
        assert_eq!(next.data, [2]);
        assert!(next.info().intra_process);
        assert!(matches!(
            old.recv_with_info_timeout(Duration::ZERO),
            Err(Error::Shutdown)
        ));
        publisher.publish(vec![3]).unwrap();
        assert_eq!(
            replacement
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .data,
            [3]
        );
        received
    };
    let expected = MessageInfo {
        topic: "/space/topic",
        partition: "/metadata",
        type_name: "opaque.Type",
        intra_process: true,
    };
    assert_eq!(received.info(), expected);
    assert_eq!(received.clone().info(), expected);
    assert_eq!(received.topic_path(), expected.topic);
    let pointer = received.data.as_ptr();
    let raw = received.into_message();
    assert_eq!(raw.data.as_ptr(), pointer);
    assert_eq!(
        raw,
        Message {
            topic: "@/metadata@/space/topic".into(),
            type_name: "opaque.Type".into(),
            data: payload
        }
    );
    let empty = ReceivedMessage {
        message: Message {
            topic: String::new(),
            type_name: String::new(),
            data: vec![],
        },
        intra_process: false,
    };
    assert_eq!(
        empty.info(),
        MessageInfo {
            topic: "",
            partition: "",
            type_name: "",
            intra_process: false
        }
    );
}

#[test]
fn wire_metadata_is_not_inferred_from_a_local_looking_address() {
    let (mut worker, _) = state(4);
    let context = zmq::Context::new();
    let publisher = context.socket(zmq::XPUB).unwrap();
    publisher.set_linger(0).unwrap();
    publisher.bind("tcp://127.0.0.1:*").unwrap();
    worker
        .subscriber
        .connect(&publisher.get_last_endpoint().unwrap().unwrap())
        .unwrap();
    let topic = qualified_name(&worker.config.partition, "space", "topic").unwrap();
    let (tx, rx) = mpsc::sync_channel(1);
    worker.command(Command::Subscribe(
        topic.clone(),
        "opaque.Type".into(),
        tx.into(),
        Arc::new(AtomicBool::new(true)),
        SubscribeOptions::default(),
        None,
    ));
    assert!(publisher.poll(zmq::POLLIN, 1000).unwrap() > 0);
    assert_eq!(
        publisher.recv_bytes(0).unwrap(),
        [vec![1], topic.as_bytes().to_vec()].concat()
    );
    publisher
        .send_multipart([topic.as_bytes(), &[255], &[1], b"opaque.Type"], 0)
        .unwrap();
    assert!(worker.subscriber.poll(zmq::POLLIN, 1000).unwrap() > 0);
    worker.receive_publications();
    assert_eq!(rx.try_recv(), Err(mpsc::TryRecvError::Empty));
    assert_eq!(worker.sender.counters.snapshot().malformed_frames, 1);
    let payload = [0, 255, 0, 128];
    publisher
        .send_multipart(
            [
                topic.as_bytes(),
                worker.address.as_bytes(),
                &payload,
                b"opaque.Type",
            ],
            0,
        )
        .unwrap();
    assert!(worker.subscriber.poll(zmq::POLLIN, 1000).unwrap() > 0);
    worker.receive_publications();
    let received = rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(received.data, payload);
    assert_eq!(
        received.info(),
        MessageInfo {
            topic: "/space/topic",
            partition: &format!("/{}", worker.config.partition),
            type_name: "opaque.Type",
            intra_process: false
        }
    );
    assert_eq!(worker.sender.counters.snapshot().malformed_frames, 1);
}

#[test]
fn subscription_clock_failures_are_recipient_errors_and_suppression_is_not_queue_loss() {
    let counters = Arc::new(Counters::default());
    assert!(SubscriptionThrottle::new(u64::MAX, counters.clone()).is_none());
    let throttle = SubscriptionThrottle::new(1, counters.clone()).unwrap();
    assert!(!throttle.admission(Err(Error::Native("injected subscription clock".into()))));
    assert!(!throttle.admission(Ok(false)));
    assert!(throttle.admission(Ok(true)));
    let snapshot = counters.snapshot();
    assert_eq!(snapshot.subscription_clock_errors, 1);
    assert_eq!(snapshot.queue_drops, 0);
    assert_eq!(snapshot.routing_errors, 0);
    assert_eq!(snapshot.send_failures, 0);
    assert_eq!(*throttle.limiter.last.lock().unwrap(), Duration::ZERO);
}

#[test]
fn subscription_filters_precede_one_shared_wire_and_local_admission_clock() {
    let (mut worker, _) = state(4);
    let (tx, rx) = mpsc::sync_channel(2);
    let mut options = SubscribeOptions::default();
    options.set_msgs_per_sec(1);
    options.set_ignore_local_messages(true);
    let throttle = SubscriptionThrottle::new(1, worker.sender.counters.clone()).unwrap();
    worker.command(Command::Subscribe(
        "topic".into(),
        "type".into(),
        tx.into(),
        Arc::new(AtomicBool::new(true)),
        options,
        Some(throttle.clone()),
    ));
    let mut message = Message {
        topic: "other".into(),
        type_name: "type".into(),
        data: vec![7],
    };
    worker.deliver(&message, false);
    message.topic = "topic".into();
    message.type_name = "wrong".into();
    worker.deliver(&message, false);
    message.type_name = "type".into();
    worker.deliver(&message, true);
    assert_eq!(*throttle.limiter.last.lock().unwrap(), Duration::ZERO);
    assert!(rx.try_recv().is_err());
    worker.deliver(&message, false);
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(1)).unwrap().message,
        message
    );
    assert!(*throttle.limiter.last.lock().unwrap() > Duration::ZERO);
    worker.subscriptions[0]
        .options
        .set_ignore_local_messages(false);
    worker.deliver(&message, true);
    assert!(rx.try_recv().is_err());
    assert_eq!(worker.sender.counters.snapshot().queue_drops, 0);
    assert_eq!(
        worker.sender.counters.snapshot().subscription_clock_errors,
        0
    );
}

#[test]
fn full_subscription_queue_consumes_its_interval_and_new_registrations_are_independent() {
    let (mut worker, _) = state(4);
    let mut options = SubscribeOptions::default();
    options.set_msgs_per_sec(1);
    let message = Message {
        topic: "topic".into(),
        type_name: "type".into(),
        data: vec![7],
    };
    let (full_tx, full_rx) = mpsc::sync_channel(1);
    full_tx
        .try_send(ReceivedMessage {
            message: message.clone(),
            intra_process: false,
        })
        .unwrap();
    let full = SubscriptionThrottle::new(1, worker.sender.counters.clone()).unwrap();
    worker.command(Command::Subscribe(
        "topic".into(),
        "type".into(),
        full_tx.into(),
        Arc::new(AtomicBool::new(true)),
        options,
        Some(full.clone()),
    ));
    worker.deliver(&message, false);
    assert_eq!(worker.sender.counters.snapshot().queue_drops, 1);
    assert!(*full.limiter.last.lock().unwrap() > Duration::ZERO);
    full_rx.recv().unwrap();
    let (fresh_tx, fresh_rx) = mpsc::sync_channel(1);
    let fresh = SubscriptionThrottle::new(1, worker.sender.counters.clone()).unwrap();
    worker.command(Command::Subscribe(
        "topic".into(),
        "type".into(),
        fresh_tx.into(),
        Arc::new(AtomicBool::new(true)),
        options,
        Some(fresh.clone()),
    ));
    worker.deliver(&message, true);
    assert!(full_rx.try_recv().is_err());
    assert_eq!(
        fresh_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .message,
        message
    );
    assert_eq!(worker.sender.counters.snapshot().queue_drops, 1);
    assert!(!Arc::ptr_eq(&full, &fresh));
}

#[test]
fn ready_topic_queries_keep_bounded_startup_deadline_and_shutdown() {
    let (mut worker, _) = state(1);
    let future = Instant::now() + Duration::from_secs(5);
    let (tx, rx) = mpsc::sync_channel(1);
    worker.command(Command::ReadyTopicInfo(TopicInfoQuery {
        topic: "/absent".into(),
        deadline: future,
        reply: tx,
    }));
    assert!(matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
    let (tx2, rx2) = mpsc::sync_channel(1);
    worker.command(Command::ReadyTopicInfo(TopicInfoQuery {
        topic: "/absent".into(),
        deadline: future,
        reply: tx2,
    }));
    assert!(matches!(rx2.recv().unwrap(), Err(Error::QueueFull)));
    assert_eq!(worker.sender.counters.snapshot().queue_drops, 1);
    worker.discovery_cycles = 1;
    worker.progress_topic_info_queries(Instant::now());
    assert!(matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
    // The existing immediate inspection still replies while startup is pending.
    let (immediate, result) = mpsc::sync_channel(1);
    worker.command(Command::KnownTopicInfo("/absent".into(), immediate));
    assert!(result.recv().unwrap().into_types().publishers.is_empty());
    worker.discovery_cycles = 2;
    worker.progress_topic_info_queries(Instant::now());
    let ready = rx.recv().unwrap().unwrap().into_types();
    assert!(ready.publishers.is_empty() && ready.subscribers.is_empty());
    assert!(worker.topic_info_queries.is_empty());

    let (tx, rx) = mpsc::sync_channel(1);
    worker.command(Command::ReadyTopicInfo(TopicInfoQuery {
        topic: "/absent".into(),
        deadline: Instant::now(),
        reply: tx,
    }));
    assert!(matches!(rx.recv().unwrap(), Err(Error::Timeout)));
    worker.discovery_cycles = 0;
    let (tx, rx) = mpsc::sync_channel(1);
    worker.command(Command::ReadyTopicInfo(TopicInfoQuery {
        topic: "/absent".into(),
        deadline: future,
        reply: tx,
    }));
    worker.sender.stop.store(true, Ordering::Release);
    worker.progress_topic_info_queries(Instant::now());
    assert!(matches!(rx.recv().unwrap(), Err(Error::Shutdown)));
    worker.sender.stop.store(false, Ordering::Release);
    worker.owner_dead.store(true, Ordering::Release);
    let (tx, rx) = mpsc::sync_channel(1);
    drop(rx);
    worker.command(Command::ReadyTopicInfo(TopicInfoQuery {
        topic: "/absent".into(),
        deadline: future,
        reply: tx,
    }));
    assert!(worker.topic_info_queries.is_empty());
}

#[test]
fn prepared_publisher_snapshot_requires_exact_live_lease_and_retains_bounds() {
    let (mut worker, _) = state(2);
    let active = Arc::new(AtomicBool::new(true));
    let bits = Arc::new(AtomicU8::new(0));
    worker.command(Command::Advertise(
        "/prepared".into(),
        "T".into(),
        AdvertiseOptions::default(),
        active.clone(),
        bits.clone(),
    ));
    let refresh = |topic: &str, kind: &str, lease: Arc<AtomicBool>, interest: Arc<AtomicU8>| {
        let (tx, rx) = mpsc::sync_channel(1);
        (
            Command::RefreshConnections(
                topic.into(),
                kind.into(),
                lease,
                interest,
                Instant::now() + Duration::from_secs(5),
                tx,
            ),
            rx,
        )
    };
    for _ in 0..2 {
        let (cmd, rx) = refresh("/prepared", "T", active.clone(), bits.clone());
        worker.command(cmd);
        assert_eq!(rx.recv().unwrap(), Ok(false));
        assert_eq!(worker.prepared_publications.len(), 1);
    }
    for (topic, kind, lease, interest) in [
        ("/other", "T", active.clone(), bits.clone()),
        ("/prepared", "wrong", active.clone(), bits.clone()),
        (
            "/prepared",
            "T",
            Arc::new(AtomicBool::new(true)),
            bits.clone(),
        ),
        ("/prepared", "T", active.clone(), Arc::new(AtomicU8::new(0))),
    ] {
        let (cmd, rx) = refresh(topic, kind, lease, interest);
        worker.command(cmd);
        assert_eq!(rx.recv().unwrap(), Err(Error::Shutdown));
    }
    let (cmd, rx) = refresh("/prepared", "T", active.clone(), bits.clone());
    drop(rx);
    worker.command(cmd);
    active.store(false, Ordering::Release);
    let (cmd, rx) = refresh("/prepared", "T", active, bits.clone());
    worker.command(cmd);
    assert_eq!(rx.recv().unwrap(), Err(Error::Shutdown));
    worker.cleanup();
    assert!(worker.prepared_publications.is_empty());

    let registry = Arc::new(Mutex::new(Vec::<LocalInterest>::new()));
    let poisoned = registry.clone();
    assert!(
        thread::spawn(move || {
            let _guard = poisoned.lock().unwrap();
            panic!("exercise isolated registry failure");
        })
        .join()
        .is_err()
    );
    let active = Arc::new(AtomicBool::new(true));
    worker.command(Command::Advertise(
        "/prepared".into(),
        "T".into(),
        AdvertiseOptions::default(),
        active,
        bits.clone(),
    ));
    bits.store(PROCESS_INTEREST, Ordering::Release);
    assert!(
        matches!(worker.refresh_process_interest_with(registry.lock()), Err(Error::Native(message)) if message.contains("poisoned"))
    );
    assert_eq!(bits.load(Ordering::Acquire), 0);
    assert!(
        worker
            .sender
            .counters
            .routing_errors
            .load(Ordering::Relaxed)
            > 0
    );
}

#[test]
fn prepared_same_process_delivery_stays_single_after_a_real_wire_connection() {
    let config = Config {
        partition: format!("prepared-wire-{}", Uuid::new_v4()),
        ..Default::default()
    };
    let observer = Node::new(config.clone()).unwrap();
    let receiver = observer.subscribe("/first", "T").unwrap();
    let owner = Node::new(config).unwrap();
    let publisher = owner.advertise("/first", "T").unwrap();
    // This bit comes only from the actual remote NEW_CONNECTION path. The
    // first publication therefore exercises both the direct and TCP routes.
    let deadline = Instant::now() + Duration::from_secs(5);
    while publisher.interest.load(Ordering::Acquire) & REMOTE_INTEREST == 0 {
        assert!(
            Instant::now() < deadline,
            "real local wire route did not register"
        );
        thread::yield_now();
    }
    for payload in [vec![1, 2, 3], vec![1, 2, 3], vec![4, 5, 6]] {
        assert!(
            publisher
                .refresh_connection_snapshot(Duration::from_secs(5))
                .unwrap()
        );
        assert!(
            publisher
                .refresh_connection_snapshot(Duration::from_secs(5))
                .unwrap()
        );
        assert!(publisher.connection_snapshot().unwrap());
        publisher.publish(payload.clone()).unwrap();
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(5)).unwrap().data,
            payload
        );
    }
    // Bounded negative observation after three positive deliveries; it is not
    // a universal remote delivery or scheduler guarantee.
    assert_eq!(
        receiver.recv_timeout(Duration::from_millis(200)),
        Err(Error::Timeout)
    );
}

#[test]
fn prepared_refresh_budget_rejects_without_enqueue_and_expired_work_stays_inert() {
    let (mut worker, commands) = state(4);
    let node = Node {
        sender: worker.sender.clone(),
        lifetime: Arc::new(WorkerLifetime {
            stop: worker.sender.stop.clone(),
            worker: Mutex::new(None),
        }),
        owner_dead: worker.owner_dead.clone(),
        local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
        config: worker.config.clone(),
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: worker.sender.identity.get().unwrap().services.clone(),
        subscription_dispatcher: Mutex::new(Weak::new()),
    };
    let publisher = node.advertise("/budget", "T").unwrap();
    worker.command(commands.recv().unwrap());
    assert_eq!(
        publisher.refresh_connection_snapshot(Duration::ZERO),
        Err(Error::Timeout)
    );
    assert_eq!(
        publisher.refresh_connection_snapshot(Duration::MAX),
        Err(Error::InvalidConfig)
    );
    assert!(matches!(
        commands.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert!(worker.prepared_publications.is_empty());
    thread::scope(|scope| {
        let pending =
            scope.spawn(|| publisher.refresh_connection_snapshot(Duration::from_millis(5)));
        let command = commands.recv_timeout(Duration::from_secs(5)).unwrap();
        let Command::RefreshConnections(_, _, _, _, deadline, _) = &command else {
            panic!("wrong command")
        };
        // This checks the propagated request deadline, not execution latency.
        assert!(*deadline <= Instant::now() + Duration::from_millis(5));
        assert_eq!(pending.join().unwrap(), Err(Error::Timeout));
        worker.command(command);
    });
    assert!(worker.prepared_publications.is_empty());
    assert_eq!(publisher.interest.load(Ordering::Acquire), 0);
}

#[test]
fn prepared_all_and_host_track_live_direct_routes_without_inventing_wire_membership() {
    for scope in [Scope::All, Scope::Host] {
        let (mut worker, _) = state(4);
        let observer = Node::new(worker.config.clone()).unwrap();
        let receiver = observer.subscribe("/route", "T").unwrap();
        let topic =
            qualified_name(&worker.config.partition, &worker.config.namespace, "/route").unwrap();
        let active = Arc::new(AtomicBool::new(true));
        let bits = Arc::new(AtomicU8::new(0));
        let mut options = AdvertiseOptions::default();
        options.set_scope(scope);
        worker.command(Command::Advertise(
            topic.clone(),
            "T".into(),
            options,
            active.clone(),
            bits.clone(),
        ));
        let refresh = |worker: &mut Worker| {
            let (tx, rx) = mpsc::sync_channel(1);
            worker.command(Command::RefreshConnections(
                topic.clone(),
                "T".into(),
                active.clone(),
                bits.clone(),
                Instant::now() + Duration::from_secs(5),
                tx,
            ));
            rx.recv().unwrap()
        };
        for _ in 0..2 {
            assert_eq!(refresh(&mut worker), Ok(true));
            assert_eq!(worker.prepared_publications.len(), 1);
            assert_eq!(
                bits.load(Ordering::Acquire) & PROCESS_INTEREST,
                PROCESS_INTEREST
            );
            // The private worker never drains discovery: subscriber metadata
            // cannot masquerade as an established remote NEW_CONNECTION.
            assert_eq!(bits.load(Ordering::Acquire) & REMOTE_INTEREST, 0);
        }
        worker.deliver_process_local(Message {
            topic: topic.clone(),
            type_name: "T".into(),
            data: vec![1, 2, 3],
        });
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(5)).unwrap().data,
            [1, 2, 3]
        );
        drop(receiver);
        assert_eq!(refresh(&mut worker), Ok(false));
        assert_eq!(bits.load(Ordering::Acquire) & PROCESS_INTEREST, 0);
        let _wrong = observer.subscribe("/route", "Wrong").unwrap();
        let mut foreign_config = worker.config.clone();
        foreign_config.message_port += 2;
        let foreign = Node::new(foreign_config).unwrap();
        let _foreign = foreign.subscribe("/route", "T").unwrap();
        assert_eq!(refresh(&mut worker), Ok(false));
        let _replacement = observer.subscribe("/route", "T").unwrap();
        assert_eq!(refresh(&mut worker), Ok(true));
        drop(observer);
        assert_eq!(refresh(&mut worker), Ok(false));
        active.store(false, Ordering::Release);
        assert_eq!(refresh(&mut worker), Err(Error::Shutdown));
        worker.cleanup();
        assert!(worker.prepared_publications.is_empty());
    }
}

#[test]
fn ready_service_queries_keep_bounds_expiry_and_shutdown_for_all_waiters() {
    let (mut worker, _) = state(2);
    let future = Instant::now() + Duration::from_secs(5);
    let enqueue = |worker: &mut Worker, deadline| {
        let (tx, rx) = mpsc::sync_channel(1);
        worker.command(Command::ReadyServiceList(ServiceListQuery {
            deadline,
            reply: tx,
        }));
        rx
    };
    let first = enqueue(&mut worker, future);
    let second = enqueue(&mut worker, future);
    assert!(matches!(
        enqueue(&mut worker, future).recv().unwrap(),
        Err(Error::QueueFull)
    ));
    assert_eq!(worker.sender.counters.snapshot().queue_drops, 1);
    worker.discovery_cycles = 1;
    worker.progress_service_list_queries(Instant::now());
    assert!(matches!(first.try_recv(), Err(mpsc::TryRecvError::Empty)));
    assert!(matches!(second.try_recv(), Err(mpsc::TryRecvError::Empty)));
    let (tx, rx) = mpsc::sync_channel(1);
    worker.command(Command::KnownServices(tx));
    assert!(rx.recv().unwrap().is_empty());
    // Expiry frees capacity without restarting discovery or blocking commands.
    worker.progress_service_list_queries(future);
    assert_eq!(first.recv().unwrap(), Err(Error::Timeout));
    assert_eq!(second.recv().unwrap(), Err(Error::Timeout));
    let ready = enqueue(&mut worker, future);
    worker.discovery_cycles = 2;
    worker.progress_service_list_queries(Instant::now());
    assert!(ready.recv().unwrap().unwrap().is_empty());
    assert!(matches!(
        enqueue(&mut worker, Instant::now()).recv().unwrap(),
        Err(Error::Timeout)
    ));
    worker.discovery_cycles = 0;
    let first = enqueue(&mut worker, future);
    let second = enqueue(&mut worker, future);
    worker.sender.stop.store(true, Ordering::Release);
    worker.progress_service_list_queries(Instant::now());
    assert_eq!(first.recv().unwrap(), Err(Error::Shutdown));
    assert_eq!(second.recv().unwrap(), Err(Error::Shutdown));
    worker.sender.stop.store(false, Ordering::Release);
    worker.owner_dead.store(true, Ordering::Release);
    assert_eq!(
        enqueue(&mut worker, future).recv().unwrap(),
        Err(Error::Shutdown)
    );
    worker.owner_dead.store(false, Ordering::Release);
    drop(enqueue(&mut worker, future));
    worker.discovery_cycles = 2;
    worker.progress_service_list_queries(Instant::now());
    assert!(worker.service_list_queries.is_empty());
}

#[test]
fn service_info_records_preserve_metadata_multiplicity_and_filter_stale_inputs() {
    let (mut worker, _) = state(16);
    let topic = qualified_name(&worker.config.partition, "", "/info").unwrap();
    let active = Arc::new(AtomicBool::new(true));
    worker.command(Command::Serve(
        topic.clone(),
        "req".into(),
        "rep".into(),
        Scope::All,
        Arc::new(mpsc::sync_channel(1).0),
        active.clone(),
    ));
    let local = worker.services[0].publisher.clone();
    let mut expected = worker.service_provider_info(&topic);
    assert_eq!(expected.len(), 1);
    for (index, channel, name, age, message) in [
        (0, 1, topic.clone(), 0, false),
        (1, 1, topic.clone(), 0, false),
        (2, 0, topic.clone(), 0, false),
        (
            3,
            1,
            qualified_name("foreign", "", "/info").unwrap(),
            0,
            false,
        ),
        (4, 1, topic.clone(), 4, false),
        (5, 1, topic.clone(), 0, true),
    ] {
        let mut publisher = local.clone();
        publisher.topic = name;
        publisher.address = format!("tcp://127.0.0.1:{}", 45000 + index);
        publisher.process_uuid = "remote".into();
        publisher.node_uuid = format!("node-{index}");
        publisher.scope = Scope::Host.into();
        publisher.pub_type = if message {
            Some(PubType::MsgPub(MessagePublisher::default()))
        } else {
            Some(PubType::SrvPub(ServicePublisher {
                socket_id: format!("socket-{index}"),
                request_type: "req".into(),
                response_type: "rep".into(),
            }))
        };
        if index < 2 {
            expected.push(ServiceProviderInfo {
                topic: topic.clone(),
                address: publisher.address.clone(),
                process_uuid: "remote".into(),
                node_uuid: publisher.node_uuid.clone(),
                scope: Scope::Host,
                socket_id: format!("socket-{index}"),
                request_type: "req".into(),
                response_type: "rep".into(),
            });
        }
        worker.peers.insert(
            PeerKey {
                channel,
                process: publisher.process_uuid.clone(),
                node: publisher.node_uuid.clone(),
                topic: publisher.topic.clone(),
                address: publisher.address.clone(),
            },
            (publisher, Instant::now() - Duration::from_secs(age)),
        );
    }
    expected.sort();
    assert_eq!(worker.service_provider_info(&topic), expected);
    assert_eq!(worker.service_provider_types(&topic).len(), 3);
    assert_eq!(worker.service_signatures(&topic).len(), 1);
    let (tx, rx) = mpsc::sync_channel(1);
    worker.command(Command::KnownServiceInfo(topic.clone(), tx));
    assert_eq!(rx.recv().unwrap(), expected);
    let (tx, rx) = mpsc::sync_channel(1);
    drop(rx);
    worker.command(Command::KnownServiceInfo(topic.clone(), tx));
    active.store(false, Ordering::Release);
    assert_eq!(worker.service_provider_info(&topic).len(), 2);
    worker.discovered(1, protocol::discovery("remote", Type::Bye, None));
    assert!(worker.service_provider_info(&topic).is_empty());
    assert_eq!(expected.len(), 3);
}

#[test]
fn service_info_query_keeps_validation_timeout_backpressure_and_shutdown_errors() {
    let (worker, commands) = state(1);
    let node = Node {
        sender: worker.sender.clone(),
        lifetime: Arc::new(WorkerLifetime {
            stop: worker.sender.stop.clone(),
            worker: Mutex::new(None),
        }),
        owner_dead: worker.owner_dead.clone(),
        local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
        config: Config {
            reply_timeout: Duration::from_millis(5),
            ..worker.config.clone()
        },
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: worker.sender.identity.get().unwrap().services.clone(),
        subscription_dispatcher: Mutex::new(Weak::new()),
    };
    assert_eq!(node.known_service_info("bad name"), Err(Error::InvalidName));
    assert_eq!(node.known_service_info("/info"), Err(Error::Timeout));
    assert_eq!(node.known_service_info("/info"), Err(Error::QueueFull));
    assert!(matches!(
        commands.recv().unwrap(),
        Command::KnownServiceInfo(_, _)
    ));
    let commands = thread::scope(|scope| {
        let consumer = scope.spawn(move || {
            drop(commands.recv().unwrap());
            commands
        });
        assert_eq!(node.known_service_info("/info"), Err(Error::Shutdown));
        consumer.join().unwrap()
    });
    drop(commands);
    assert_eq!(node.known_service_info("/info"), Err(Error::Shutdown));
    node.cancel();
    assert_eq!(node.known_service_info("/info"), Err(Error::Shutdown));
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert_eq!(node.statistics().queue_drops, 1);
}

#[test]
fn publication_info_normalizes_native_options_without_changing_wire_records() {
    let (mut worker, _) = state(16);
    let topic = qualified_name(&worker.config.partition, "", "/options").unwrap();
    let peer = std::env::var("GZ_CPP_PEER")
        .unwrap_or_else(|_| format!("{}/artifacts/cpp-peer", env!("CARGO_MANIFEST_DIR")));
    let mut expected = Vec::new();
    for (index, (flag, rate)) in [false, true]
        .into_iter()
        .flat_map(|flag| {
            [0, 1, 7, u64::MAX]
                .into_iter()
                .map(move |rate| (flag, rate))
        })
        .enumerate()
    {
        let output = std::process::Command::new(&peer)
            .args([
                "publisher-option-reference",
                if flag { "1" } else { "0" },
                &rate.to_string(),
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let native = String::from_utf8(output.stdout).unwrap();
        let fields: Vec<_> = native.split_whitespace().collect();
        assert_eq!(fields.len(), 2);
        let effective = fields[1].parse::<u64>().unwrap();
        assert!(matches!(fields[0], "0" | "1"));
        let scope = if index % 2 == 0 {
            Scope::All
        } else {
            Scope::Host
        };
        let publisher = WirePublisher {
            topic: topic.clone(),
            address: format!("tcp://127.0.0.1:{}", 45000 + index),
            process_uuid: "remote".into(),
            node_uuid: format!("node-{index}"),
            scope: scope.into(),
            pub_type: Some(PubType::MsgPub(MessagePublisher {
                ctrl: format!("tcp://127.0.0.1:{}", 46000 + index),
                msg_type: "kind".into(),
                throttled: flag,
                msgs_per_sec: rate,
            })),
        };
        worker.discovered(0, protocol::advertisement("remote", publisher.clone()));
        expected.push(TopicPublisherInfo {
            topic: topic.clone(),
            address: publisher.address,
            control_address: format!("tcp://127.0.0.1:{}", 46000 + index),
            process_uuid: "remote".into(),
            node_uuid: format!("node-{index}"),
            scope,
            type_name: "kind".into(),
            throttled: fields[0] == "1",
            msgs_per_sec: effective,
        });
    }
    let raw = worker.peers.clone();
    let inactive =
        worker.wire_publisher(topic.clone(), PubType::MsgPub(MessagePublisher::default()));
    worker.advertisements.push((
        inactive,
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicU8::new(0)),
    ));
    expected.sort();
    assert_eq!(worker.topic_publisher_info(&topic), expected);
    let (tx, rx) = mpsc::sync_channel(1);
    worker.command(Command::KnownTopicPublishers(topic.clone(), tx));
    assert_eq!(rx.recv().unwrap(), expected);
    let (tx, rx) = mpsc::sync_channel(1);
    drop(rx);
    worker.command(Command::KnownTopicPublishers(topic.clone(), tx));
    assert!(worker.peers == raw);
    // Filtering follows the same channel/kind/topic/expiry boundary as existing snapshots.
    for (index, channel, other, age, service) in [
        (8, 1, false, 0, false),
        (9, 0, true, 0, false),
        (10, 0, false, 4, false),
        (11, 0, false, 0, true),
    ] {
        let publisher = WirePublisher {
            topic: if other {
                qualified_name("foreign", "", "/options").unwrap()
            } else {
                topic.clone()
            },
            address: format!("tcp://127.0.0.1:{}", 45000 + index),
            process_uuid: "filtered".into(),
            node_uuid: format!("node-{index}"),
            scope: Scope::All.into(),
            pub_type: Some(if service {
                PubType::SrvPub(ServicePublisher::default())
            } else {
                PubType::MsgPub(MessagePublisher::default())
            }),
        };
        worker.peers.insert(
            PeerKey {
                channel,
                process: publisher.process_uuid.clone(),
                node: publisher.node_uuid.clone(),
                topic: publisher.topic.clone(),
                address: publisher.address.clone(),
            },
            (publisher, Instant::now() - Duration::from_secs(age)),
        );
    }
    assert_eq!(worker.topic_publisher_info(&topic), expected);
    worker.discovered(0, protocol::discovery("remote", Type::Bye, None));
    assert!(worker.topic_publisher_info(&topic).is_empty());
    assert_eq!(expected.len(), 8);
}

#[test]
fn publication_info_keeps_validation_queue_timeout_and_shutdown_bounds() {
    let (worker, commands) = state(1);
    let node = Node {
        sender: worker.sender.clone(),
        lifetime: Arc::new(WorkerLifetime {
            stop: worker.sender.stop.clone(),
            worker: Mutex::new(None),
        }),
        owner_dead: worker.owner_dead.clone(),
        local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
        config: Config {
            reply_timeout: Duration::from_millis(5),
            ..worker.config.clone()
        },
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: worker.sender.identity.get().unwrap().services.clone(),
        subscription_dispatcher: Mutex::new(Weak::new()),
    };
    assert_eq!(
        node.known_topic_publishers("bad name"),
        Err(Error::InvalidName)
    );
    assert_eq!(node.known_topic_publishers("/info"), Err(Error::Timeout));
    assert_eq!(node.known_topic_publishers("/info"), Err(Error::QueueFull));
    assert!(matches!(
        commands.recv().unwrap(),
        Command::KnownTopicPublishers(_, _)
    ));
    let commands = thread::scope(|scope| {
        let consumer = scope.spawn(move || {
            drop(commands.recv().unwrap());
            commands
        });
        assert_eq!(node.known_topic_publishers("/info"), Err(Error::Shutdown));
        consumer.join().unwrap()
    });
    drop(commands);
    assert_eq!(node.known_topic_publishers("/info"), Err(Error::Shutdown));
    node.cancel();
    assert_eq!(node.known_topic_publishers("/info"), Err(Error::Shutdown));
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert_eq!(node.statistics().queue_drops, 1);
}

fn subscriber_metadata_packet(worker: &Worker, topic: &str, node: &str) -> WirePublisher {
    let mut p = worker.subscriber_publisher(topic, "gz.msgs.Int32");
    p.process_uuid = "remote".into();
    p.node_uuid = node.into();
    p.address = "tcp://127.0.0.1:45000".into();
    let Some(PubType::MsgPub(info)) = &mut p.pub_type else {
        unreachable!()
    };
    info.ctrl = "ctrl-A".into();
    info.throttled = true;
    info.msgs_per_sec = 7;
    p
}
fn subscriber_metadata_key(p: &WirePublisher) -> PeerKey {
    PeerKey {
        channel: 0,
        process: p.process_uuid.clone(),
        node: p.node_uuid.clone(),
        topic: p.topic.clone(),
        address: p.address.clone(),
    }
}
fn receive_subscriber_metadata(worker: &mut Worker, p: WirePublisher) {
    worker.discovered(
        0,
        protocol::discovery(
            "wire-sender",
            Type::SubscribersRep,
            Some(DiscContents::Pub(p)),
        ),
    );
}

#[test]
fn subscriber_metadata_keeps_native_first_record_and_legacy_type_variants() {
    let (mut worker, _) = state(16);
    let topic = qualified_name(&worker.config.partition, "", "/metadata").unwrap();
    let first = subscriber_metadata_packet(&worker, &topic, "node");
    receive_subscriber_metadata(&mut worker, first.clone());
    let held = worker.topic_subscriber_info(&topic);
    assert_eq!(held.len(), 1);
    assert_eq!(
        held[0],
        TopicSubscriberInfo {
            topic: topic.clone(),
            address: "tcp://127.0.0.1:45000".into(),
            control_address: "ctrl-A".into(),
            process_uuid: "remote".into(),
            node_uuid: "node".into(),
            scope: Scope::All,
            type_name: "gz.msgs.Int32".into(),
            throttled: true,
            msgs_per_sec: 7
        }
    );
    for kind in 0..4 {
        let mut changed = first.clone();
        if kind == 1 {
            changed.scope = Scope::Host.into();
        }
        let Some(PubType::MsgPub(info)) = &mut changed.pub_type else {
            unreachable!()
        };
        match kind {
            0 => info.ctrl = "ctrl-B".into(),
            2 => info.msg_type = "gz.msgs.StringMsg".into(),
            3 => info.msgs_per_sec = 0,
            _ => {}
        };
        receive_subscriber_metadata(&mut worker, changed);
    }
    assert_eq!(worker.subscriber_metadata.len(), 1);
    assert_eq!(
        worker.subscriber_metadata[&subscriber_metadata_key(&first)].publisher,
        first
    );
    assert_eq!(worker.topic_subscriber_info(&topic), held);
    assert_eq!(
        worker.topic_endpoint_types(&topic).subscribers,
        ["gz.msgs.Int32", "gz.msgs.StringMsg"]
    );
    let mut address = first.clone();
    address.address = "tcp://127.0.0.1:45001".into();
    receive_subscriber_metadata(&mut worker, address);
    let mut node = first.clone();
    node.node_uuid = "other-node".into();
    receive_subscriber_metadata(&mut worker, node);
    let mut process = first.clone();
    process.process_uuid = "other-process".into();
    receive_subscriber_metadata(&mut worker, process);
    assert_eq!(worker.topic_subscriber_info(&topic).len(), 4);
    assert_eq!(worker.topic_endpoint_types(&topic).subscribers.len(), 5);
    let mut end = first.clone();
    end.node_uuid = "missing".into();
    worker.discovered(
        0,
        protocol::discovery(
            "wire-sender",
            Type::EndConnection,
            Some(DiscContents::Pub(end.clone())),
        ),
    );
    assert_eq!(worker.topic_subscriber_info(&topic).len(), 4);
    end.node_uuid = "node".into();
    let Some(PubType::MsgPub(info)) = &mut end.pub_type else {
        unreachable!()
    };
    info.ctrl = "unrelated-control".into();
    worker.discovered(
        0,
        protocol::discovery(
            "wire-sender",
            Type::EndConnection,
            Some(DiscContents::Pub(end)),
        ),
    );
    assert_eq!(worker.topic_subscriber_info(&topic).len(), 2);
    assert_eq!(worker.topic_endpoint_types(&topic).subscribers.len(), 2);
    worker.discovered(0, protocol::discovery("remote", Type::Bye, None));
    assert_eq!(worker.topic_subscriber_info(&topic).len(), 1);
    worker.discovered(0, protocol::discovery("other-process", Type::Bye, None));
    assert!(worker.topic_subscriber_info(&topic).is_empty());
    assert_eq!(held[0].control_address, "ctrl-A");
}

#[test]
fn subscriber_metadata_capacity_refresh_expiry_and_drop_counts_are_bounded() {
    let (mut worker, _) = state(1);
    let topic = qualified_name(&worker.config.partition, "", "/bounded").unwrap();
    let first = subscriber_metadata_packet(&worker, &topic, "node");
    let key = subscriber_metadata_key(&first);
    receive_subscriber_metadata(&mut worker, first.clone());
    worker.subscriber_metadata.get_mut(&key).unwrap().seen =
        Instant::now() - Duration::from_secs(2);
    let mut changed = first.clone();
    let Some(PubType::MsgPub(info)) = &mut changed.pub_type else {
        unreachable!()
    };
    info.msg_type = "gz.msgs.StringMsg".into();
    info.msgs_per_sec = 0;
    receive_subscriber_metadata(&mut worker, changed);
    assert!(worker.subscriber_metadata[&key].seen.elapsed() < Duration::from_secs(1));
    assert_eq!(worker.subscriber_metadata[&key].publisher, first);
    assert_eq!(worker.sender.counters.snapshot().queue_drops, 1);
    let mut other = first.clone();
    other.node_uuid = "other".into();
    receive_subscriber_metadata(&mut worker, other.clone());
    assert_eq!(worker.sender.counters.snapshot().queue_drops, 2);
    assert_eq!(worker.subscriber_interests.len(), 1);
    assert_eq!(worker.subscriber_metadata.len(), 1);
    // The two views have independently bounded admission, with one drop per packet.
    worker.subscriber_metadata.get_mut(&key).unwrap().seen =
        Instant::now() - Duration::from_secs(4);
    assert!(worker.topic_subscriber_info(&topic).is_empty());
    receive_subscriber_metadata(&mut worker, other.clone());
    assert_eq!(worker.sender.counters.snapshot().queue_drops, 3);
    assert_eq!(worker.topic_subscriber_info(&topic)[0].node_uuid, "other");
    receive_subscriber_metadata(&mut worker, first);
    assert_eq!(worker.sender.counters.snapshot().queue_drops, 4);
    assert_eq!(worker.topic_subscriber_info(&topic)[0].node_uuid, "other");
    for stamp in worker.subscriber_interests.values_mut() {
        *stamp = Instant::now() - Duration::from_secs(4);
    }
    for record in worker.subscriber_metadata.values_mut() {
        record.seen = Instant::now() - Duration::from_secs(4);
    }
    other.node_uuid = "replacement".into();
    receive_subscriber_metadata(&mut worker, other);
    assert_eq!(worker.sender.counters.snapshot().queue_drops, 4);
    assert_eq!(worker.subscriber_interests.len(), 1);
    assert_eq!(worker.subscriber_metadata.len(), 1);
    assert_eq!(
        worker.topic_subscriber_info(&topic)[0].node_uuid,
        "replacement"
    );
}

#[test]
fn subscriber_metadata_local_normalization_and_native_merge_identity_are_distinct() {
    let (mut worker, _) = state(16);
    let topic = qualified_name(&worker.config.partition, "", "/local").unwrap();
    let first = Arc::new(AtomicBool::new(true));
    let duplicate = Arc::new(AtomicBool::new(true));
    let integer = Arc::new(AtomicBool::new(true));
    for (kind, active) in [
        ("gz.msgs.StringMsg", first.clone()),
        ("gz.msgs.StringMsg", duplicate.clone()),
        ("gz.msgs.Int32", integer.clone()),
    ] {
        let mut options = SubscribeOptions::default();
        options.set_msgs_per_sec(0);
        worker.command(Command::Subscribe(
            topic.clone(),
            kind.into(),
            mpsc::sync_channel(1).0.into(),
            active,
            options,
            None,
        ));
    }
    let local = worker.topic_subscriber_info(&topic);
    assert_eq!(local.len(), 2);
    for info in &local {
        assert!(!info.throttled);
        assert_eq!(info.msgs_per_sec, u64::MAX);
        assert!(info.control_address.is_empty());
        assert_eq!(info.scope, Scope::All);
    }
    let original = local[0].clone();
    let mut rate = original.clone();
    rate.msgs_per_sec = 7;
    rate.throttled = true;
    assert!(original.same_subscriber(&rate));
    for field in 0..7 {
        let mut other = original.clone();
        match field {
            0 => other.topic.push_str("_other"),
            1 => other.address.push_str("_other"),
            2 => other.control_address.push_str("other"),
            3 => other.process_uuid.push_str("_other"),
            4 => other.node_uuid.push_str("_other"),
            5 => other.scope = Scope::Host,
            _ => other.type_name.push_str("_other"),
        };
        assert!(!original.same_subscriber(&other));
    }
    let mut remote = subscriber_metadata_packet(&worker, &topic, "remote-node");
    remote.scope = Scope::Host.into();
    receive_subscriber_metadata(&mut worker, remote.clone());
    let records = worker.topic_subscriber_info(&topic);
    assert_eq!(records.len(), 3);
    let observed = records.iter().find(|r| r.process_uuid == "remote").unwrap();
    assert_eq!(observed.control_address, "ctrl-A");
    assert_eq!(observed.scope, Scope::Host);
    assert!(observed.throttled);
    assert_eq!(observed.msgs_per_sec, 7);
    for (node, flag, rate) in [("false-rate", false, 7), ("true-max", true, u64::MAX)] {
        let mut p = remote.clone();
        p.node_uuid = node.into();
        let Some(PubType::MsgPub(info)) = &mut p.pub_type else {
            unreachable!()
        };
        info.throttled = flag;
        info.msgs_per_sec = rate;
        receive_subscriber_metadata(&mut worker, p);
        let records = worker.topic_subscriber_info(&topic);
        let normalized = records.iter().find(|r| r.node_uuid == node).unwrap();
        assert!(!normalized.throttled);
        assert_eq!(normalized.msgs_per_sec, u64::MAX);
    }
    first.store(false, Ordering::Release);
    duplicate.store(false, Ordering::Release);
    integer.store(false, Ordering::Release);
    assert_eq!(worker.topic_subscriber_info(&topic).len(), 3);
    let other = qualified_name(&worker.config.partition, "", "/other").unwrap();
    assert!(worker.topic_subscriber_info(&other).is_empty());
    let (tx, rx) = mpsc::sync_channel(1);
    worker.command(Command::KnownTopicSubscribers(topic.clone(), tx));
    assert_eq!(rx.recv().unwrap().len(), 3);
    let (tx, rx) = mpsc::sync_channel(1);
    drop(rx);
    worker.command(Command::KnownTopicSubscribers(topic, tx));
}

#[test]
fn subscriber_metadata_query_keeps_name_queue_timeout_and_shutdown_bounds() {
    let (worker, commands) = state(1);
    let node = Node {
        sender: worker.sender.clone(),
        lifetime: Arc::new(WorkerLifetime {
            stop: worker.sender.stop.clone(),
            worker: Mutex::new(None),
        }),
        owner_dead: worker.owner_dead.clone(),
        local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
        config: Config {
            reply_timeout: Duration::from_millis(5),
            ..worker.config.clone()
        },
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: worker.sender.identity.get().unwrap().services.clone(),
        subscription_dispatcher: Mutex::new(Weak::new()),
    };
    assert_eq!(
        node.known_topic_subscribers("bad name"),
        Err(Error::InvalidName)
    );
    assert_eq!(node.known_topic_subscribers("/info"), Err(Error::Timeout));
    assert_eq!(node.known_topic_subscribers("/info"), Err(Error::QueueFull));
    assert!(matches!(
        commands.recv().unwrap(),
        Command::KnownTopicSubscribers(_, _)
    ));
    let commands = thread::scope(|scope| {
        let consumer = scope.spawn(move || {
            drop(commands.recv().unwrap());
            commands
        });
        assert_eq!(node.known_topic_subscribers("/info"), Err(Error::Shutdown));
        consumer.join().unwrap()
    });
    drop(commands);
    assert_eq!(node.known_topic_subscribers("/info"), Err(Error::Shutdown));
    node.cancel();
    assert_eq!(node.known_topic_subscribers("/info"), Err(Error::Shutdown));
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert_eq!(node.statistics().queue_drops, 1);
}

fn route_subscription(worker: &mut Worker, topic: &str) -> Arc<AtomicBool> {
    let active = Arc::new(AtomicBool::new(true));
    let (tx, _) = mpsc::sync_channel(1);
    worker.command(Command::Subscribe(
        topic.into(),
        "kind".into(),
        tx.into(),
        active.clone(),
        SubscribeOptions::default(),
        None,
    ));
    active
}
fn route_publication(worker: &Worker, topic: &str, address: &str) -> WirePublisher {
    let mut publisher = worker.wire_publisher(
        topic.into(),
        PubType::MsgPub(MessagePublisher {
            msg_type: "kind".into(),
            ..Default::default()
        }),
    );
    publisher.process_uuid = "route-process".into();
    publisher.address = address.into();
    publisher
}
#[test]
fn publication_routes_outlive_silence_with_separate_capacity_and_drop_accounting() {
    let (mut worker, _) = state(1);
    let active = route_subscription(&mut worker, "/route");
    let first = route_publication(&worker, "/route", "tcp://127.0.0.1:45010");
    worker.discovered(0, protocol::advertisement("route-process", first.clone()));
    assert_eq!(worker.publication_routes.len(), 1);
    assert_eq!(worker.subscriptions_connected.len(), 1);
    worker.peers.clear(); // The worker's silence sweep removes discovery records.
    worker.cleanup();
    assert_eq!(worker.publication_routes.len(), 1);
    assert_eq!(worker.subscriptions_connected.len(), 1);
    let mut other = first.clone();
    other.address = "tcp://127.0.0.1:45011".into();
    worker.discovered(0, protocol::advertisement("route-process", other.clone()));
    assert_eq!(worker.peers.len(), 1);
    assert_eq!(worker.publication_routes.len(), 1);
    assert!(!worker.subscriptions_connected.contains(&other.address));
    assert_eq!(worker.sender.counters.snapshot().queue_drops, 1);
    // An ignored duplicate advertisement does not retry or charge a second drop.
    worker.discovered(0, protocol::advertisement("route-process", other.clone()));
    assert_eq!(worker.sender.counters.snapshot().queue_drops, 1);
    active.store(false, Ordering::Release);
    worker.cleanup();
    assert!(worker.publication_routes.is_empty());
    assert!(worker.subscriptions_connected.is_empty());
    let _replacement = route_subscription(&mut worker, "/route");
    assert_eq!(worker.publication_routes.len(), 1);
    assert!(worker.subscriptions_connected.contains(&other.address));
    // This is the same final subscription retirement performed by Worker::run.
    worker.subscriptions.clear();
    worker.cleanup();
    assert!(worker.publication_routes.is_empty());
    assert!(worker.subscriptions_connected.is_empty());
}
#[test]
fn publication_routes_share_addresses_and_preserve_explicit_channel_withdrawals() {
    let (mut worker, _) = state(4);
    let first_active = route_subscription(&mut worker, "/first");
    let second_active = route_subscription(&mut worker, "/second");
    let first = route_publication(&worker, "/first", "tcp://127.0.0.1:45012");
    let second = route_publication(&worker, "/second", &first.address);
    for publisher in [&first, &second] {
        worker.discovered(
            0,
            protocol::advertisement("route-process", publisher.clone()),
        );
    }
    assert_eq!(worker.publication_routes.len(), 2);
    assert_eq!(worker.subscriptions_connected.len(), 1);
    worker.peers.clear();
    worker.cleanup();
    worker.discovered(1, protocol::discovery("route-process", Type::Bye, None));
    worker.discovered(
        1,
        protocol::discovery(
            "route-process",
            Type::Unadvertise,
            Some(DiscContents::Pub(first.clone())),
        ),
    );
    worker.cleanup();
    assert_eq!(worker.publication_routes.len(), 2);
    first_active.store(false, Ordering::Release);
    worker.cleanup();
    assert_eq!(worker.publication_routes.len(), 1);
    assert_eq!(worker.subscriptions_connected.len(), 1);
    worker.discovered(
        0,
        protocol::discovery(
            "route-process",
            Type::Unadvertise,
            Some(DiscContents::Pub(first.clone())),
        ),
    );
    worker.cleanup();
    assert_eq!(worker.publication_routes.len(), 1);
    worker.discovered(
        0,
        protocol::discovery(
            "route-process",
            Type::Unadvertise,
            Some(DiscContents::Pub(second.clone())),
        ),
    );
    worker.cleanup();
    assert!(worker.publication_routes.is_empty());
    assert!(worker.subscriptions_connected.is_empty());
    worker.discovered(0, protocol::advertisement("route-process", second));
    worker.discovered(0, protocol::discovery("route-process", Type::Bye, None));
    worker.cleanup();
    assert!(worker.publication_routes.is_empty());
    assert!(worker.subscriptions_connected.is_empty());
    assert!(second_active.load(Ordering::Acquire));
}

// Regression supplied by the independent source reviewer.
#[test]
fn withdrawal_and_replacement_in_one_batch_keeps_new_route_connected() {
    let (mut worker, _) = state(2);
    let _active = route_subscription(&mut worker, "/route");
    let first = route_publication(&worker, "/route", "tcp://127.0.0.1:45020");
    let mut other = route_publication(&worker, "/route", "tcp://127.0.0.1:45021");
    other.node_uuid = "other-node".into();
    for publication in [&first, &other] {
        worker.discovered(
            0,
            protocol::advertisement("route-process", publication.clone()),
        );
    }
    assert_eq!(worker.subscriptions_connected.len(), 2);
    worker.discovered(
        0,
        protocol::discovery(
            "route-process",
            Type::Unadvertise,
            Some(DiscContents::Pub(first.clone())),
        ),
    );
    let mut replacement = first;
    replacement.address = "tcp://127.0.0.1:45022".into();
    worker.discovered(
        0,
        protocol::advertisement("route-process", replacement.clone()),
    );
    worker.cleanup();
    assert!(
        worker
            .subscriptions_connected
            .contains(&replacement.address),
        "admitted replacement route has no physical connection"
    );
}

// Regression supplied by the independent source reviewer.
#[test]
fn failed_connection_does_not_retain_permanent_route_capacity() {
    let (mut worker, _) = state(1);
    let _active = route_subscription(&mut worker, "/route");
    let controls = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    controls
        .set_read_timeout(Some(Duration::from_millis(30)))
        .unwrap();
    worker.config.multicast = Ipv4Addr::LOCALHOST;
    worker.config.message_port = controls.local_addr().unwrap().port();
    let invalid = route_publication(&worker, "/route", "not-an-endpoint");
    worker.discovered(0, protocol::advertisement("route-process", invalid.clone()));
    assert_eq!(worker.sender.counters.snapshot().send_failures, 1);
    assert!(worker.publication_routes.is_empty());
    assert!(worker.subscriptions_connected.is_empty());
    assert_eq!(worker.sender.counters.snapshot().queue_drops, 0);
    let mut buffer = vec![0; protocol::MAX_DATAGRAM];
    assert!(
        controls.recv(&mut buffer).is_err(),
        "failed connect announced registration"
    );
    worker.peers.clear();
    worker.cleanup();
    let valid = route_publication(&worker, "/route", "tcp://127.0.0.1:45023");
    worker.discovered(0, protocol::advertisement("route-process", valid.clone()));
    assert!(
        worker.subscriptions_connected.contains(&valid.address),
        "failed connect permanently consumed route capacity"
    );
    let count = controls.recv(&mut buffer).unwrap();
    assert_eq!(
        protocol::decode(&buffer[..count]).unwrap().r#type,
        Type::NewConnection as i32
    );
    assert_eq!(worker.sender.counters.snapshot().send_failures, 1);
    assert_eq!(worker.sender.counters.snapshot().queue_drops, 0);
}

fn service_registry_node(worker: &Worker) -> Node {
    Node {
        sender: worker.sender.clone(),
        lifetime: Arc::new(WorkerLifetime {
            stop: worker.sender.stop.clone(),
            worker: Mutex::new(None),
        }),
        owner_dead: worker.owner_dead.clone(),
        local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
        config: worker.config.clone(),
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: worker.sender.identity.get().unwrap().services.clone(),
        subscription_dispatcher: Mutex::new(Weak::new()),
    }
}
fn service_registry_query(
    node: &Node,
    worker: &mut Worker,
    commands: &Receiver<Command>,
    topic: &str,
) -> Result<ServiceTypeResolution, Error> {
    thread::scope(|scope| {
        let query = scope.spawn(|| node.resolve_service_types(topic, "", "", Duration::ZERO));
        worker.command(commands.recv_timeout(Duration::from_secs(1)).unwrap());
        query.join().unwrap()
    })
}
fn service_registry_packet(source: &Worker, topic: &str) -> WirePublisher {
    let mut publisher = source.wire_publisher(
        topic.into(),
        PubType::SrvPub(ServicePublisher {
            request_type: "wire-old".into(),
            response_type: "wire-old".into(),
            ..Default::default()
        }),
    );
    publisher.address = source.service_address.clone();
    publisher
}
#[test]
fn local_service_resolution_masks_stale_copies_without_changing_worker_views() {
    let (mut source, source_commands) = state(2);
    let node = service_registry_node(&source);
    let (mut observer, commands) = service_state(source.config.clone());
    let query_node = service_registry_node(&observer);
    let service = node.serve("/cached", "declared", "declared").unwrap();
    source.command(source_commands.recv().unwrap());
    let topic = node.topic("/cached").unwrap();
    let publisher = service_registry_packet(&source, &topic);
    observer.discovered(
        1,
        protocol::advertisement(&source.process, publisher.clone()),
    );
    assert_eq!(observer.local_service_owners.len(), 1);
    assert_eq!(
        observer.service_signatures(&topic)[0].request_type,
        "wire-old"
    );
    assert_eq!(
        service_registry_query(&query_node, &mut observer, &commands, "/cached"),
        Ok(ServiceTypeResolution::Resolved(ServiceTypes {
            request_type: "declared".into(),
            response_type: "declared".into()
        }))
    );
    let active = service._lease.active.clone();
    drop(service);
    assert!(!active.load(Ordering::Acquire));
    assert_eq!(
        service_registry_query(&query_node, &mut observer, &commands, "/cached"),
        Ok(ServiceTypeResolution::NoProviders)
    );
    // The old public worker projection is deliberately still its wire snapshot.
    assert_eq!(
        observer.service_signatures(&topic)[0].request_type,
        "wire-old"
    );
    drop(node);
    drop(source);
    drop(source_commands);
    let held = observer.local_service_owners[&publisher.process_uuid].clone();
    assert!(held.get().unwrap().stopped.upgrade().is_none());
    // Even a retained active flag cannot override expired owner stop ownership.
    held.get()
        .unwrap()
        .services
        .lock()
        .unwrap()
        .push(LocalService {
            topic: topic.clone(),
            types: ServiceTypes {
                request_type: "not-live".into(),
                response_type: "not-live".into(),
            },
            active: Arc::downgrade(&active),
            callback: None,
            queue: Weak::new(),
        });
    active.store(true, Ordering::Release);
    assert_eq!(
        service_registry_query(&query_node, &mut observer, &commands, "/cached"),
        Ok(ServiceTypeResolution::NoProviders)
    );
    let weak = Arc::downgrade(&held);
    drop(held);
    observer.discovered(
        1,
        protocol::discovery(&publisher.process_uuid, Type::Bye, None),
    );
    assert!(observer.local_service_owners.is_empty());
    assert!(weak.upgrade().is_none());
}
#[test]
fn local_service_owner_retention_follows_admitted_peer_capacity_and_withdrawal() {
    let (source, _) = state(2);
    let (other, _) = service_state(source.config.clone());
    let mut cfg = source.config.clone();
    cfg.capacity = 1;
    let (mut observer, _) = service_state(cfg);
    let first_topic = qualified_name(&source.config.partition, "", "/first").unwrap();
    let second_topic = qualified_name(&source.config.partition, "", "/second").unwrap();
    let first = service_registry_packet(&source, &first_topic);
    let second = service_registry_packet(&other, &second_topic);
    observer.discovered(1, protocol::advertisement(&source.process, first.clone()));
    observer.discovered(1, protocol::advertisement(&other.process, second.clone()));
    assert_eq!(observer.peers.len(), 1);
    assert_eq!(observer.local_service_owners.len(), 1);
    assert!(!observer.local_service_owners.contains_key(&other.process));
    observer.discovered(0, protocol::discovery(&source.process, Type::Bye, None));
    assert_eq!(observer.local_service_owners.len(), 1);
    observer.discovered(
        1,
        protocol::discovery(
            &source.process,
            Type::Unadvertise,
            Some(DiscContents::Pub(first)),
        ),
    );
    assert!(observer.local_service_owners.is_empty());
    observer.discovered(1, protocol::advertisement(&other.process, second));
    assert_eq!(observer.local_service_owners.len(), 1);
    observer.peers.clear();
    observer.retain_local_service_owners();
    assert!(observer.local_service_owners.is_empty());
}
#[test]
fn local_service_registry_admission_rollback_keeps_types_and_leases_authoritative() {
    let (worker, commands) = state(1);
    let node = service_registry_node(&worker);
    let (tx, _) = mpsc::sync_channel(1);
    node.sender.send(Command::KnownServices(tx)).unwrap();
    assert!(matches!(
        node.serve("/rejected", "bad", "bad"),
        Err(Error::QueueFull)
    ));
    assert!(node.services.lock().unwrap().is_empty());
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    drop(commands.recv().unwrap());
    let service = node.serve("/accepted", "original", "original").unwrap();
    assert!(matches!(
        node.serve("/accepted", "different", "different"),
        Err(Error::AlreadyAdvertised)
    ));
    assert!(matches!(
        node.serve("/overflow", "different", "different"),
        Err(Error::QueueFull)
    ));
    let services = node.services.lock().unwrap();
    assert_eq!(services.len(), 1);
    assert_eq!(services[0].types.request_type, "original");
    drop(services);
    drop(service);
    assert!(
        !node.services.lock().unwrap()[0]
            .active
            .upgrade()
            .is_some_and(|active| active.load(Ordering::Acquire))
    );
    let index = Mutex::new(vec![Arc::downgrade(&worker.sender.identity)]);
    assert_eq!(service_workers(&index).unwrap().len(), 1);
    let dead = Arc::new(OnceLock::<WorkerIdentity>::new());
    index.lock().unwrap().push(Arc::downgrade(&dead));
    drop(dead);
    assert_eq!(service_workers(&index).unwrap().len(), 1);
    assert_eq!(index.lock().unwrap().len(), 1);
    thread::scope(|scope| {
        assert!(
            scope
                .spawn(|| {
                    let _guard = index.lock().unwrap();
                    panic!("isolated owner index poison");
                })
                .join()
                .is_err()
        );
    });
    assert!(matches!(service_workers(&index), Err(Error::Native(_))));
}

#[test]
fn local_service_owner_expires_with_its_last_admitted_peer() {
    let (source, _) = state(1);
    let (mut observer, commands) = service_state(source.config.clone());
    let topic = qualified_name(&source.config.partition, "", "/expired").unwrap();
    let publisher = service_registry_packet(&source, &topic);
    observer.discovered(1, protocol::advertisement(&source.process, publisher));
    let weak = Arc::downgrade(&observer.local_service_owners[&source.process]);
    for (_, stamp) in observer.peers.values_mut() {
        *stamp = Instant::now() - Duration::from_secs(4);
    }
    drop(source);
    let node = service_registry_node(&observer);
    let stop = observer.sender.stop.clone();
    let worker = thread::spawn(move || observer.run(commands, stop));
    *node.lifetime.worker.lock().unwrap() = Some(worker);
    let deadline = Instant::now() + Duration::from_secs(1);
    while weak.upgrade().is_some() {
        assert!(
            Instant::now() < deadline,
            "expired peer retained owner identity"
        );
        thread::sleep(Duration::from_millis(1));
    }
    // Node owns stop-and-join even if the assertion above unwinds.
    drop(node);
}

fn callback_config(capacity: usize) -> Config {
    Config {
        partition: format!("private-callback-{}", Uuid::new_v4()),
        capacity,
        ..Config::default()
    }
}
fn callback_spawn_failure(_: CallbackTask) -> std::io::Result<JoinHandle<()>> {
    Err(std::io::Error::other("injected callback spawn failure"))
}
fn callback_wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !predicate() {
        assert!(Instant::now() < deadline, "callback worker did not finish");
        thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn response_callbacks_validate_admission_and_rollback_spawn_without_user_code() {
    let (worker, commands) = service_state(callback_config(2));
    let node = service_registry_node(&worker);
    let timeout = Duration::from_secs(1);
    for (topic, input, output, data, bound, expected) in [
        ("Bad Srv", "in", "out", vec![], timeout, Error::InvalidName),
        ("/test", "", "out", vec![], timeout, Error::InvalidFrame),
        ("/test", "in", "", vec![], timeout, Error::InvalidFrame),
        (
            "/test",
            "in",
            EMPTY_TYPE,
            vec![],
            timeout,
            Error::Unsupported("response callback for gz.msgs.Empty"),
        ),
        ("/test", "in", "out", vec![], Duration::ZERO, Error::Timeout),
        (
            "/test",
            "in",
            "out",
            vec![],
            Duration::MAX,
            Error::InvalidConfig,
        ),
        (
            "/test",
            "in",
            "out",
            vec![0; node.config.max_message_bytes + 1],
            timeout,
            Error::InvalidFrame,
        ),
    ] {
        let result = node.request_callback_with_spawner(
            topic,
            input,
            output,
            data,
            bound,
            |_, _| panic!("invalid request invoked user code"),
            callback_spawn_failure,
        );
        assert!(matches!(result, Err(error) if error == expected));
    }
    assert!(commands.try_recv().is_err());
    for (topic, input, output, expected) in [
        ("Bad Srv", "in", "out", Error::InvalidName),
        ("/test", "", "out", Error::InvalidFrame),
        (
            "/test",
            "in",
            EMPTY_TYPE,
            Error::Unsupported("response callback for gz.msgs.Empty"),
        ),
    ] {
        assert!(
            matches!(node.serve_callback(topic,input,output, |_| unreachable!()), Err(error) if error == expected)
        );
    }
    assert!(matches!(
        node.serve_callback_with_spawner(
            "/test",
            "in",
            "out",
            |_| unreachable!(),
            callback_spawn_failure
        ),
        Err(Error::Native(_))
    ));
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 0);
    assert_eq!(node.advertised_services().unwrap(), Vec::<String>::new());
    assert!(matches!(commands.try_recv(), Ok(Command::Serve(..))));
    assert!(matches!(
        node.request_callback_with_spawner(
            "/test",
            "in",
            "out",
            vec![],
            timeout,
            |_, _| unreachable!(),
            callback_spawn_failure
        ),
        Err(Error::Native(_))
    ));
    assert!(commands.try_recv().is_err()); // Failed spawn precedes request admission.
    assert_eq!(node.sender.callback_requests.load(Ordering::Acquire), 0);
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 0);
    // A full command queue rolls back both the dispatcher and registration.
    node.sender
        .send(Command::Unsubscribe("filler".into()))
        .unwrap();
    node.sender
        .send(Command::Unsubscribe("filler".into()))
        .unwrap();
    assert!(matches!(
        node.serve_callback("/test", "in", "out", |_| unreachable!()),
        Err(Error::QueueFull)
    ));
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert!(matches!(
        node.request_callback("/test", "in", "out", vec![], timeout, |_, _| unreachable!()),
        Err(Error::QueueFull)
    ));
    assert_eq!(node.sender.callback_requests.load(Ordering::Acquire), 0);
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 0);
    drop(commands);
    assert!(matches!(
        node.request_callback("/test", "in", "out", vec![], timeout, |_, _| unreachable!()),
        Err(Error::Shutdown)
    ));
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 0);
    node.cancel();
    assert!(matches!(
        node.serve_callback("/test", "in", "out", |_| unreachable!()),
        Err(Error::Shutdown)
    ));
    assert!(matches!(
        node.request_callback("/test", "in", "out", vec![], timeout, |_, _| unreachable!()),
        Err(Error::Shutdown)
    ));
}
#[test]
fn response_callbacks_inline_need_no_thread_and_obey_independent_request_capacity() {
    let cfg = callback_config(1);
    let provider = Node::new(cfg.clone()).unwrap();
    let caller = Node::new(cfg.clone()).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let _service = provider
        .serve_callback("/test", "in", "out", move |data| {
            count.fetch_add(1, Ordering::Relaxed);
            Ok((data.to_vec(), true))
        })
        .unwrap();
    let permit = caller
        .sender
        .callback_permit(&caller.sender.callback_requests)
        .unwrap();
    assert!(matches!(
        caller.request_callback(
            "/test",
            "in",
            "out",
            vec![],
            Duration::from_secs(1),
            |_, _| unreachable!()
        ),
        Err(Error::QueueFull)
    ));
    assert_eq!(calls.load(Ordering::Acquire), 0);
    drop(permit);
    let request = caller
        .request_callback_with_spawner(
            "/test",
            "in",
            "out",
            vec![1],
            Duration::from_secs(1),
            |data, ok| {
                assert_eq!(data, [1]);
                assert!(ok);
            },
            callback_spawn_failure,
        )
        .unwrap();
    assert!(request.worker.is_none());
    assert_eq!(request.wait(Duration::from_secs(1)), Ok(()));
    assert_eq!(calls.load(Ordering::Acquire), 1);
    assert_eq!(caller.sender.callback_threads.load(Ordering::Acquire), 0);
    for dimension in 0..5 {
        let mut other = cfg.clone();
        match dimension {
            0 => other.partition.push_str("other"),
            1 => other.namespace = "other".into(),
            2 => other.service_port = 15487,
            3 => other.multicast = "239.255.0.8".parse().unwrap(),
            _ => other.interface = "127.0.0.2".parse().unwrap(),
        }
        let other = Node::new(other).unwrap();
        assert!(matches!(
            other.request_callback_with_spawner(
                "test",
                "in",
                "out",
                vec![],
                Duration::from_secs(1),
                |_, _| unreachable!(),
                callback_spawn_failure
            ),
            Err(Error::Native(_))
        ));
    }
    assert_eq!(calls.load(Ordering::Acquire), 1);
    assert!(matches!(
        provider.serve_callback("/second", "in", "out", |_| unreachable!()),
        Err(Error::QueueFull)
    ));
    assert_eq!(provider.sender.counters.snapshot().queue_drops, 1);
}
#[test]
fn response_callbacks_preserve_errors_panics_deadlines_and_lease_ownership() {
    let node = Node::new(callback_config(8)).unwrap();
    let timeout = Duration::from_secs(1);
    let service = node
        .serve_callback("/test", "in", "out", |_| panic!("service panic"))
        .unwrap();
    let request = node
        .request_callback("/test", "in", "out", vec![], timeout, |_, _| unreachable!())
        .unwrap();
    assert!(
        matches!(request.wait(timeout),Err(Error::Native(message)) if message=="service callback panicked")
    );
    assert_eq!(node.advertised_services().unwrap(), ["/test"]);
    drop(service);
    let _service = node
        .serve_callback("/test", "in", "out", |_| Ok((vec![1], false)))
        .unwrap();
    let request = node
        .request_callback("/test", "in", "out", vec![], timeout, |data, ok| {
            assert_eq!(data, [1]);
            assert!(!ok);
            panic!("response panic");
        })
        .unwrap();
    assert!(
        matches!(request.wait(timeout),Err(Error::Native(message)) if message=="response callback panicked")
    );
    assert_eq!(node.advertised_services().unwrap(), ["/test"]);
    let _slow = node
        .serve_callback("/slow", "in", "out", |_| {
            thread::sleep(Duration::from_millis(40));
            Ok((vec![], true))
        })
        .unwrap();
    let request = node
        .request_callback(
            "/slow",
            "in",
            "out",
            vec![],
            Duration::from_millis(10),
            |_, _| unreachable!(),
        )
        .unwrap();
    assert_eq!(request.wait(timeout), Err(Error::Timeout));
    let pending = node
        .begin_request("/slow", "in", "out", vec![], timeout)
        .unwrap();
    assert_eq!(pending.wait(timeout), Ok((vec![], true)));
    let service = node
        .serve_callback("/error", "in", "out", |_| Err(Error::InvalidFrame))
        .unwrap();
    assert_eq!(
        node.request("/error", "in", "out", vec![], timeout),
        Ok((vec![], false))
    );
    service.cancel();
    assert!(
        !node
            .advertised_services()
            .unwrap()
            .contains(&"/error".into())
    );
    drop(service);
    let request = node
        .request_callback(
            "/absent",
            "in",
            "out",
            vec![],
            Duration::from_millis(30),
            |_, _| unreachable!(),
        )
        .unwrap();
    assert_eq!(request.wait(timeout), Err(Error::Timeout));
    let request = node
        .request_callback(
            "/absent",
            "in",
            "out",
            vec![],
            timeout,
            |_, _| unreachable!(),
        )
        .unwrap();
    request.cancel();
    assert_eq!(request.wait(timeout), Err(Error::Shutdown));
    let request = node
        .request_callback(
            "/absent",
            "in",
            "out",
            vec![],
            timeout,
            |_, _| unreachable!(),
        )
        .unwrap();
    node.cancel();
    assert_eq!(request.wait(timeout), Err(Error::Shutdown));
}

struct CallbackUnblockOnDrop(SyncSender<()>);
impl Drop for CallbackUnblockOnDrop {
    fn drop(&mut self) {
        let _ = self.0.try_send(());
    }
}
// A capacity-one command queue must drain Serve before the next operation.
// This private setup barrier does not wait for discovery or expected peer data.
fn callback_worker_service_ready(node: &Node, topic: &str) {
    let deadline = Instant::now() + Duration::from_secs(1);
    let (tx, rx) = mpsc::sync_channel(1);
    loop {
        match node.sender.send(Command::KnownServices(tx.clone())) {
            Ok(()) => break,
            Err(Error::QueueFull) => {
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(2));
            }
            Err(error) => panic!("callback setup failed: {error}"),
        }
    }
    assert!(
        rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap()
            .contains(&topic.to_string())
    );
}
#[test]
fn response_callbacks_hold_thread_and_request_permits_until_real_user_exit() {
    let cfg = callback_config(1);
    let provider = Node::new(cfg.clone()).unwrap();
    let caller = Node::new(cfg).unwrap();
    let (entered, entry) = mpsc::sync_channel(1);
    let (resume, resumed) = mpsc::sync_channel(1);
    let resumed = Mutex::new(resumed);
    let service = provider
        .serve_callback("/busy", "in", "out", move |_| {
            entered.send(()).unwrap();
            resumed.lock().unwrap().recv().unwrap();
            Ok((vec![], true))
        })
        .unwrap();
    let resume = CallbackUnblockOnDrop(resume);
    // Use the worker's local queue: this tests permit lifetime, not cold
    // cross-worker discovery. The ordinary API never selects the inline path.
    callback_worker_service_ready(&provider, "/busy");
    let pending = provider
        .begin_request("/busy", "in", "out", vec![], Duration::from_secs(2))
        .unwrap();
    entry.recv_timeout(Duration::from_secs(1)).unwrap();
    service.cancel();
    assert_eq!(provider.sender.slots.load(Ordering::Acquire), 0);
    assert_eq!(provider.sender.callback_threads.load(Ordering::Acquire), 1);
    assert!(matches!(
        provider.serve_callback("/next", "in", "out", |_| unreachable!()),
        Err(Error::QueueFull)
    ));
    resume.0.send(()).unwrap();
    assert_eq!(pending.wait(Duration::from_secs(1)), Ok((vec![], true)));
    drop(service);
    assert_eq!(provider.sender.callback_threads.load(Ordering::Acquire), 0);

    // A response closure can continue after cancellation; neither permit is
    // returned until that closure really exits.
    let queued = caller.serve("/queued", "in", "out").unwrap();
    callback_worker_service_ready(&caller, "/queued");
    let (entered, entry) = mpsc::sync_channel(1);
    let (resume, resumed) = mpsc::sync_channel(1);
    let request = caller
        .request_callback(
            "/queued",
            "in",
            "out",
            vec![],
            Duration::from_secs(2),
            move |_, _| {
                entered.send(()).unwrap();
                resumed.recv().unwrap();
            },
        )
        .unwrap();
    let resume = CallbackUnblockOnDrop(resume);
    queued
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .respond(vec![], true)
        .unwrap();
    entry.recv_timeout(Duration::from_secs(1)).unwrap();
    request.cancel();
    assert_eq!(caller.sender.callback_threads.load(Ordering::Acquire), 1);
    assert_eq!(caller.sender.callback_requests.load(Ordering::Acquire), 1);
    assert!(matches!(
        caller.request_callback(
            "/absent",
            "in",
            "out",
            vec![],
            Duration::from_secs(1),
            |_, _| unreachable!()
        ),
        Err(Error::QueueFull)
    ));
    resume.0.send(()).unwrap();
    assert_eq!(request.wait(Duration::from_secs(1)), Ok(()));
    drop(request);
    assert_eq!(caller.sender.callback_threads.load(Ordering::Acquire), 0);
    assert_eq!(caller.sender.callback_requests.load(Ordering::Acquire), 0);
}
#[test]
fn response_callbacks_are_reentrant_and_can_drop_their_own_service_and_request() {
    let node = Arc::new(Node::new(callback_config(8)).unwrap());
    let timeout = Duration::from_secs(1);
    let nested = node.clone();
    let _inner = node
        .serve_callback("/inner", "in", "out", |data| Ok((data.to_vec(), true)))
        .unwrap();
    let holder = Arc::new(Mutex::new(None::<CallbackService>));
    let destroy = holder.clone();
    let service = node
        .serve_callback("/outer", "in", "out", move |_| {
            drop(destroy.lock().unwrap().take());
            assert_eq!(nested.advertised_services().unwrap(), ["/inner"]);
            let request = nested
                .request_callback(
                    "/inner",
                    "in",
                    "out",
                    vec![3],
                    Duration::from_secs(1),
                    |data, ok| {
                        assert_eq!(data, [3]);
                        assert!(ok);
                    },
                )
                .unwrap();
            request.wait(Duration::from_secs(1))?;
            Ok((vec![4], true))
        })
        .unwrap();
    *holder.lock().unwrap() = Some(service);
    // This runs on the dispatcher and drops that dispatcher handle itself.
    assert_eq!(
        node.request("/outer", "in", "out", vec![], timeout),
        Ok((vec![4], true))
    );
    callback_wait_until(|| node.sender.callback_threads.load(Ordering::Acquire) == 1);
    // Self-drop of an inline service also holds no registry lock.
    let destroy = holder.clone();
    let service = node
        .serve_callback("/inline", "in", "out", move |_| {
            drop(destroy.lock().unwrap().take());
            Ok((vec![], true))
        })
        .unwrap();
    *holder.lock().unwrap() = Some(service);
    let request = node
        .request_callback("/inline", "in", "out", vec![], timeout, |_, _| {})
        .unwrap();
    request.wait(timeout).unwrap();
    assert!(holder.lock().unwrap().is_none());
    let queued = node.serve("/queued", "in", "out").unwrap();
    let requests = Arc::new(Mutex::new(None::<CallbackRequest>));
    let own = requests.clone();
    let (ready, read) = mpsc::sync_channel(1);
    let request = node
        .request_callback("/queued", "in", "out", vec![], timeout, move |_, _| {
            read.recv().unwrap();
            drop(own.lock().unwrap().take());
        })
        .unwrap();
    *requests.lock().unwrap() = Some(request);
    queued
        .recv_timeout(timeout)
        .unwrap()
        .respond(vec![], true)
        .unwrap();
    ready.send(()).unwrap();
    callback_wait_until(|| requests.lock().unwrap().is_none());
    callback_wait_until(|| node.sender.callback_requests.load(Ordering::Acquire) == 0);
}
#[test]
fn response_callbacks_enforce_provider_and_caller_payload_bounds_before_response_code() {
    let cfg = callback_config(4);
    let mut small = cfg.clone();
    small.max_message_bytes = 1;
    let provider = Node::new(small.clone()).unwrap();
    let caller = Node::new(cfg.clone()).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let _service = provider
        .serve_callback("/small", "in", "out", move |_| {
            count.fetch_add(1, Ordering::Relaxed);
            Ok((vec![1, 2], true))
        })
        .unwrap();
    assert!(matches!(
        caller.request_callback(
            "/small",
            "in",
            "out",
            vec![1, 2],
            Duration::from_secs(1),
            |_, _| unreachable!()
        ),
        Err(Error::InvalidFrame)
    ));
    assert_eq!(calls.load(Ordering::Acquire), 0);
    let request = caller
        .request_callback(
            "/small",
            "in",
            "out",
            vec![1],
            Duration::from_secs(1),
            |_, _| unreachable!(),
        )
        .unwrap();
    assert_eq!(
        request.wait(Duration::from_secs(1)),
        Err(Error::InvalidFrame)
    );
    assert_eq!(calls.load(Ordering::Acquire), 1);
    let provider = Node::new(cfg).unwrap();
    let _service = provider
        .serve_callback("/large", "in", "out", |_| Ok((vec![1, 2], true)))
        .unwrap();
    let caller = Node::new(small).unwrap();
    let request = caller
        .request_callback(
            "/large",
            "in",
            "out",
            vec![],
            Duration::from_secs(1),
            |_, _| unreachable!(),
        )
        .unwrap();
    assert_eq!(
        request.wait(Duration::from_secs(1)),
        Err(Error::InvalidFrame)
    );
}

struct CallbackUserDrop(Box<dyn Fn() + Send + Sync>);
impl Drop for CallbackUserDrop {
    fn drop(&mut self) {
        (self.0)();
    }
}
#[test]
fn inline_response_admission_includes_last_captured_destructor() {
    let cfg = callback_config(2);
    let provider = Node::new(cfg.clone()).unwrap();
    let mut cfg = cfg;
    cfg.capacity = 1;
    let caller = Arc::new(Node::new(cfg).unwrap());
    let (entered, entry) = mpsc::sync_channel(1);
    let (resume, resumed) = mpsc::sync_channel(1);
    let resumed = Mutex::new(resumed);
    let (destroying, destruction) = mpsc::sync_channel(1);
    let (release, released) = mpsc::sync_channel(1);
    let released = Mutex::new(released);
    let user_drop = CallbackUserDrop(Box::new(move || {
        destroying.send(()).unwrap();
        released
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
    }));
    let service = provider
        .serve_callback("/drop", "in", "out", move |_| {
            let _ = &user_drop;
            entered.send(()).unwrap();
            resumed
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            Ok((vec![], true))
        })
        .unwrap();
    let resume = CallbackUnblockOnDrop(resume);
    let calls = Arc::new(AtomicUsize::new(0));
    let called = calls.clone();
    let _other = provider
        .serve_callback("/other", "in", "out", move |_| {
            called.fetch_add(1, Ordering::AcqRel);
            Ok((vec![], true))
        })
        .unwrap();
    let client = caller.clone();
    let request = thread::spawn(move || {
        client.request_callback(
            "/drop",
            "in",
            "out",
            vec![],
            Duration::from_secs(5),
            |_, _| {},
        )
    });
    let release = CallbackUnblockOnDrop(release);
    entry.recv_timeout(Duration::from_secs(1)).unwrap();
    // The request now owns the last callback reference after this dispatcher
    // retires. Its captured user value must be destroyed on the requesting path.
    drop(service);
    resume.0.send(()).unwrap();
    destruction.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(!request.is_finished());
    assert_eq!(caller.sender.callback_requests.load(Ordering::Acquire), 1);
    assert!(matches!(
        caller.request_callback(
            "/other",
            "in",
            "out",
            vec![],
            Duration::from_secs(1),
            |_, _| {},
        ),
        Err(Error::QueueFull)
    ));
    assert_eq!(calls.load(Ordering::Acquire), 0);
    release.0.send(()).unwrap();
    request
        .join()
        .unwrap()
        .unwrap()
        .wait(Duration::from_secs(1))
        .unwrap();
    assert_eq!(caller.sender.callback_requests.load(Ordering::Acquire), 0);
    caller
        .request_callback(
            "/other",
            "in",
            "out",
            vec![],
            Duration::from_secs(1),
            |_, _| {},
        )
        .unwrap()
        .wait(Duration::from_secs(1))
        .unwrap();
    assert_eq!(calls.load(Ordering::Acquire), 1);
}
#[test]
fn inline_response_destructor_reentry_and_unwind_preserve_locks_and_admission() {
    for panic_on_drop in [false, true] {
        let cfg = callback_config(2);
        let provider = Arc::new(Node::new(cfg.clone()).unwrap());
        let mut cfg = cfg;
        cfg.capacity = 1;
        let caller = Arc::new(Node::new(cfg).unwrap());
        let (entered, entry) = mpsc::sync_channel(1);
        let (resume, resumed) = mpsc::sync_channel(1);
        let resumed = Mutex::new(resumed);
        let weak_provider = Arc::downgrade(&provider);
        let weak_caller = Arc::downgrade(&caller);
        let dropped = Arc::new(AtomicUsize::new(0));
        let drops = dropped.clone();
        let user_drop = CallbackUserDrop(Box::new(move || {
            let provider = weak_provider.upgrade().unwrap();
            let caller = weak_caller.upgrade().unwrap();
            // Other test workers may briefly hold the shared owner index.
            // A lock held by this destructor's caller could never pass these
            // bounded probes, even while unrelated workers make progress.
            callback_wait_until(|| provider.services.try_lock().is_ok());
            callback_wait_until(|| local_service_workers().try_lock().is_ok());
            assert_eq!(provider.advertised_services().unwrap(), ["/other"]);
            assert_eq!(
                provider.resolve_service_types("/other", "", "", Duration::ZERO),
                Ok(ServiceTypeResolution::Resolved(ServiceTypes {
                    request_type: "in".into(),
                    response_type: "out".into(),
                }))
            );
            assert!(matches!(
                caller.request_callback(
                    "/other",
                    "in",
                    "out",
                    vec![],
                    Duration::from_secs(1),
                    |_, _| unreachable!(),
                ),
                Err(Error::QueueFull)
            ));
            drops.fetch_add(1, Ordering::AcqRel);
            if panic_on_drop {
                panic!("intentional captured destructor panic");
            }
        }));
        let service = provider
            .serve_callback("/drop", "in", "out", move |_| {
                let _ = &user_drop;
                entered.send(()).unwrap();
                resumed
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
                Ok((vec![], true))
            })
            .unwrap();
        let resume = CallbackUnblockOnDrop(resume);
        let _other = provider
            .serve_callback("/other", "in", "out", |_| Ok((vec![], true)))
            .unwrap();
        let client = caller.clone();
        let request = thread::spawn(move || {
            client.request_callback(
                "/drop",
                "in",
                "out",
                vec![],
                Duration::from_secs(5),
                |_, _| {},
            )
        });
        entry.recv_timeout(Duration::from_secs(1)).unwrap();
        drop(service);
        resume.0.send(()).unwrap();
        let result = request.join();
        if panic_on_drop {
            let panic = result.err().expect("ordinary user Drop panic must unwind");
            assert_eq!(
                panic.downcast_ref::<&str>(),
                Some(&"intentional captured destructor panic")
            );
        } else {
            result
                .unwrap()
                .unwrap()
                .wait(Duration::from_secs(1))
                .unwrap();
        }
        assert_eq!(dropped.load(Ordering::Acquire), 1);
        assert_eq!(caller.sender.callback_requests.load(Ordering::Acquire), 0);
        assert!(!provider.services.is_poisoned());
        assert!(!local_service_workers().is_poisoned());
        caller
            .request_callback(
                "/other",
                "in",
                "out",
                vec![],
                Duration::from_secs(1),
                |_, _| {},
            )
            .unwrap()
            .wait(Duration::from_secs(1))
            .unwrap();
    }
}

std::thread_local! {
    static ONEWAY_SPAWN_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        std::cell::RefCell::new(None);
}
fn oneway_spawn_with_hook(task: CallbackTask) -> std::io::Result<JoinHandle<()>> {
    let worker = spawn_callback(task)?;
    let hook = ONEWAY_SPAWN_HOOK.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook();
    }
    Ok(worker)
}
#[test]
fn oneway_callback_admission_rolls_back_spawn_queue_and_commit_retirement() {
    let (worker, commands) = service_state(callback_config(2));
    let node = service_registry_node(&worker);
    assert!(matches!(
        node.serve_oneway_with_spawner("/spawn", "in", |_| unreachable!(), callback_spawn_failure),
        Err(Error::Native(_))
    ));
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 0);
    assert!(node.advertised_services().unwrap().is_empty());
    assert!(matches!(commands.try_recv(), Ok(Command::Serve(..))));
    node.sender
        .send(Command::Unsubscribe("full".into()))
        .unwrap();
    node.sender
        .send(Command::Unsubscribe("full".into()))
        .unwrap();
    assert!(matches!(
        node.serve_oneway_callback("/queue", "in", |_| unreachable!()),
        Err(Error::QueueFull)
    ));
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 0);
    drop(commands);
    assert!(matches!(
        node.serve_oneway_callback("/closed", "in", |_| unreachable!()),
        Err(Error::Shutdown)
    ));
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 0);
    for retire in 0..3 {
        let node = Arc::new(Node::new(callback_config(2)).unwrap());
        let weak = Arc::downgrade(&node);
        ONEWAY_SPAWN_HOOK.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move || {
                let node = weak.upgrade().unwrap();
                // The handoff can succeed before registration commits, but a
                // parked dispatcher must never execute this failed registration.
                node.begin_request("/race", "in", EMPTY_TYPE, vec![], Duration::from_secs(1))
                    .unwrap()
                    .wait(Duration::from_secs(1))
                    .unwrap();
                thread::sleep(Duration::from_millis(30));
                match retire {
                    0 => node.unadvertise_service("/race").unwrap(),
                    1 => node.cancel(),
                    _ => {
                        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            let _guard = node.services.lock().unwrap();
                            panic!("injected commit registry poison");
                        }));
                    }
                }
            }))
        });
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let result = node.serve_oneway_with_spawner(
            "/race",
            "in",
            move |_| {
                count.fetch_add(1, Ordering::AcqRel);
                Ok(())
            },
            oneway_spawn_with_hook,
        );
        if retire == 2 {
            assert!(matches!(result, Err(Error::Native(_))));
        } else {
            assert!(matches!(result, Err(Error::Shutdown)));
        }
        assert_eq!(calls.load(Ordering::Acquire), 0);
        assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
        assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 0);
    }
}
#[test]
fn oneway_dispatcher_capacity_survives_lease_retirement_until_actual_exit() {
    let node = Node::new(callback_config(1)).unwrap();
    let (entered, entry) = mpsc::sync_channel(1);
    let (resume, resumed) = mpsc::sync_channel(1);
    let resumed = Mutex::new(resumed);
    let service = node
        .serve_oneway_callback("/busy", "in", move |_| {
            entered.send(()).unwrap();
            resumed
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            Ok(())
        })
        .unwrap();
    let resume = CallbackUnblockOnDrop(resume);
    assert!(matches!(
        node.serve_oneway_callback("/busy", "in", |_| unreachable!()),
        Err(Error::AlreadyAdvertised)
    ));
    callback_worker_service_ready(&node, "/busy");
    assert_eq!(
        node.begin_request("/busy", "in", EMPTY_TYPE, vec![], Duration::from_secs(2))
            .unwrap()
            .wait(Duration::from_secs(1)),
        Ok((vec![], true))
    );
    entry.recv_timeout(Duration::from_secs(1)).unwrap();
    node.unadvertise_service("/busy").unwrap();
    assert!(node.advertised_services().unwrap().is_empty());
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert!(matches!(
        node.serve_oneway_callback("/busy", "in", |_| unreachable!()),
        Err(Error::QueueFull)
    ));
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 1);
    resume.0.send(()).unwrap();
    drop(service);
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 0);
    let _replacement = node
        .serve_oneway_callback("/busy", "in", |_| Ok(()))
        .unwrap();
    node.request_oneway("/busy", "in", vec![], Duration::from_secs(1))
        .unwrap();
}
#[test]
fn oneway_local_domains_payload_validation_and_nonpreemptible_timeout_are_explicit() {
    let cfg = callback_config(4);
    let mut small = cfg.clone();
    small.max_message_bytes = 1;
    let provider = Node::new(small).unwrap();
    let caller = Node::new(cfg.clone()).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let service = provider
        .serve_oneway_callback("/bounded", "in", move |_| {
            count.fetch_add(1, Ordering::AcqRel);
            thread::sleep(Duration::from_millis(20));
            Ok(())
        })
        .unwrap();
    assert_eq!(
        caller.request_oneway("/bounded", "in", vec![1, 2], Duration::from_secs(1)),
        Err(Error::InvalidFrame)
    );
    assert_eq!(calls.load(Ordering::Acquire), 0);
    assert_eq!(
        caller.request_oneway("/bounded", "in", vec![1], Duration::ZERO),
        Err(Error::Timeout)
    );
    assert_eq!(
        caller.request_oneway("/bounded", "in", vec![1], Duration::MAX),
        Err(Error::InvalidConfig)
    );
    assert_eq!(calls.load(Ordering::Acquire), 0);
    // Timeout validates queued admission. It never preempts an inline function
    // or changes successful completion into a post-execution timeout.
    assert_eq!(
        caller.request_oneway("/bounded", "in", vec![1], Duration::from_millis(1)),
        Ok(())
    );
    assert_eq!(calls.load(Ordering::Acquire), 1);
    for dimension in 0..5 {
        let mut other = cfg.clone();
        let topic = if dimension == 1 {
            "bounded"
        } else {
            "/bounded"
        };
        match dimension {
            0 => other.partition.push_str("other"),
            1 => other.namespace = "other".into(),
            2 => other.service_port = 15487,
            3 => other.multicast = "239.255.0.8".parse().unwrap(),
            _ => other.interface = "127.0.0.2".parse().unwrap(),
        }
        let other = Node::new(other).unwrap();
        let qualified = other.remapped_topic(topic).unwrap();
        assert!(
            other
                .local_callback(&qualified, "in", EMPTY_TYPE)
                .unwrap()
                .is_none()
        );
    }
    let mut message_port = cfg;
    message_port.message_port = 15486;
    let other = Node::new(message_port).unwrap();
    assert!(matches!(
        other
            .local_callback(&other.remapped_topic("/bounded").unwrap(), "in", EMPTY_TYPE)
            .unwrap(),
        Some(CallbackState::OneWay(_))
    ));
    provider.cancel();
    assert!(
        caller
            .local_callback(
                &caller.remapped_topic("/bounded").unwrap(),
                "in",
                EMPTY_TYPE
            )
            .unwrap()
            .is_none()
    );
    drop(service);
    caller.cancel();
    assert_eq!(
        caller.request_oneway("/bounded", "in", vec![], Duration::ZERO),
        Err(Error::Shutdown)
    );
}
#[test]
fn oneway_inline_admission_covers_last_user_destructor_and_unwind() {
    for panic_on_drop in [false, true] {
        let cfg = callback_config(2);
        let provider = Node::new(cfg.clone()).unwrap();
        let mut cfg = cfg;
        cfg.capacity = 1;
        let caller = Arc::new(Node::new(cfg).unwrap());
        let (entered, entry) = mpsc::sync_channel(1);
        let (resume, resumed) = mpsc::sync_channel(1);
        let resumed = Mutex::new(resumed);
        let weak = Arc::downgrade(&caller);
        let dropped = Arc::new(AtomicUsize::new(0));
        let drops = dropped.clone();
        let user_drop = CallbackUserDrop(Box::new(move || {
            let caller = weak.upgrade().unwrap();
            callback_wait_until(|| caller.services.try_lock().is_ok());
            callback_wait_until(|| local_service_workers().try_lock().is_ok());
            assert!(matches!(
                caller.request_oneway("/other", "in", vec![], Duration::from_secs(1)),
                Err(Error::QueueFull)
            ));
            drops.fetch_add(1, Ordering::AcqRel);
            if panic_on_drop {
                panic!("one-way captured destructor panic");
            }
        }));
        let service = provider
            .serve_oneway_callback("/drop", "in", move |_| {
                let _ = &user_drop;
                entered.send(()).unwrap();
                resumed
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
                Ok(())
            })
            .unwrap();
        let resume = CallbackUnblockOnDrop(resume);
        let _other = provider
            .serve_oneway_callback("/other", "in", |_| Ok(()))
            .unwrap();
        let client = caller.clone();
        let request = thread::spawn(move || {
            client.request_oneway("/drop", "in", vec![], Duration::from_secs(1))
        });
        entry.recv_timeout(Duration::from_secs(1)).unwrap();
        drop(service);
        resume.0.send(()).unwrap();
        let result = request.join();
        if panic_on_drop {
            let panic = result.expect_err("ordinary destructor panic must unwind");
            assert_eq!(
                panic.downcast_ref::<&str>(),
                Some(&"one-way captured destructor panic")
            );
        } else {
            assert_eq!(result.unwrap(), Ok(()));
        }
        assert_eq!(dropped.load(Ordering::Acquire), 1);
        assert_eq!(caller.sender.callback_requests.load(Ordering::Acquire), 0);
        caller
            .request_oneway("/other", "in", vec![], Duration::from_secs(1))
            .unwrap();
    }
}

#[test]
fn oneway_dispatcher_can_drop_its_own_handle_without_early_permit_release() {
    let node = Arc::new(Node::new(callback_config(1)).unwrap());
    let holder = Arc::new(Mutex::new(None::<OneWayCallbackService>));
    let own = holder.clone();
    let weak = Arc::downgrade(&node);
    let completed = Arc::new(AtomicBool::new(false));
    let done = completed.clone();
    let service = node
        .serve_oneway_callback("/self", "in", move |_| {
            let service = own.lock().unwrap().take();
            drop(service);
            let node = weak.upgrade().unwrap();
            assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
            assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 1);
            assert!(matches!(
                node.serve_oneway_callback("/next", "in", |_| unreachable!()),
                Err(Error::QueueFull)
            ));
            done.store(true, Ordering::Release);
            Ok(())
        })
        .unwrap();
    *holder.lock().unwrap() = Some(service);
    callback_worker_service_ready(&node, "/self");
    assert_eq!(
        node.begin_request("/self", "in", EMPTY_TYPE, vec![], Duration::from_secs(1))
            .unwrap()
            .wait(Duration::from_secs(1)),
        Ok((vec![], true))
    );
    callback_wait_until(|| completed.load(Ordering::Acquire));
    callback_wait_until(|| node.sender.callback_threads.load(Ordering::Acquire) == 0);
    assert!(holder.lock().unwrap().is_none());
    let _replacement = node
        .serve_oneway_callback("/next", "in", |_| Ok(()))
        .unwrap();
}

// These manual workers deliberately do not process the provider's Serve command:
// admission must reuse the live Service queue without discovery or a wire request.
fn queued_local_pending(node: &Node, deadline: Instant) -> (Pending, ReplyReceiver) {
    let (mut pending, reply) = pending(deadline);
    pending.topic = node.topic("/service").unwrap();
    (pending, reply)
}
fn progress_queued_local_reply(
    provider: &mut Worker,
    caller: &mut Worker,
    reply: &ReplyReceiver,
) -> Result<(Vec<u8>, bool), Error> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        provider.send_replies();
        caller.receive_responses();
        match reply.try_recv() {
            Ok(result) => return result,
            Err(mpsc::TryRecvError::Empty) => {}
            Err(error) => panic!("reply disconnected: {error:?}"),
        }
        assert!(
            Instant::now() < deadline,
            "queued local reply was not routed"
        );
        thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn admitted_service_queue_preserves_provider_reply_route_and_single_handoff() {
    let cfg = callback_config(4);
    let (mut provider, _commands) = service_state(cfg.clone());
    let node = service_registry_node(&provider);
    let service = node.serve("/service", "req", "rep").unwrap();
    let (mut caller, _) = service_state(cfg);
    let (request, reply) = queued_local_pending(&node, Instant::now() + Duration::from_secs(2));
    caller.command(Command::Request(request));
    assert!(provider.services.is_empty());
    assert!(caller.peers.is_empty());
    assert!(
        caller
            .service_provider_info(&node.topic("/service").unwrap())
            .is_empty()
    );
    assert!(caller.connected.is_empty());
    assert_eq!(caller.pending.len(), 1);
    assert!(caller.pending.values().all(|pending| pending.sent));
    let incoming = service.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(incoming.data, [1]);
    let id = caller.pending.keys().next().unwrap();
    assert_eq!(
        incoming.route,
        [
            caller.response_address.as_bytes().to_vec(),
            caller.response_id.as_bytes().to_vec(),
            node.topic("/service").unwrap().as_bytes().to_vec(),
            caller.node.as_bytes().to_vec(),
            id.as_bytes().to_vec()
        ]
    );
    assert!(Arc::ptr_eq(
        &incoming.sender.identity,
        &provider.sender.identity
    ));
    caller.send_requests();
    assert!(caller.connected.is_empty());
    assert!(matches!(
        service.rx.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    incoming.respond(vec![9], false).unwrap();
    assert!(caller.response_rx.try_recv().is_err());
    provider.command(
        provider
            .response_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap(),
    );
    assert_eq!(
        progress_queued_local_reply(&mut provider, &mut caller, &reply),
        Ok((vec![9], false))
    );
    assert!(caller.pending.is_empty());
    assert!(provider.replies.is_empty());
    assert!(matches!(
        service.rx.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
}
#[test]
fn admitted_service_queue_keeps_provider_backpressure_and_weak_retirement() {
    let cfg = callback_config(1);
    let (mut provider, commands) = service_state(cfg.clone());
    let node = service_registry_node(&provider);
    let service = node.serve("/service", "req", EMPTY_TYPE).unwrap();
    let retained_owner = provider.sender.identity.clone();
    let weak_queue = Arc::downgrade(&service._queue);
    let (mut caller, _) = service_state(cfg.clone());
    let (mut first, first_reply) =
        queued_local_pending(&node, Instant::now() + Duration::from_secs(2));
    first.response_type = EMPTY_TYPE.into();
    caller.command(Command::Request(first));
    assert_eq!(
        first_reply.recv_timeout(Duration::from_secs(1)).unwrap(),
        Ok((vec![], true))
    );
    assert!(caller.pending.is_empty());
    let (mut second, second_reply) =
        queued_local_pending(&node, Instant::now() + Duration::from_secs(2));
    second.response_type = EMPTY_TYPE.into();
    caller.command(Command::Request(second));
    assert_eq!(
        second_reply.recv_timeout(Duration::from_secs(1)).unwrap(),
        Err(Error::QueueFull)
    );
    assert_eq!(provider.sender.counters.snapshot().queue_drops, 1);
    assert_eq!(caller.sender.counters.snapshot().queue_drops, 0);
    let incoming = service.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(!incoming.expects_response());
    assert_eq!(incoming.respond(vec![], true), Ok(()));
    let (mut request, _) = queued_local_pending(&node, Instant::now() + Duration::from_secs(2));
    request.response_type = EMPTY_TYPE.into();
    let index = Mutex::new(vec![Arc::downgrade(&retained_owner)]);
    provider.command(commands.recv_timeout(Duration::from_secs(1)).unwrap());
    node.unadvertise_service("/service").unwrap();
    provider.cleanup();
    assert_eq!(
        service.recv_timeout(Duration::from_millis(20)).err(),
        Some(Error::Shutdown)
    );
    assert!(service._queue.tx.upgrade().is_none());
    assert!(
        !send_admitted_service_request(
            &index,
            &caller.sender,
            &caller.owner_dead,
            "retired",
            &request
        )
        .unwrap()
    );
    drop(service);
    assert!(weak_queue.upgrade().is_none());
    assert!(
        !send_admitted_service_request(
            &index,
            &caller.sender,
            &caller.owner_dead,
            "retired",
            &request
        )
        .unwrap()
    );
    drop(node);
    assert!(
        retained_owner
            .get()
            .unwrap()
            .stopped
            .upgrade()
            .unwrap()
            .load(Ordering::Acquire)
    );
}
#[test]
fn admitted_service_queue_preserves_deadlines_abandonment_and_late_wire_reply() {
    let cfg = callback_config(4);
    let (mut provider, _commands) = service_state(cfg.clone());
    let node = service_registry_node(&provider);
    let service = node.serve("/service", "req", "rep").unwrap();
    let (mut caller, _) = service_state(cfg);
    let (request, reply) = queued_local_pending(&node, Instant::now() + Duration::from_millis(20));
    caller.command(Command::Request(request));
    let incoming = service.recv_timeout(Duration::from_secs(1)).unwrap();
    thread::sleep(Duration::from_millis(30));
    caller.send_requests();
    assert_eq!(
        reply.recv_timeout(Duration::from_secs(1)).unwrap(),
        Err(Error::Timeout)
    );
    incoming.respond(vec![9], true).unwrap();
    provider.command(
        provider
            .response_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(1);
    while !provider.replies.is_empty() {
        provider.send_replies();
        caller.receive_responses();
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
    caller.receive_responses();
    assert!(caller.pending.is_empty());
    let (request, reply) = queued_local_pending(&node, Instant::now() + Duration::from_secs(1));
    caller.command(Command::Request(request));
    drop(reply);
    let incoming = service.recv_timeout(Duration::from_secs(1)).unwrap();
    incoming.respond(vec![7], true).unwrap();
    provider.command(
        provider
            .response_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(1);
    while !caller.pending.is_empty() {
        provider.send_replies();
        caller.receive_responses();
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
    assert!(provider.replies.is_empty());
}

#[test]
fn admitted_service_queue_obeys_domains_payloads_and_final_live_guards() {
    let mut cfg = callback_config(4);
    cfg.max_message_bytes = 1;
    let (provider, commands) = service_state(cfg.clone());
    let node = service_registry_node(&provider);
    let service = node.serve("/service", "req", "rep").unwrap();
    let target = service._queue.clone();
    let mut caller_cfg = cfg.clone();
    caller_cfg.max_message_bytes = 3;
    let (caller, _) = service_state(caller_cfg);
    let index = Mutex::new(vec![Arc::downgrade(&provider.sender.identity)]);
    let (mut request, _) = queued_local_pending(&node, Instant::now() + Duration::from_secs(2));
    request.data = vec![1, 2];
    assert_eq!(
        send_admitted_service_request(
            &index,
            &caller.sender,
            &caller.owner_dead,
            "large",
            &request
        ),
        Err(Error::InvalidFrame)
    );
    assert!(service.rx.try_recv().is_err());
    request.data = vec![1];
    for field in [0, 1, 2] {
        let (mut mismatch, _) = queued_local_pending(&node, request.deadline);
        match field {
            0 => mismatch.topic.push_str("-absent"),
            1 => mismatch.request_type.push_str("-wrong"),
            _ => mismatch.response_type.push_str("-wrong"),
        }
        assert!(
            !send_admitted_service_request(
                &index,
                &caller.sender,
                &caller.owner_dead,
                "mismatch",
                &mismatch
            )
            .unwrap()
        );
    }
    for field in [0, 1, 2, 3] {
        let mut isolated = cfg.clone();
        match field {
            0 => isolated.partition.push_str("-other"),
            1 => isolated.service_port += 31,
            2 => isolated.multicast = "239.255.0.8".parse().unwrap(),
            _ => isolated.interface = "127.0.0.2".parse().unwrap(),
        }
        let (isolated, _) = service_state(isolated);
        assert!(
            !send_admitted_service_request(
                &index,
                &isolated.sender,
                &isolated.owner_dead,
                "domain",
                &request
            )
            .unwrap()
        );
    }
    cfg.message_port += 33;
    let (same_service, _) = service_state(cfg);
    assert!(
        send_admitted_service_request(
            &index,
            &same_service.sender,
            &same_service.owner_dead,
            "same-domain",
            &request
        )
        .unwrap()
    );
    assert_eq!(service.rx.try_recv().unwrap().data, [1]);
    caller.owner_dead.store(true, Ordering::Release);
    assert_eq!(
        target.send(&caller.sender, &caller.owner_dead, "dead-caller", &request),
        Err(Error::Shutdown)
    );
    caller.owner_dead.store(false, Ordering::Release);
    caller.sender.stop.store(true, Ordering::Release);
    assert_eq!(
        target.send(
            &caller.sender,
            &caller.owner_dead,
            "stopped-caller",
            &request
        ),
        Err(Error::Shutdown)
    );
    caller.sender.stop.store(false, Ordering::Release);
    let (expired, _) = queued_local_pending(&node, Instant::now());
    assert_eq!(
        target.send(&caller.sender, &caller.owner_dead, "expired", &expired),
        Err(Error::Timeout)
    );
    provider.owner_dead.store(true, Ordering::Release);
    assert!(
        !target
            .send(&caller.sender, &caller.owner_dead, "dead-owner", &request)
            .unwrap()
    );
    provider.owner_dead.store(false, Ordering::Release);
    provider.sender.stop.store(true, Ordering::Release);
    assert!(
        !target
            .send(
                &caller.sender,
                &caller.owner_dead,
                "stopped-owner",
                &request
            )
            .unwrap()
    );
    assert!(
        !send_admitted_service_request(
            &index,
            &caller.sender,
            &caller.owner_dead,
            "stopped-owner",
            &request
        )
        .unwrap()
    );
    provider.sender.stop.store(false, Ordering::Release);
    drop(commands.recv_timeout(Duration::from_secs(1)).unwrap());
    assert!(target.tx.upgrade().is_none());
    assert!(
        !target
            .send(
                &caller.sender,
                &caller.owner_dead,
                "closed-channel",
                &request
            )
            .unwrap()
    );
    assert_eq!(
        service.recv_timeout(Duration::from_millis(20)).err(),
        Some(Error::Shutdown)
    );
    service._lease.active.store(false, Ordering::Release);
    assert!(
        !target
            .send(&caller.sender, &caller.owner_dead, "retired", &request)
            .unwrap()
    );
    drop(service);
    assert!(target.active.upgrade().is_none());
    assert!(
        !target
            .send(
                &caller.sender,
                &caller.owner_dead,
                "expired-lease",
                &request
            )
            .unwrap()
    );
}
#[test]
fn admitted_service_queue_preserves_own_receiver_priority_and_poison_errors() {
    let cfg = callback_config(1);
    let (foreign, _foreign_commands) = service_state(cfg.clone());
    let foreign_node = service_registry_node(&foreign);
    let foreign_queue = foreign_node.serve("/service", "req", "rep").unwrap();
    let (mut caller, commands) = service_state(cfg);
    let node = service_registry_node(&caller);
    let own_queue = node.serve("/service", "req", "rep").unwrap();
    let (request, _) = queued_local_pending(&node, Instant::now() + Duration::from_secs(2));
    assert!(
        send_local_request(
            &caller.services,
            &caller.sender,
            &caller.owner_dead,
            caller.config.max_message_bytes,
            "own",
            &request
        )
        .unwrap()
    );
    assert!(foreign_queue.rx.try_recv().is_err());
    assert_eq!(
        send_local_request(
            &caller.services,
            &caller.sender,
            &caller.owner_dead,
            caller.config.max_message_bytes,
            "full",
            &request
        ),
        Err(Error::QueueFull)
    );
    assert!(foreign_queue.rx.try_recv().is_err());
    assert_eq!(own_queue.rx.try_recv().unwrap().route.len(), 5);
    caller.command(commands.recv_timeout(Duration::from_secs(1)).unwrap());
    assert!(
        send_local_request(
            &caller.services,
            &caller.sender,
            &caller.owner_dead,
            caller.config.max_message_bytes,
            "processed",
            &request
        )
        .unwrap()
    );
    assert_eq!(
        own_queue.rx.try_recv().unwrap().route,
        [b"processed".to_vec()]
    );
    drop(own_queue);
    let index = Mutex::new(vec![Arc::downgrade(&foreign.sender.identity)]);
    thread::scope(|scope| {
        assert!(
            scope
                .spawn(|| {
                    let _guard = index.lock().unwrap();
                    panic!("isolated queue index poison");
                })
                .join()
                .is_err()
        );
    });
    assert!(matches!(
        send_admitted_service_request(
            &index,
            &caller.sender,
            &caller.owner_dead,
            "poison",
            &request
        ),
        Err(Error::Native(_))
    ));
    thread::scope(|scope| {
        assert!(
            scope
                .spawn(|| {
                    let _guard = foreign_node.services.lock().unwrap();
                    panic!("isolated queue registry poison");
                })
                .join()
                .is_err()
        );
    });
    let index = Mutex::new(vec![Arc::downgrade(&foreign.sender.identity)]);
    assert!(matches!(
        send_admitted_service_request(
            &index,
            &caller.sender,
            &caller.owner_dead,
            "poison",
            &request
        ),
        Err(Error::Native(_))
    ));
}

#[test]
fn admitted_service_queue_inflight_sender_lease_does_not_reopen_a_withdrawn_service() {
    let cfg = callback_config(2);
    let (mut provider, commands) = service_state(cfg.clone());
    let node = service_registry_node(&provider);
    let service = node.serve("/service", "req", "rep").unwrap();
    provider.command(commands.recv_timeout(Duration::from_secs(1)).unwrap());
    let queue = service._queue.clone();
    // Pause the actual production path after Weak::upgrade and before its
    // final live checks. This temporary strong sender can delay disconnect.
    let inflight = queue.tx.upgrade().unwrap();
    let (caller, _) = service_state(cfg);
    let (request, _) = queued_local_pending(&node, Instant::now() + Duration::from_secs(1));
    node.unadvertise_service("/service").unwrap();
    provider.cleanup();
    assert!(matches!(
        service.rx.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    let index = Mutex::new(vec![Arc::downgrade(&provider.sender.identity)]);
    assert!(
        !send_admitted_service_request(&index, &caller.sender, &caller.owner_dead, "new", &request)
            .unwrap()
    );
    assert!(
        !queue
            .send_on(
                inflight,
                &caller.sender,
                &caller.owner_dead,
                "inflight",
                &request
            )
            .unwrap()
    );
    assert_eq!(
        service.recv_timeout(Duration::from_millis(20)).err(),
        Some(Error::Shutdown)
    );
    assert!(queue.tx.upgrade().is_none());
}

fn queued_spawn_failure_after_hook(task: CallbackTask) -> std::io::Result<JoinHandle<()>> {
    let hook = ONEWAY_SPAWN_HOOK.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook();
    }
    callback_spawn_failure(task)
}
#[test]
fn admitted_service_queue_does_not_execute_a_failed_callback_registration() {
    let cfg = callback_config(2);
    let provider = Arc::new(Node::new(cfg.clone()).unwrap());
    let caller = Arc::new(Node::new(cfg).unwrap());
    let (saved, received) = mpsc::sync_channel(1);
    let weak = Arc::downgrade(&provider);
    let client = caller.clone();
    ONEWAY_SPAWN_HOOK.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || {
            let provider = weak.upgrade().unwrap();
            let pending = client
                .begin_request("/race", "req", "rep", vec![1], Duration::from_secs(2))
                .unwrap();
            // Bounded command ordering only: no discovery or expected-data poll.
            let (barrier, barrier_reply) = mpsc::sync_channel(1);
            let deadline = Instant::now() + Duration::from_secs(1);
            loop {
                match client.sender.send(Command::KnownServices(barrier.clone())) {
                    Ok(()) => break,
                    Err(Error::QueueFull) => {
                        assert!(Instant::now() < deadline);
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("queue barrier failed: {error:?}"),
                }
            }
            barrier_reply
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
            let queue = provider.services.lock().unwrap()[0].queue.clone();
            let target = queue.upgrade().unwrap();
            let (mut probe, _) =
                queued_local_pending(&provider, Instant::now() + Duration::from_secs(1));
            probe.topic = provider.topic("/race").unwrap();
            // One first request occupies the two-slot queue. Fill the second and
            // verify a third cannot enter before the failed spawner drops Service.
            assert!(
                target
                    .send(&client.sender, &client.owner_dead, "probe", &probe)
                    .unwrap()
            );
            assert_eq!(
                target.send(&client.sender, &client.owner_dead, "full", &probe),
                Err(Error::QueueFull)
            );
            saved.send((pending, queue)).unwrap();
        }))
    });
    let entries = Arc::new(AtomicUsize::new(0));
    let calls = entries.clone();
    assert!(matches!(
        provider.serve_callback_with_spawner(
            "/race",
            "req",
            "rep",
            move |_| {
                calls.fetch_add(1, Ordering::AcqRel);
                Ok((vec![], true))
            },
            queued_spawn_failure_after_hook
        ),
        Err(Error::Native(_))
    ));
    let (pending, queue) = received.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(queue.upgrade().is_none());
    assert_eq!(pending.wait(Duration::from_secs(3)), Err(Error::Timeout));
    assert_eq!(entries.load(Ordering::Acquire), 0);
    assert_eq!(provider.sender.slots.load(Ordering::Acquire), 0);
    assert_eq!(provider.sender.callback_threads.load(Ordering::Acquire), 0);
    assert!(provider.advertised_services().unwrap().is_empty());
}

#[test]
fn service_scope_controls_preserve_initial_subscribe_and_withdrawal_policy() {
    for scope in [Scope::All, Scope::Host, Scope::Process] {
        let (mut worker, _commands) = state(4);
        let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        receiver
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        worker.config.multicast = Ipv4Addr::LOCALHOST;
        worker.config.service_port = receiver.local_addr().unwrap().port();
        let active = Arc::new(AtomicBool::new(true));
        let (tx, _requests) = mpsc::sync_channel(1);
        worker.command(Command::Serve(
            "/scope".into(),
            "req".into(),
            "rep".into(),
            scope,
            Arc::new(tx),
            active.clone(),
        ));
        let mut bytes = vec![0; protocol::MAX_DATAGRAM];
        if scope == Scope::Process {
            assert!(receiver.recv(&mut bytes).is_err());
        } else {
            let n = receiver.recv(&mut bytes).unwrap();
            let msg = protocol::decode(&bytes[..n]).unwrap();
            assert_eq!(msg.r#type, Type::Advertise as i32);
            let Some(DiscContents::Pub(p)) = msg.disc_contents else {
                panic!()
            };
            assert_eq!(p.scope, scope as i32);
        }
        for (source, local) in [
            (Ipv4Addr::LOCALHOST, true),
            (Ipv4Addr::new(198, 51, 100, 2), false),
        ] {
            worker.discovery(
                1,
                protocol::discovery(
                    "requester",
                    Type::Subscribe,
                    Some(DiscContents::Sub(Subscriber {
                        topic: "/scope".into(),
                    })),
                ),
                source.into(),
            );
            if scope == Scope::Process || (scope == Scope::Host && !local) {
                assert!(receiver.recv(&mut bytes).is_err());
            } else {
                let n = receiver.recv(&mut bytes).unwrap();
                let msg = protocol::decode(&bytes[..n]).unwrap();
                assert_eq!(msg.r#type, Type::Advertise as i32);
                let Some(DiscContents::Pub(p)) = msg.disc_contents else {
                    panic!()
                };
                assert_eq!(p.scope, scope as i32);
            }
        }
        active.store(false, Ordering::Release);
        worker.cleanup();
        if scope == Scope::Process {
            assert!(receiver.recv(&mut bytes).is_err());
        } else {
            let n = receiver.recv(&mut bytes).unwrap();
            let msg = protocol::decode(&bytes[..n]).unwrap();
            assert_eq!(msg.r#type, Type::Unadvertise as i32);
            let Some(DiscContents::Pub(p)) = msg.disc_contents else {
                panic!()
            };
            assert_eq!(p.scope, scope as i32);
        }
        assert!(worker.services.is_empty());
    }
}

#[test]
fn scoped_service_options_preserve_duplicate_cancel_and_spawn_rollback() {
    let (mut worker, commands) = state(2);
    let node = service_registry_node(&worker);
    let mut options = ServiceOptions::default();
    options.set_scope(Scope::Process);
    assert!(matches!(
        node.serve_with_options("bad name", "req", "rep", options),
        Err(Error::InvalidName)
    ));
    let service = node
        .serve_with_options("/scope", "req", "rep", options)
        .unwrap();
    assert!(matches!(
        node.serve_with_options("/scope", "other", "other", ServiceOptions::default()),
        Err(Error::AlreadyAdvertised)
    ));
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 1);
    worker.command(commands.recv_timeout(Duration::from_secs(1)).unwrap());
    assert_eq!(
        worker.service_provider_info(&node.topic("/scope").unwrap())[0].scope,
        Scope::Process
    );
    drop(service);
    worker.cleanup();
    let calls = Arc::new(AtomicUsize::new(0));
    let entries = calls.clone();
    assert!(matches!(
        node.serve_callback_options_with_spawner(
            "/failed",
            "req",
            "rep",
            options,
            move |_| {
                entries.fetch_add(1, Ordering::AcqRel);
                Ok((vec![], true))
            },
            callback_spawn_failure
        ),
        Err(Error::Native(_))
    ));
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 0);
    worker.command(commands.recv_timeout(Duration::from_secs(1)).unwrap());
    assert!(worker.services.is_empty());
    let entries = calls.clone();
    assert!(matches!(
        node.serve_oneway_options_with_spawner(
            "/failed",
            "req",
            options,
            move |_| {
                entries.fetch_add(1, Ordering::AcqRel);
                Ok(())
            },
            callback_spawn_failure
        ),
        Err(Error::Native(_))
    ));
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 0);
    worker.command(commands.recv_timeout(Duration::from_secs(1)).unwrap());
    assert!(worker.services.is_empty());
    assert_eq!(calls.load(Ordering::Acquire), 0);
    node.cancel();
    assert!(matches!(
        node.serve_with_options("/stopped", "req", "rep", options),
        Err(Error::Shutdown)
    ));
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
}

#[test]
fn statistics_wire_metadata_errors_are_observable_without_losing_local_delivery() {
    let reserved = [
        UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap(),
        UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap(),
    ];
    let [message_port, service_port] = reserved.each_ref().map(|s| s.local_addr().unwrap().port());
    assert_ne!(message_port, service_port);
    drop(reserved);
    let (mut worker, _commands) = service_state(Config {
        statistics_wire: true,
        capacity: 4,
        partition: Uuid::new_v4().to_string(),
        message_port,
        service_port,
        ..Default::default()
    });
    let topic = qualified_name(&worker.config.partition, "", "/metadata-errors").unwrap();
    let (tx, local) = mpsc::sync_channel(1);
    worker.command(Command::Subscribe(
        topic.clone(),
        "type".into(),
        tx.into(),
        Arc::new(AtomicBool::new(true)),
        SubscribeOptions::default(),
        None,
    ));
    let mut registration = worker.wire_publisher(
        topic.clone(),
        PubType::MsgPub(MessagePublisher {
            msg_type: "type".into(),
            ctrl: worker.process.clone(),
            ..Default::default()
        }),
    );
    registration.process_uuid = "registered-peer".into();
    registration.node_uuid = "registered-node".into();
    let mut message = protocol::discovery(
        "registered-peer",
        Type::NewConnection,
        Some(DiscContents::Pub(registration)),
    );
    message.version = 110;
    worker.discovered(
        0,
        protocol::decode_version(&protocol::encode(&message).unwrap(), 110).unwrap(),
    );
    assert_eq!(worker.remote_subscribers.len(), 1);
    assert!(worker.has_remote_subscribers(&topic, "type"));
    let context = zmq::Context::new();
    let publisher = context.socket(zmq::PAIR).unwrap();
    let observer = context.socket(zmq::PAIR).unwrap();
    publisher.set_linger(0).unwrap();
    observer.set_linger(0).unwrap();
    let address = format!("inproc://metadata-errors-{}", Uuid::new_v4());
    publisher.bind(&address).unwrap();
    observer.connect(&address).unwrap();
    worker.publisher = publisher;
    // This private invalid command has no admitted sequence. No timestamp or
    // packet may be invented, but existing local delivery still completes.
    worker.command(Command::Publish(
        topic.clone(),
        "type".into(),
        vec![5],
        None,
    ));
    assert_eq!(worker.sender.counters.snapshot().send_failures, 1);
    assert_eq!(
        observer.recv_msg(zmq::DONTWAIT).unwrap_err(),
        zmq::Error::EAGAIN
    );
    assert_eq!(
        local.recv_timeout(Duration::from_secs(1)).unwrap().data,
        vec![5]
    );
    assert!(matches!(local.try_recv(), Err(mpsc::TryRecvError::Empty)));
    // An unconnected PAIR deterministically rejects a nonblocking send. The
    // bounded Rust policy reserves one sequence before that multipart attempt.
    worker.publisher = context.socket(zmq::PAIR).unwrap();
    worker.publisher.set_linger(0).unwrap();
    let sequence = AtomicU64::new(17);
    worker.send_publication_wire(&topic, "type", &[6], Some(&sequence));
    assert_eq!(sequence.load(Ordering::Relaxed), 18);
    assert_eq!(worker.sender.counters.snapshot().send_failures, 2);
    worker.remote_subscribers.clear();
    worker.send_publication_wire(&topic, "type", &[7], Some(&sequence));
    worker.send_publication_wire(&topic, "type", &[8], None);
    assert_eq!(sequence.load(Ordering::Relaxed), 18);
    let counters = worker.sender.counters.snapshot();
    assert_eq!(counters.send_failures, 2);
    assert_eq!(counters.queue_drops, 0);
    assert_eq!(counters.malformed_frames, 0);
}
