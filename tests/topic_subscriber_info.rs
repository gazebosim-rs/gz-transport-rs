// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Supplemental metadata and native storage identity controls; no original credit.
use gz_transport::{Config, Error, Node, Scope, SubscribeOptions, qualified_name};
use std::process::Command;
use std::time::Duration;
const TYPE: &str = "gz.msgs.StringMsg";
fn config() -> Config {
    Config {
        partition: format!("subscriber-info-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    }
}
#[test]
fn native_subscriber_storage_identity_keep_first_and_retirement_are_replayed() {
    let peer = std::env::var("GZ_CPP_PEER")
        .unwrap_or_else(|_| format!("{}/artifacts/cpp-peer", env!("CARGO_MANIFEST_DIR")));
    let output = Command::new(peer)
        .args(["subscriber-storage-identity", "@/probe@/topic", "unused"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let expected = "identical 1 1 0 1 ctrl-A 2 gz.msgs.Int32 7\nctrl 0 1 0 1 ctrl-A 2 gz.msgs.Int32 7\nscope 0 1 0 1 ctrl-A 2 gz.msgs.Int32 7\ntype 0 1 0 1 ctrl-A 2 gz.msgs.Int32 7\nrate 1 1 0 1 ctrl-A 2 gz.msgs.Int32 7\naddress 0 1 1 2 ctrl-A 2 gz.msgs.Int32 7\nnode 0 1 1 2 ctrl-A 2 gz.msgs.Int32 7\nprocess 0 1 1 2 ctrl-A 2 gz.msgs.Int32 7\ntopic 0 1 1 1 ctrl-A 2 gz.msgs.Int32 7\nretirement 4 0 4 1 2 1\n";
    assert_eq!(output.stdout, expected.as_bytes());
}
#[test]
fn own_subscriber_metadata_preserves_names_types_defaults_and_owned_retirement() {
    let mut cfg = config();
    cfg.namespace = "ns".into();
    cfg.add_topic_remap("old", "new").unwrap();
    let mut node = Node::new(cfg.clone()).unwrap();
    let mut options = SubscribeOptions::default();
    options.set_msgs_per_sec(0);
    options.set_ignore_local_messages(true);
    let first = node.subscribe_with_options("old", TYPE, options).unwrap();
    let duplicate = node.subscribe("old", TYPE).unwrap();
    let _integer = node.subscribe("old", "gz.msgs.Int32").unwrap();
    let _wildcard = node.subscribe("old", "").unwrap();
    assert!(node.known_topic_subscribers("old").unwrap().is_empty());
    assert!(node.known_topic_subscribers("/absent").unwrap().is_empty());
    let held = node.known_topic_subscribers("new").unwrap();
    assert_eq!(held.len(), 3);
    for record in &held {
        assert_eq!(
            record.topic,
            qualified_name(&cfg.partition, "ns", "new").unwrap()
        );
        assert_eq!(record.scope, Scope::All);
        assert!(record.control_address.is_empty());
        assert!(!record.throttled);
        assert_eq!(record.msgs_per_sec, u64::MAX);
        assert!(record.address.starts_with("tcp://127.0.0.1:"));
        uuid::Uuid::parse_str(&record.process_uuid).unwrap();
        uuid::Uuid::parse_str(&record.node_uuid).unwrap();
    }
    let names: Vec<_> = held.iter().map(|r| r.type_name.as_str()).collect();
    assert_eq!(names, ["google.protobuf.Message", "gz.msgs.Int32", TYPE]);
    drop(first);
    assert_eq!(node.known_topic_subscribers("new").unwrap().len(), 3);
    drop(duplicate);
    assert_eq!(node.known_topic_subscribers("new").unwrap().len(), 2);
    node.unsubscribe("new").unwrap();
    assert!(node.known_topic_subscribers("new").unwrap().is_empty());
    node.shutdown();
    assert_eq!(node.known_topic_subscribers("new"), Err(Error::Shutdown));
    assert_eq!(held.len(), 3);
    assert_eq!(
        node.known_topic_subscribers("bad name"),
        Err(Error::InvalidName)
    );
}
#[test]
fn subscriber_metadata_filters_foreign_domains_and_other_record_roles() {
    let cfg = config();
    let node = Node::new(cfg.clone()).unwrap();
    let _publisher = node.advertise("/same", TYPE).unwrap();
    let _service = node.serve("/same", TYPE, TYPE).unwrap();
    let foreign = Node::new(config()).unwrap();
    let _foreign = foreign.subscribe("/same", TYPE).unwrap();
    let mut other = cfg;
    other.message_port += 37;
    let isolated = Node::new(other).unwrap();
    let _isolated = isolated.subscribe("/same", TYPE).unwrap();
    assert_eq!(foreign.known_topic_subscribers("/same").unwrap().len(), 1);
    assert_eq!(isolated.known_topic_subscribers("/same").unwrap().len(), 1);
    node.topic_info("/same", Duration::from_secs(5)).unwrap();
    assert!(node.known_topic_subscribers("/same").unwrap().is_empty());
    assert_eq!(node.known_topic_publishers("/same").unwrap().len(), 1);
    assert_eq!(node.known_service_info("/same").unwrap().len(), 1);
}
