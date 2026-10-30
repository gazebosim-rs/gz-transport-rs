// Copyright (C) 2014, 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Complete owned-subscriber original states; extra quiet proof stays separate.
use gz_msgs::msgs::Int32;
use gz_transport::{CallbackSubscription, Config, Node};
use std::{
    net::UdpSocket,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

type Gate = Arc<(Mutex<bool>, Condvar)>;
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
        partition: format!("owned-sub-{}", uuid::Uuid::new_v4()),
        message_port: ports[0],
        service_port: ports[1],
        ..Default::default()
    }
}
fn connected(publisher: &gz_transport::Publisher) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !publisher.has_connections().unwrap() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
}
fn gate() -> Gate {
    Arc::new((Mutex::new(false), Condvar::new()))
}
fn observed(gate: &Gate, timeout: Duration) -> bool {
    let (flag, ready) = &**gate;
    let (flag, _) = ready
        .wait_timeout_while(flag.lock().unwrap(), timeout, |flag| !*flag)
        .unwrap();
    *flag
}
fn subscribe(node: &Node, gate: &Gate, entries: &Arc<AtomicUsize>) -> CallbackSubscription {
    let gate = gate.clone();
    let entries = entries.clone();
    node.subscribe_typed_callback::<Int32, _>("/foo", move |message, _| {
        entries.fetch_add(1, Ordering::AcqRel);
        assert_eq!(message.data, 5);
        *gate.0.lock().unwrap() = true;
        gate.1.notify_all();
        Ok(())
    })
    .unwrap()
}
#[test]
fn original_owned_subscriber_bool_operator_states() {
    let node = Node::new(config()).unwrap();
    // Rust has no allocated invalid callback registration. Option preserves the
    // two native default-invalid states, including the immutable one.
    let mut sub: Option<CallbackSubscription> = None;
    let sub_const: Option<CallbackSubscription> = None;
    assert!(!sub.as_ref().is_some_and(CallbackSubscription::is_active));
    assert!(
        !sub_const
            .as_ref()
            .is_some_and(CallbackSubscription::is_active)
    );
    sub = Some(
        node.subscribe_typed_callback::<Int32, _>("/foo", |_, _| Ok(()))
            .unwrap(),
    );
    assert!(sub.as_ref().unwrap().is_active());
    assert!(sub.as_ref().unwrap().unsubscribe());
    assert!(!sub.as_ref().unwrap().is_active());
    let sub2_const = node
        .subscribe_typed_callback::<Int32, _>("/foo", |_, _| Ok(()))
        .unwrap();
    assert!(sub2_const.is_active());
}
#[test]
fn original_pub_sub_with_create_subscriber() {
    let node = Node::new(config()).unwrap();
    let message = Int32 {
        data: 5,
        ..Default::default()
    };
    let publisher = node.advertise_typed::<Int32>("/foo").unwrap();
    let executed = gate();
    let entries = Arc::new(AtomicUsize::new(0));
    {
        // Preserve the original default-invalid slot followed by assignment.
        let mut sub: Option<CallbackSubscription> = None;
        assert!(sub.replace(subscribe(&node, &executed, &entries)).is_none());
        assert!(sub.as_ref().unwrap().is_active());
        connected(&publisher);
        publisher.publish_typed(&message).unwrap();
        assert!(observed(&executed, Duration::from_secs(1)));
    }
    publisher.publish_typed(&message).unwrap();
    // Preserve native's actual original: executed was not reset, and there is
    // no false assertion after this 300ms wait. Strong quiet proof is separate.
    let _ = observed(&executed, Duration::from_millis(300));
}
#[test]
fn original_pub_sub_with_mixed_subscribe_apis() {
    let node = Node::new(config()).unwrap();
    let message = Int32 {
        data: 5,
        ..Default::default()
    };
    let publisher = node.advertise_typed::<Int32>("/foo").unwrap();
    let executed = [gate(), gate(), gate()];
    let entries = std::array::from_fn::<_, 3, _>(|_| Arc::new(AtomicUsize::new(0)));
    // Holding the first owned callback to Node-wide unsubscribe translates the
    // native legacy registration lifetime; no second execution API is invented.
    let subscribers =
        std::array::from_fn::<_, 3, _>(|i| subscribe(&node, &executed[i], &entries[i]));
    assert!(subscribers.iter().all(CallbackSubscription::is_active));
    connected(&publisher);
    publisher.publish_typed(&message).unwrap();
    for flag in &executed {
        assert!(observed(flag, Duration::from_secs(1)));
    }
    for flag in &executed {
        *flag.0.lock().unwrap() = false;
    }
    assert!(subscribers[1].unsubscribe());
    publisher.publish_typed(&message).unwrap();
    assert!(observed(&executed[0], Duration::from_secs(1)));
    assert!(!observed(&executed[1], Duration::from_millis(300)));
    assert!(observed(&executed[2], Duration::from_secs(1)));
    assert_eq!(
        entries.each_ref().map(|n| n.load(Ordering::Acquire)),
        [2, 1, 2]
    );
    for flag in &executed {
        *flag.0.lock().unwrap() = false;
    }
    node.unsubscribe("/foo").unwrap();
    publisher.publish_typed(&message).unwrap();
    for flag in &executed {
        assert!(!observed(flag, Duration::from_millis(300)));
    }
    assert_eq!(
        entries.each_ref().map(|n| n.load(Ordering::Acquire)),
        [2, 1, 2]
    );
}
#[test]
fn scoped_handle_drop_suppresses_further_entry_with_held_publisher() {
    let node = Node::new(config()).unwrap();
    let message = Int32 {
        data: 5,
        ..Default::default()
    };
    let publisher = node.advertise_typed::<Int32>("/foo").unwrap();
    let executed = gate();
    let entries = Arc::new(AtomicUsize::new(0));
    {
        let _sub = subscribe(&node, &executed, &entries);
        connected(&publisher);
        publisher.publish_typed(&message).unwrap();
        assert!(observed(&executed, Duration::from_secs(1)));
        assert_eq!(entries.load(Ordering::Acquire), 1);
    }
    *executed.0.lock().unwrap() = false;
    publisher.publish_typed(&message).unwrap();
    assert!(!observed(&executed, Duration::from_millis(300)));
    assert_eq!(entries.load(Ordering::Acquire), 1);
    assert!(node.subscribed_topics().unwrap().is_empty());
}

#[test]
fn native_stored_handle_validity_remains_distinct_from_live_registration() {
    use std::process::{Command, Stdio};
    let settings = config();
    let path = std::env::var("GZ_OWNED_SUBSCRIBER_PEER").unwrap_or_else(|_| {
        format!(
            "{}/artifacts/owned-subscriber-peer",
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
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("native owned-subscriber probe timed out");
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
        "own first=1 valid_after=0 repeated=0\nnode unsubscribe=1 held_valid=1 held_unsubscribe=1\nowner_exit held_valid=1 held_unsubscribe=1\n"
    );
}
