// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Supplemental owned publication metadata; no upstream original credit.
use gz_transport::{AdvertiseOptions, Config, Error, Node, Scope, qualified_name};
use std::{
    thread,
    time::{Duration, Instant},
};
const TYPE: &str = "gz.msgs.StringMsg";
fn config() -> Config {
    Config {
        partition: format!("publisher-info-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    }
}
#[test]
fn owned_publication_info_preserves_original_names_scope_and_retired_values() {
    for scope in [Scope::All, Scope::Host, Scope::Process] {
        let mut cfg = config();
        cfg.namespace = "ns".into();
        cfg.add_topic_remap("old", "new").unwrap();
        let node = Node::new(cfg.clone()).unwrap();
        let mut options = AdvertiseOptions::default();
        options.set_scope(scope);
        options.set_msgs_per_sec(7);
        let publisher = node.advertise_with_options("old", TYPE, options).unwrap();
        assert!(node.known_topic_publishers("old").unwrap().is_empty());
        assert!(node.known_topic_publishers("/absent").unwrap().is_empty());
        let records = node.known_topic_publishers("new").unwrap();
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert_eq!(
            record.topic,
            qualified_name(&cfg.partition, "ns", "new").unwrap()
        );
        assert_eq!(record.scope, scope);
        assert!(record.throttled);
        assert_eq!(record.msgs_per_sec, 7);
        assert!(record.address.starts_with("tcp://127.0.0.1:"));
        assert!(record.control_address.is_empty());
        uuid::Uuid::parse_str(&record.process_uuid).unwrap();
        uuid::Uuid::parse_str(&record.node_uuid).unwrap();
        assert_eq!(node.known_topic_publishers("new").unwrap(), records);
        assert_eq!(node.known_topic_info("new").unwrap().publishers, [TYPE]);
        drop(publisher);
        assert!(node.known_topic_publishers("new").unwrap().is_empty());
        node.cancel();
        assert_eq!(node.known_topic_publishers("new"), Err(Error::Shutdown));
        assert_eq!(record.type_name, TYPE);
        assert_eq!(
            node.known_topic_publishers("bad name"),
            Err(Error::InvalidName)
        );
    }
}
#[test]
fn publication_info_filters_domains_services_and_subscriber_only_interests() {
    let cfg = config();
    let node = Node::new(cfg.clone()).unwrap();
    let _service = node.serve("/same", TYPE, TYPE).unwrap();
    let _subscriber = node.subscribe("/same", TYPE).unwrap();
    let foreign = Node::new(config()).unwrap();
    let _foreign = foreign.advertise("/same", TYPE).unwrap();
    let mut other = cfg.clone();
    other.message_port += 37;
    let isolated = Node::new(other).unwrap();
    let _isolated = isolated.advertise("/same", TYPE).unwrap();
    let local_peer = Node::new(cfg).unwrap();
    let mut options = AdvertiseOptions::default();
    options.set_scope(Scope::Process);
    let _process = local_peer
        .advertise_with_options("/same", TYPE, options)
        .unwrap();
    assert_eq!(foreign.known_topic_publishers("/same").unwrap().len(), 1);
    assert_eq!(isolated.known_topic_publishers("/same").unwrap().len(), 1);
    assert_eq!(node.known_publisher_topics().unwrap(), ["/same"]);
    node.topic_info("/same", Duration::from_secs(5)).unwrap();
    // This worker snapshot does not import the process-local name registry.
    assert!(node.known_topic_publishers("/same").unwrap().is_empty());
}
#[test]
fn publication_info_observes_publisher_survival_without_retaining_it() {
    let cfg = config();
    let owner = Node::new(cfg.clone()).unwrap();
    let publisher = owner.advertise("/survivor", TYPE).unwrap();
    let held = owner.known_topic_publishers("/survivor").unwrap();
    drop(owner);
    let observer = Node::new(cfg).unwrap();
    observer
        .topic_info("/survivor", Duration::from_secs(5))
        .unwrap();
    assert_eq!(observer.known_topic_publishers("/survivor").unwrap(), held);
    drop(publisher);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !observer
        .known_topic_publishers("/survivor")
        .unwrap()
        .is_empty()
    {
        assert!(
            Instant::now() < deadline,
            "retired publication remained visible"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(held[0].type_name, TYPE);
}
