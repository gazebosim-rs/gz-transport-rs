// Copyright (C) 2023, 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Complete CallbackScope setup is distinct from positive opaque/lifecycle controls.
use gz_msgs::{
    msgs::{Int32, Vector3d},
    prost::Message as _,
};
use gz_transport::{Config, Node, Publisher, SubscribeOptions};
use std::{
    net::UdpSocket,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
fn config() -> Config {
    let sockets = [
        UdpSocket::bind(("127.0.0.1", 0)).unwrap(),
        UdpSocket::bind(("127.0.0.1", 0)).unwrap(),
    ];
    let ports = sockets
        .each_ref()
        .map(|socket| socket.local_addr().unwrap().port());
    assert_ne!(ports[0], ports[1]);
    drop(sockets);
    Config {
        partition: format!("raw-callback-{}", uuid::Uuid::new_v4()),
        message_port: ports[0],
        service_port: ports[1],
        ..Default::default()
    }
}
fn ready(publisher: &Publisher) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !publisher.has_connections().unwrap() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn opaque_callback_preserves_payloads_and_actual_metadata() {
    let settings = config();
    let partition = format!("/{}", settings.partition);
    let node = Node::new(settings).unwrap();
    let publisher = node.advertise("/opaque", "opaque.future.Type").unwrap();
    let (done, observed) = mpsc::channel();
    let caller = thread::current().id();
    let _callback = node
        .subscribe_raw_callback("/opaque", "", move |data, info| {
            done.send((
                data.to_vec(),
                info.topic.to_owned(),
                info.partition.to_owned(),
                info.type_name.to_owned(),
                info.intra_process,
                thread::current().id(),
            ))
            .unwrap();
            Ok(())
        })
        .unwrap();
    ready(&publisher);
    for data in [
        vec![],
        vec![0xff],
        (0..=255).collect(),
        vec![0x10, 5, 0xf8, 7, 1],
    ] {
        publisher.publish(data.clone()).unwrap();
        let actual = observed.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(actual.0, data);
        assert_eq!(actual.1, "/opaque");
        assert_eq!(actual.2, partition);
        assert_eq!(actual.3, "opaque.future.Type");
        assert!(actual.4);
        assert_ne!(actual.5, caller);
    }
    assert_eq!(node.statistics().routing_errors, 0);
}
#[test]
fn opaque_and_typed_registration_share_one_dispatcher_with_exact_typed_wire() {
    let node = Node::new(config()).unwrap();
    let publisher = node.advertise_typed::<Int32>("/mixed").unwrap();
    let (done, observed) = mpsc::channel();
    let typed_done = done.clone();
    let _typed = node
        .subscribe_typed_callback::<Int32, _>("/mixed", move |value, _| {
            assert_eq!(value.data, 5);
            typed_done.send((true, thread::current().id())).unwrap();
            Ok(())
        })
        .unwrap();
    let expected = Int32 {
        data: 5,
        ..Default::default()
    }
    .encode_to_vec();
    let _raw = node
        .subscribe_raw_callback("/mixed", "gz.msgs.Int32", move |data, info| {
            assert_eq!(data, expected);
            assert_eq!(info.type_name, "gz.msgs.Int32");
            done.send((false, thread::current().id())).unwrap();
            Ok(())
        })
        .unwrap();
    ready(&publisher);
    publisher
        .publish_typed(&Int32 {
            data: 5,
            ..Default::default()
        })
        .unwrap();
    let first = observed.recv_timeout(Duration::from_secs(1)).unwrap();
    let second = observed.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_ne!(first.0, second.0);
    assert_eq!(first.1, second.1);
    assert_ne!(first.1, thread::current().id());
}
#[test]
fn raw_type_hint_and_ignore_local_filter_before_user_entry() {
    let settings = config();
    let node = Node::new(settings.clone()).unwrap();
    let other = Node::new(settings.clone()).unwrap();
    let wrong = Node::new(settings).unwrap();
    let local = node.advertise_typed::<Int32>("/filtered").unwrap();
    let good = other.advertise_typed::<Int32>("/filtered").unwrap();
    let incompatible = wrong.advertise_typed::<Vector3d>("/filtered").unwrap();
    let entries = Arc::new(AtomicUsize::new(0));
    let callback_entries = entries.clone();
    let (done, observed) = mpsc::channel();
    let mut options = SubscribeOptions::default();
    options.set_ignore_local_messages(true);
    let _callback = node
        .subscribe_raw_callback_with_options(
            "/filtered",
            "gz.msgs.Int32",
            options,
            move |data, _| {
                callback_entries.fetch_add(1, Ordering::AcqRel);
                assert_eq!(data, [0xff]);
                done.send(()).unwrap();
                Ok(())
            },
        )
        .unwrap();
    local
        .publish_typed(&Int32 {
            data: 5,
            ..Default::default()
        })
        .unwrap();
    incompatible
        .publish_typed(&Vector3d {
            x: 1.0,
            y: 2.0,
            z: 3.0,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        observed.recv_timeout(Duration::from_millis(300)),
        Err(mpsc::RecvTimeoutError::Timeout)
    );
    assert_eq!(entries.load(Ordering::Acquire), 0);
    ready(&good);
    good.publish(vec![0xff]).unwrap();
    observed.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(entries.load(Ordering::Acquire), 1);
}

fn logical_scope(node: &Node, publisher: &Publisher, raw: bool) {
    let data = Arc::new(Mutex::new(Some(Box::new(Int32::default()))));
    let callback_data = data.clone();
    let invalid = Arc::new(AtomicBool::new(false));
    let callback_invalid = invalid.clone();
    let topic = if raw { "/my_topic_raw" } else { "/my_topic" };
    let callback = if raw {
        node.subscribe_raw_callback(topic, "", move |_, _| {
            if callback_data.lock().unwrap().is_none() {
                callback_invalid.store(true, Ordering::Release);
                panic!("native raw logical lifetime predicate failed");
            }
            Ok(())
        })
        .unwrap()
    } else {
        node.subscribe_typed_callback::<Int32, _>(topic, move |_, _| {
            if callback_data.lock().unwrap().is_none() {
                callback_invalid.store(true, Ordering::Release);
                panic!("native typed logical lifetime predicate failed");
            }
            Ok(())
        })
        .unwrap()
    };
    // Match the original: no readiness barrier or completion wait is added.
    publisher.publish_typed(&Int32::default()).unwrap();
    publisher.publish_typed(&Int32::default()).unwrap();
    node.unsubscribe(topic).unwrap();
    *data.lock().unwrap() = None;
    // Owned Drop is after the logical pointer reset. It cannot mask a null
    // observation by waiting before reset; it only makes that failure visible.
    drop(callback);
    assert!(!invalid.load(Ordering::Acquire));
}
#[test]
fn original_callback_scope_cleanup_correctly() {
    let settings = config();
    let node = Node::new(settings.clone()).unwrap();
    let publisher = node.advertise_typed::<Int32>("/my_topic").unwrap();
    logical_scope(&node, &publisher, false);
    // The first Node and publisher remain alive through the raw phase.
    let raw_node = Node::new(settings).unwrap();
    let raw_publisher = raw_node.advertise_typed::<Int32>("/my_topic_raw").unwrap();
    logical_scope(&raw_node, &raw_publisher, true);
}

#[test]
fn native_selected_handler_is_not_revoked_by_queue_unsubscribe() {
    use std::process::{Command, Stdio};
    let settings = config();
    let path = std::env::var("GZ_RAW_SUBSCRIPTION_PEER").unwrap_or_else(|_| {
        format!(
            "{}/artifacts/raw-subscription-peer",
            env!("CARGO_MANIFEST_DIR")
        )
    });
    let mut child = Command::new(path)
        .env("GZ_PARTITION", &settings.partition)
        .env("GZ_IP", settings.interface.to_string())
        .env("GZ_DISCOVERY_MULTICAST_IP", settings.multicast.to_string())
        .env("GZ_DISCOVERY_MSG_PORT", settings.message_port.to_string())
        .env("GZ_DISCOVERY_SRV_PORT", settings.service_port.to_string())
        .env("GZ_TRANSPORT_IMPLEMENTATION", "zeromq")
        .env("GZ_TRANSPORT_TOPIC_STATISTICS", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("native selected-handler probe timed out");
        }
        thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "typed unsubscribe_before_release=1 result=1 completed=1\nraw unsubscribe_before_release=1 result=1 completed=1\n"
    );
}

#[test]
fn actual_cpp_raw_opaque_and_empty_publications_reach_raw_callback_unchanged() {
    use std::{
        io::{BufRead, BufReader, Read, Write},
        process::{Child, Command, Stdio},
    };
    struct Peer {
        child: Child,
        lines: mpsc::Receiver<String>,
        reader: Option<thread::JoinHandle<()>>,
    }
    impl Peer {
        fn line(&self) -> String {
            self.lines.recv_timeout(Duration::from_secs(8)).unwrap()
        }
        fn command(&mut self, text: &str) -> String {
            writeln!(self.child.stdin.as_mut().unwrap(), "{text}").unwrap();
            self.line()
        }
    }
    impl Drop for Peer {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            if let Some(reader) = self.reader.take() {
                reader.join().unwrap();
            }
        }
    }
    let settings = config();
    let partition = format!("/{}", settings.partition);
    let node = Node::new(settings.clone()).unwrap();
    let (done, observed) = mpsc::channel();
    let _raw = node
        .subscribe_raw_callback("/foo", "", move |data, info| {
            assert_eq!(info.topic, "/foo");
            assert_eq!(info.partition, partition);
            assert_eq!(info.type_name, "gz.msgs.Int32");
            assert!(!info.intra_process);
            done.send(data.to_vec()).unwrap();
            Ok(())
        })
        .unwrap();
    let _other = node
        .subscribe_raw_callback("/bar", "", |_, _| Ok(()))
        .unwrap();
    let path = std::env::var("GZ_CPP_PEER")
        .unwrap_or_else(|_| format!("{}/artifacts/cpp-peer", env!("CARGO_MANIFEST_DIR")));
    let mut child = Command::new(path)
        .args(["statistics-wire-publisher", "/foo", "/bar"])
        .env("GZ_PARTITION", &settings.partition)
        .env("GZ_IP", settings.interface.to_string())
        .env("GZ_DISCOVERY_MULTICAST_IP", settings.multicast.to_string())
        .env("GZ_DISCOVERY_MSG_PORT", settings.message_port.to_string())
        .env("GZ_DISCOVERY_SRV_PORT", settings.service_port.to_string())
        .env("GZ_TRANSPORT_IMPLEMENTATION", "zeromq")
        .env("GZ_TRANSPORT_TOPIC_STATISTICS", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (lines_tx, lines) = mpsc::channel();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if lines_tx.send(line.unwrap()).is_err() {
                break;
            }
        }
    });
    let mut peer = Peer {
        child,
        lines,
        reader: Some(reader),
    };
    assert!(peer.line().starts_with("tcp://"));
    assert!(peer.line().ends_with("@/foo"));
    assert!(peer.line().ends_with("@/bar"));
    assert_eq!(peer.line(), "READY");
    assert_eq!(peer.command("CONNECTIONS 1"), "CONNECTIONS 1 1");
    assert_eq!(peer.command("OPAQUE foo"), "PUBLISHED 1 00ff0100");
    assert_eq!(
        observed.recv_timeout(Duration::from_secs(1)).unwrap(),
        [0, 255, 1, 0]
    );
    assert_eq!(peer.command("EMPTY foo"), "PUBLISHED 1 -");
    assert!(
        observed
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .is_empty()
    );
    assert_eq!(peer.command("PUB foo 5"), "PUBLISHED 1 1005");
    assert_eq!(
        observed.recv_timeout(Duration::from_secs(1)).unwrap(),
        Int32 {
            data: 5,
            ..Default::default()
        }
        .encode_to_vec()
    );
    writeln!(peer.child.stdin.as_mut().unwrap(), "QUIT").unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = peer.child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < end);
        thread::sleep(Duration::from_millis(10));
    }
    let mut stderr = String::new();
    peer.child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(stderr.is_empty(), "{stderr}");
}
