// Copyright (C) 2014, 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Pinned Node_TEST.cc527: explicit fallible-construction/default-state disposition.
use gz_msgs::{msgs::Vector3d, prost::Message};
use gz_transport::{Config, Error, Node, Publisher, qualified_name};
use socket2::{Domain, Protocol, Socket, Type};
use std::{
    net::{Ipv4Addr, SocketAddrV4},
    time::Duration,
};

fn config() -> (Config, [Socket; 2]) {
    let reserve = || {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).unwrap();
        socket.set_reuse_address(true).unwrap();
        socket
            .bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into())
            .unwrap();
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
    (
        Config {
            partition: uuid::Uuid::new_v4().to_string(),
            message_port: ports[0],
            service_port: ports[1],
            ..Default::default()
        },
        reservations,
    )
}

// Also a positive compiler control: the public typed factory supplies the real
// Publisher type. It needs no default object or boolean facade.
fn vector_publisher(node: &Node) -> Result<Publisher, Error> {
    node.advertise_typed::<Vector3d>("/foo")
}

#[test]
fn publisher_bool_operator_original_with_fallible_construction() {
    let (settings, _reservations) = config();
    let node = Node::new(settings.clone()).unwrap();
    let node2 = Node::new(settings).unwrap();

    // C++ default Publisher objects have empty topics and convert to false.
    // Rust cannot construct them (the public compile-fail control is separate).
    // These real factory errors retain invalid empty-topic intent; they are
    // Result values, not default Publisher objects or literal bool parity.
    let mut publisher = node.advertise_typed::<Vector3d>("");
    let publisher_const = node.advertise_typed::<Vector3d>("");
    assert!(matches!(&publisher, Err(Error::InvalidName)));
    assert!(matches!(&publisher_const, Err(Error::InvalidName)));

    publisher = vector_publisher(&node);
    let publisher2_const = vector_publisher(&node2);
    assert!(publisher.is_ok());
    assert!(publisher2_const.is_ok());
}

#[test]
fn two_typed_factory_handles_preserve_topic_type_and_shared_borrow_use() {
    let (settings, _reservations) = config();
    let topic = qualified_name(&settings.partition, "", "/foo").unwrap();
    let node = Node::new(settings.clone()).unwrap();
    let node2 = Node::new(settings).unwrap();
    let receiver = node.subscribe("/foo", "gz.msgs.Vector3d").unwrap();
    let publisher = vector_publisher(&node).unwrap();
    let publisher2_const = vector_publisher(&node2).unwrap();
    for owner in [&node, &node2] {
        assert_eq!(owner.advertised_topics().unwrap(), ["/foo"]);
        let records = owner.known_topic_publishers("/foo").unwrap();
        assert!(!records.is_empty());
        for record in records {
            assert_eq!(record.topic, topic);
            assert_eq!(record.type_name, "gz.msgs.Vector3d");
        }
    }
    // Supplemental bounded receipts establish usable &Publisher handles. The
    // original has no publication, receipt, cancellation or liveness predicate.
    for (handle, x) in [(&publisher, 1.0), (&publisher2_const, 2.0)] {
        let value = Vector3d {
            x,
            y: 3.0,
            z: 4.0,
            ..Default::default()
        };
        handle.publish_typed(&value).unwrap();
        let received = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(Vector3d::decode(received.data.as_slice()).unwrap(), value);
    }
}
