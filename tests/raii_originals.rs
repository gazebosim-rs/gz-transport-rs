// Copyright (C) 2014, 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Exact lifetime setups from pinned src/Node_TEST.cc 979 and 2822. Neither
// upstream body contains EXPECT/ASSERT predicates. Supplements below are separate.
use gz_msgs::{
    msgs::{Int32, Vector3d},
    prost::Message,
};
use gz_transport::{Config, Error, Node};
use std::{
    net::UdpSocket,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

fn config() -> Config {
    let reservations = [
        UdpSocket::bind(("127.0.0.1", 0)).unwrap(),
        UdpSocket::bind(("127.0.0.1", 0)).unwrap(),
    ];
    let ports = reservations
        .each_ref()
        .map(|socket| socket.local_addr().unwrap().port());
    assert_ne!(ports[0], ports[1]);
    drop(reservations);
    Config {
        partition: format!("raii-{}", uuid::Uuid::new_v4()),
        message_port: ports[0],
        service_port: ports[1],
        ..Default::default()
    }
}

#[test]
fn node_pub_destruction_order_original() {
    // Rust has no invalid Publisher handle: None preserves the outer default
    // slot, replaced only by the successful inner Vector3d advertisement.
    let mut publisher = None;
    assert!(publisher.is_none());
    {
        let node = Node::new(config()).unwrap();
        publisher = Some(node.advertise_typed::<Vector3d>("/foo").unwrap());
    }
    drop(publisher);
}

#[test]
fn node_move_subscriber_original() {
    let node = Node::new(config()).unwrap();
    // The native callback is never invoked by this no-publication body. The
    // owned receiver preserves the admitted Int32 registration and move/drop.
    let subscribers = vec![node.subscribe("/foo", "gz.msgs.Int32").unwrap()];
    drop(subscribers);
    drop(node);
}

#[test]
fn moved_receiver_preserves_delivery_and_final_drop_retires_interest() {
    let mut node = Node::new(config()).unwrap();
    let publisher = node.advertise_typed::<Int32>("/foo").unwrap();
    let subscribers = vec![node.subscribe("/foo", "gz.msgs.Int32").unwrap()];
    let mut moved = subscribers;
    assert!(publisher.has_connections().unwrap());
    let receiver = moved.pop().unwrap();
    assert!(moved.is_empty());
    drop(moved);
    let value = Int32 {
        data: 5,
        ..Default::default()
    };
    publisher.publish_typed(&value).unwrap();
    let message = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(value, Int32::decode(message.data.as_slice()).unwrap());
    assert_eq!(["/foo"], node.subscribed_topics().unwrap().as_slice());
    drop(receiver);
    assert!(node.subscribed_topics().unwrap().is_empty());
    assert!(!publisher.has_connections().unwrap());
    let receiver = node.subscribe("/foo", "gz.msgs.Int32").unwrap();
    publisher.publish_typed(&value).unwrap();
    // This worker command follows publication, fixing admission order for the
    // supplemental drain check without adding readiness to either original.
    assert!(publisher.has_connections().unwrap());
    node.shutdown();
    let queued = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(value, Int32::decode(queued.data.as_slice()).unwrap());
    assert_eq!(
        Err(Error::Shutdown),
        receiver.recv_timeout(Duration::from_millis(20))
    );
    assert_eq!(Err(Error::Shutdown), publisher.publish_typed(&value));
}

#[test]
fn vector_publisher_survives_owner_and_releases_worker_after_last_drop() {
    let settings = config();
    let observer = Node::new(settings.clone()).unwrap();
    let receiver = observer.subscribe("/foo", "gz.msgs.Vector3d").unwrap();
    let publisher;
    {
        let owner = Node::new(settings).unwrap();
        publisher = owner.advertise_typed::<Vector3d>("/foo").unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while !publisher.has_connections().unwrap() {
        assert!(
            Instant::now() < deadline,
            "surviving publisher lost the observer interest"
        );
        thread::sleep(Duration::from_millis(2));
    }
    let value = Vector3d {
        x: 1.0,
        y: 2.0,
        z: 3.0,
        ..Default::default()
    };
    publisher.publish_typed(&value).unwrap();
    let message = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(value, Vector3d::decode(message.data.as_slice()).unwrap());
    let (done, joined) = mpsc::channel();
    let dropper = thread::spawn(move || {
        drop(publisher);
        done.send(()).unwrap();
    });
    joined.recv_timeout(Duration::from_secs(2)).unwrap();
    dropper.join().unwrap();
    assert!(observer.known_publisher_topics().unwrap().is_empty());
}
