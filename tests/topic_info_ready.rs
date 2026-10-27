// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Supplemental bounded readiness contracts; no new upstream original credit.
use gz_msgs::{prost::Message, stringmsg::StringMsg};
use gz_transport::{AdvertiseOptions, Config, Error, Node, Scope, SubscribeOptions};
use std::{
    sync::{Arc, Barrier},
    thread,
    time::Duration,
};
const TYPE: &str = "gz.msgs.StringMsg";
fn config() -> Config {
    Config {
        partition: format!("ready-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    }
}
fn bytes(value: &str) -> Vec<u8> {
    StringMsg {
        data: value.into(),
        ..Default::default()
    }
    .encode_to_vec()
}
#[test]
fn startup_query_preserves_immediate_types_and_warm_absence() {
    let cfg = config();
    let observer = Node::new(cfg.clone()).unwrap();
    let peer = Node::new(cfg).unwrap();
    let _publisher = peer.advertise("/clock", "gz.msgs.Clock").unwrap();
    let _subscriber = peer.subscribe("/clock", TYPE).unwrap();
    let immediate = observer.known_topic_info("/clock").unwrap();
    assert_eq!(immediate.publishers, ["gz.msgs.Clock"]);
    assert_eq!(immediate.subscribers, [TYPE]);
    let ready = observer
        .topic_info("/clock", Duration::from_secs(5))
        .unwrap();
    assert_eq!(ready, immediate);
    let absent = observer
        .topic_info("/absent", Duration::from_secs(5))
        .unwrap();
    assert!(absent.publishers.is_empty() && absent.subscribers.is_empty());
}
#[test]
fn startup_query_has_deadline_name_overflow_and_cancel_errors() {
    let node = Arc::new(Node::new(config()).unwrap());
    assert_eq!(
        node.topic_info("invalid topic", Duration::ZERO),
        Err(Error::InvalidName)
    );
    assert_eq!(
        node.topic_info("/absent", Duration::MAX),
        Err(Error::InvalidConfig)
    );
    assert_eq!(
        node.topic_info("/absent", Duration::ZERO),
        Err(Error::Timeout)
    );
    assert_eq!(
        node.topic_info("/absent", Duration::from_millis(5)),
        Err(Error::Timeout)
    );
    let barrier = Arc::new(Barrier::new(2));
    let caller = node.clone();
    let started = barrier.clone();
    let pending = thread::spawn(move || {
        started.wait();
        caller.topic_info("/absent", Duration::from_secs(5))
    });
    barrier.wait();
    node.cancel();
    assert_eq!(pending.join().unwrap(), Err(Error::Shutdown));
    assert_eq!(
        node.topic_info("/absent", Duration::from_secs(5)),
        Err(Error::Shutdown)
    );
}
#[test]
fn prepared_all_host_and_process_publishers_deliver_first_payload_once() {
    for scope in [Scope::All, Scope::Host, Scope::Process] {
        let cfg = config();
        let owner = Node::new(cfg.clone()).unwrap();
        let observer = Node::new(cfg).unwrap();
        let receiver = observer.subscribe("/first", TYPE).unwrap();
        let mut options = AdvertiseOptions::default();
        options.set_scope(scope);
        let publisher = owner
            .advertise_with_options("/first", TYPE, options)
            .unwrap();
        assert!(
            publisher
                .refresh_connection_snapshot(Duration::from_secs(5))
                .unwrap()
        );
        assert!(publisher.connection_snapshot().unwrap());
        publisher.publish(bytes("first")).unwrap();
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(5)).unwrap().data,
            bytes("first")
        );
        assert_eq!(
            receiver.recv_timeout(Duration::from_millis(20)),
            Err(Error::Timeout)
        );
    }
}
#[test]
fn prepared_snapshot_filters_domain_and_descriptor_and_tracks_process_withdrawal() {
    let cfg = config();
    let owner = Node::new(cfg.clone()).unwrap();
    let wrong_kind = Node::new(cfg.clone()).unwrap();
    let _wrong = wrong_kind
        .subscribe("/filtered", "gz.msgs.Vector3d")
        .unwrap();
    let foreign = Node::new(config()).unwrap();
    let _foreign = foreign.subscribe("/filtered", TYPE).unwrap();
    let mut options = AdvertiseOptions::default();
    options.set_scope(Scope::Process);
    let publisher = owner
        .advertise_with_options("/filtered", TYPE, options)
        .unwrap();
    assert!(
        !publisher
            .refresh_connection_snapshot(Duration::from_secs(5))
            .unwrap()
    );
    assert!(!publisher.connection_snapshot().unwrap());
    let observer = Node::new(cfg).unwrap();
    let receiver = observer.subscribe("/filtered", TYPE).unwrap();
    assert!(
        publisher
            .refresh_connection_snapshot(Duration::from_secs(5))
            .unwrap()
    );
    publisher.publish(bytes("matching")).unwrap();
    assert_eq!(
        receiver.recv_timeout(Duration::from_secs(5)).unwrap().data,
        bytes("matching")
    );
    drop(receiver);
    assert!(
        !publisher
            .refresh_connection_snapshot(Duration::from_secs(5))
            .unwrap()
    );
    assert!(!publisher.connection_snapshot().unwrap());
    let _replacement = observer.subscribe("/filtered", TYPE).unwrap();
    assert!(
        publisher
            .refresh_connection_snapshot(Duration::from_secs(5))
            .unwrap()
    );
    drop(observer);
    assert!(
        !publisher
            .refresh_connection_snapshot(Duration::from_secs(5))
            .unwrap()
    );
}
#[test]
fn prepared_membership_keeps_ignore_local_and_publisher_owner_lifetime() {
    let cfg = config();
    let owner = Node::new(cfg.clone()).unwrap();
    let mut subscribe = SubscribeOptions::default();
    subscribe.set_ignore_local_messages(true);
    let ignored = owner
        .subscribe_with_options("/owned", TYPE, subscribe)
        .unwrap();
    let mut options = AdvertiseOptions::default();
    options.set_scope(Scope::Process);
    let publisher = owner
        .advertise_with_options("/owned", TYPE, options)
        .unwrap();
    assert!(
        publisher
            .refresh_connection_snapshot(Duration::from_secs(5))
            .unwrap()
    );
    publisher.publish(bytes("ignored")).unwrap();
    assert_eq!(
        ignored.recv_timeout(Duration::from_millis(20)),
        Err(Error::Timeout)
    );
    let observer = Node::new(cfg).unwrap();
    let receiver = observer.subscribe("/owned", TYPE).unwrap();
    drop(owner);
    assert!(
        publisher
            .refresh_connection_snapshot(Duration::from_secs(5))
            .unwrap()
    );
    publisher.publish(bytes("survives")).unwrap();
    assert_eq!(
        receiver.recv_timeout(Duration::from_secs(5)).unwrap().data,
        bytes("survives")
    );
}
#[test]
fn prepared_snapshot_reports_cancel_and_explicit_shutdown() {
    for shutdown in [false, true] {
        let mut owner = Node::new(config()).unwrap();
        let publisher = owner.advertise("/stopped", TYPE).unwrap();
        publisher
            .refresh_connection_snapshot(Duration::from_secs(5))
            .unwrap();
        if shutdown {
            owner.shutdown();
        } else {
            owner.cancel();
        }
        assert_eq!(
            publisher.refresh_connection_snapshot(Duration::from_secs(5)),
            Err(Error::Shutdown)
        );
        assert_eq!(publisher.connection_snapshot(), Err(Error::Shutdown));
    }
}
