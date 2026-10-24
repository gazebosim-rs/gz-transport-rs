use super::*;

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
        route: vec![],
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
    worker.discovery(
        0,
        protocol::advertisement(&worker.process, publisher.clone()),
    );
    assert!(worker.peers.is_empty());
    let mut process = publisher.clone();
    process.scope = Scope::Process.into();
    worker.discovery(0, protocol::advertisement("remote", process));
    assert!(worker.peers.is_empty());
    worker.discovery(0, advertise.clone());
    assert_eq!(worker.peers.len(), 1);
    worker.discovery(0, protocol::discovery("remote", Type::Heartbeat, None));
    worker.discovery(
        0,
        protocol::discovery(
            "remote",
            Type::Unadvertise,
            Some(DiscContents::Pub(publisher.clone())),
        ),
    );
    assert!(worker.peers.is_empty());
    worker.discovery(0, advertise.clone());
    worker.discovery(0, protocol::discovery("remote", Type::Bye, None));
    assert!(worker.peers.is_empty());
    worker.discovery(0, advertise);
    for i in 0..10 {
        let mut p = publisher.clone();
        p.node_uuid = i.to_string();
        worker.discovery(0, protocol::advertisement("remote", p));
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
            worker.discovery(
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
    assert!(receive(&receiver, 4, &Counters::default()).is_none());
    for count in [1, 3, 5, 1000] {
        sender.send_multipart(vec!["x"; count], 0).unwrap();
        assert!(receive(&receiver, 4, &Counters::default()).is_none());
    }
    sender.send_multipart(["a", "b", "c", "d"], 0).unwrap();
    assert_eq!(
        receive(&receiver, 4, &Counters::default()).unwrap(),
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
        node.advertise("topic", "type"),
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
    assert!(receive(&receiver, 4, &counters).is_none());
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
    worker.discovery(1, protocol::advertisement("remote", service));
    assert!(worker.peers.is_empty());
}
