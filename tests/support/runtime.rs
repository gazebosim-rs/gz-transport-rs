// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// TopicUtils assertion tables: Copyright (C) 2014 Open Source Robotics Foundation.
use super::*;
trait TestDiscovery {
    fn discovered(&mut self, channel: usize, message: Discovery);
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
        old.clone(),
    ));
    old.store(false, Ordering::Release);
    let retired = Arc::new(AtomicBool::new(false));
    worker.command(Command::Advertise(
        "/topic".into(),
        "never-advertised".into(),
        retired,
    ));
    worker.command(Command::Advertise(
        "/topic".into(),
        "new".into(),
        Arc::new(AtomicBool::new(true)),
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
        worker: None,
        config: worker.config.clone(),
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: Mutex::new(Vec::new()),
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
fn service_withdrawal_releases_capacity_once_and_rolls_back_failed_admission() {
    let (mut worker, commands) = state(1);
    let node = Node {
        sender: worker.sender.clone(),
        worker: None,
        config: worker.config.clone(),
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: Mutex::new(Vec::new()),
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
        worker: None,
        config: worker.config.clone(),
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: Mutex::new(Vec::new()),
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
    worker.command(commands.recv().unwrap());
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
        worker: None,
        config: worker.config.clone(),
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: Mutex::new(Vec::new()),
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
        worker: None,
        config: worker.config.clone(),
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: Mutex::new(Vec::new()),
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
    let config = Config {
        capacity,
        partition: Uuid::new_v4().to_string(),
        ..Default::default()
    };
    let (tx, rx) = mpsc::sync_channel(capacity);
    let sender = Sender {
        tx,
        stop: Arc::new(AtomicBool::new(false)),
        slots: Arc::new(AtomicUsize::new(0)),
        capacity,
        counters: Arc::new(Counters::default()),
    };
    (Worker::new(config, sender).unwrap(), rx)
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
        .send(Command::Publish("a".into(), "b".into(), vec![]))
        .unwrap();
    assert_eq!(
        sender.send(Command::Publish("a".into(), "b".into(), vec![])),
        Err(Error::QueueFull)
    );
    drop(rx);
    assert_eq!(
        sender.send(Command::Publish("a".into(), "b".into(), vec![])),
        Err(Error::Shutdown)
    );
    sender.stop.store(true, Ordering::Release);
    assert_eq!(
        sender.send(Command::Publish("a".into(), "b".into(), vec![])),
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
    };
    assert_eq!(request.respond(vec![0; 2], true), Err(Error::InvalidFrame));
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
        tx,
        active.clone(),
    ));
    assert_eq!(worker.subscriptions_connected.len(), 1);
    worker.command(Command::Advertise(
        "/topic".into(),
        "type".into(),
        active.clone(),
    ));
    let (tx, _) = mpsc::sync_channel(1);
    worker.command(Command::Serve(
        "/service".into(),
        "req".into(),
        "rep".into(),
        tx,
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
        tx,
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
fn service_frames_validate_type_node_topic_and_request_correlation() {
    let (mut worker, _) = state(10);
    let active = Arc::new(AtomicBool::new(true));
    let (tx, rx) = mpsc::sync_channel(1);
    worker.command(Command::Serve(
        "/service".into(),
        "req".into(),
        "rep".into(),
        tx,
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
        tx,
        active,
    ));
    let message = Message {
        topic: "topic".into(),
        type_name: "type".into(),
        data: vec![1],
    };
    worker.deliver(message.clone());
    worker.deliver(message.clone());
    assert_eq!(rx.try_recv().unwrap(), message);
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
        tx,
        active: Arc::new(AtomicBool::new(true)),
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
        tx,
        active: old,
    });
    let (tx, rx) = mpsc::sync_channel(1);
    worker.subscriptions.push(SubscriberState {
        topic: "topic".into(),
        type_name: "type".into(),
        tx,
        active: Arc::new(AtomicBool::new(true)),
    });
    worker.cleanup();
    assert_eq!(worker.subscriptions.len(), 1);
    worker.deliver(Message {
        topic: "topic".into(),
        type_name: "type".into(),
        data: vec![7],
    });
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
        tx,
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
            tx,
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
            tx,
            active,
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
            tx,
            active,
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
        tx,
        Arc::new(AtomicBool::new(true)),
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
        tx,
        Arc::new(AtomicBool::new(true)),
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
        requests,
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
        worker: None,
        config: Config {
            reply_timeout: Duration::from_millis(5),
            ..worker.config.clone()
        },
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: Mutex::new(Vec::new()),
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
        worker: None,
        config,
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: Mutex::new(Vec::new()),
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
        worker: None,
        config: long,
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: Mutex::new(Vec::new()),
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
        tx,
        active.clone(),
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
        worker: None,
        config: Config {
            reply_timeout: Duration::from_millis(5),
            ..worker.config.clone()
        },
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: Mutex::new(Vec::new()),
    };
    let publisher = node.advertise("/topic", "T").unwrap();
    assert_eq!(publisher.has_connections(), Err(Error::QueueFull));
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
    assert_eq!(publisher.has_connections(), Err(Error::Shutdown));
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 1);
    drop(publisher);
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert_eq!(node.statistics().queue_drops, 2);
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
            tx,
            active.clone(),
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
fn local_topic_inventory_tracks_live_leases_bounds_and_failed_admission() {
    let (mut worker, commands) = state(3);
    let mut config = worker.config.clone();
    config.namespace = "ns".into();
    config.add_topic_remap("alias", "mapped").unwrap();
    let node = Node {
        sender: worker.sender.clone(),
        worker: None,
        config,
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: Mutex::new(Vec::new()),
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
        tx,
        old.clone(),
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
        tx,
        Arc::new(AtomicBool::new(true)),
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
        worker: None,
        config: worker.config.clone(),
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: Mutex::new(Vec::new()),
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
            tx,
            flag.clone(),
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
        worker: None,
        config: Config {
            reply_timeout: Duration::from_millis(5),
            ..worker.config.clone()
        },
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: Mutex::new(Vec::new()),
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
        worker: None,
        config: worker.config.clone(),
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: Mutex::new(Vec::new()),
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
    worker.deliver(Message {
        topic: topic.clone(),
        type_name: "old".into(),
        data: vec![1],
    });
    node.unsubscribe("/topic").unwrap();
    worker.deliver(Message {
        topic: topic.clone(),
        type_name: "old".into(),
        data: vec![2],
    });
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
    worker.deliver(Message {
        topic: topic.clone(),
        type_name: "new".into(),
        data: vec![3],
    });
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
        worker: None,
        config: worker.config.clone(),
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: Mutex::new(Vec::new()),
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
        worker: None,
        config: worker.config.clone(),
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: Mutex::new(Vec::new()),
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
    let Command::Publish(_, name, bytes) = commands.recv().unwrap() else {
        panic!("missing typed publication")
    };
    assert_eq!(name, "gz.msgs.Int32");
    assert_eq!(Int32::decode(bytes.as_slice()).unwrap().data, 1);
    node.cancel();
    assert_eq!(publisher.publish_typed(&right), Err(Error::Shutdown));
    let (worker, _commands) = state(1);
    let node = Node {
        sender: worker.sender.clone(),
        worker: None,
        config: Config {
            max_message_bytes: 1,
            ..worker.config.clone()
        },
        topics: Mutex::new(HashMap::new()),
        subscriptions: Mutex::new(Vec::new()),
        services: Mutex::new(Vec::new()),
    };
    let publisher = node.advertise_typed::<Int32>("/size").unwrap();
    assert_eq!(publisher.publish_typed(&right), Err(Error::InvalidFrame));
    assert_eq!(node.statistics().queue_drops, 0);
}
