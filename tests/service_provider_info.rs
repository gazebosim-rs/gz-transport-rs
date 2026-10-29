// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Supplemental owned provider-metadata contract; no new original-case credit.
use gz_transport::{Config, Error, Node, Scope, qualified_name};
use std::time::Duration;
fn config() -> Config {
    Config {
        partition: format!("service-info-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    }
}
#[test]
fn owned_service_info_preserves_original_query_names_and_retired_values() {
    let mut cfg = config();
    cfg.namespace = "ns".into();
    cfg.add_topic_remap("old", "new").unwrap();
    let node = Node::new(cfg.clone()).unwrap();
    let service = node.serve("old", "gz.msgs.Int32", "gz.msgs.Empty").unwrap();
    assert!(node.known_service_info("old").unwrap().is_empty());
    assert!(node.known_service_info("/absent").unwrap().is_empty());
    let records = node.known_service_info("new").unwrap();
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(
        record.topic,
        qualified_name(&cfg.partition, "ns", "new").unwrap()
    );
    assert_eq!(record.scope, Scope::All);
    assert_eq!(record.request_type, "gz.msgs.Int32");
    assert!(record.address.starts_with("tcp://127.0.0.1:"));
    for id in [&record.process_uuid, &record.node_uuid, &record.socket_id] {
        uuid::Uuid::parse_str(id).unwrap();
    }
    assert_eq!(node.known_service_info("new").unwrap(), records);
    drop(service);
    assert!(node.known_service_info("new").unwrap().is_empty());
    node.cancel();
    assert_eq!(node.known_service_info("new"), Err(Error::Shutdown));
    // This owned snapshot remains readable after the live registration is gone.
    assert_eq!(record.response_type, "gz.msgs.Empty");
    assert_eq!(
        node.known_service_info("invalid name"),
        Err(Error::InvalidName)
    );
}
#[test]
fn service_info_does_not_import_other_partitions_or_message_endpoints() {
    let cfg = config();
    let node = Node::new(cfg.clone()).unwrap();
    let _publisher = node.advertise("/same", "gz.msgs.Int32").unwrap();
    let foreign = Node::new(config()).unwrap();
    let _service = foreign
        .serve("/same", "gz.msgs.Int32", "gz.msgs.Int32")
        .unwrap();
    let mut separate = cfg.clone();
    separate.service_port += 37;
    let other_domain = Node::new(separate).unwrap();
    let _other = other_domain
        .serve("/same", "gz.msgs.Int32", "gz.msgs.Int32")
        .unwrap();
    assert_eq!(foreign.known_service_info("/same").unwrap().len(), 1);
    assert_eq!(other_domain.known_service_info("/same").unwrap().len(), 1);
    node.service_list(Duration::from_secs(5)).unwrap();
    assert!(node.known_service_info("/same").unwrap().is_empty());
    assert!(node.service_provider_types("/same").unwrap().is_empty());
}
