// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Supplemental readiness contract; no upstream original credit.
use gz_transport::{Config, Error, Node};
use std::{sync::Arc, thread, time::Duration};

fn config() -> Config {
    Config {
        partition: format!("services-ready-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    }
}

#[test]
fn ready_service_list_preserves_local_snapshot_and_tracks_live_handles() {
    let mut cfg = config();
    cfg.namespace = "ns".into();
    cfg.add_topic_remap("first", "/remapped").unwrap();
    let node = Node::new(cfg).unwrap();
    let first = node
        .serve("first", "gz.msgs.Int32", "gz.msgs.Int32")
        .unwrap();
    let second = node
        .serve("second", "gz.msgs.Empty", "gz.msgs.Int32")
        .unwrap();
    let immediate = node.known_services().unwrap();
    assert_eq!(immediate, ["/ns/second", "/remapped"]);
    assert_eq!(
        node.service_list(Duration::from_secs(5)).unwrap(),
        immediate
    );
    drop(first);
    assert_eq!(
        node.service_list(Duration::from_secs(5)).unwrap(),
        ["/ns/second"]
    );
    drop(second);
    assert!(
        node.service_list(Duration::from_secs(5))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn ready_service_list_bounds_deadlines_and_cancellation() {
    let node = Arc::new(Node::new(config()).unwrap());
    assert_eq!(node.service_list(Duration::MAX), Err(Error::InvalidConfig));
    assert_eq!(node.service_list(Duration::ZERO), Err(Error::Timeout));
    assert_eq!(
        node.service_list(Duration::from_millis(5)),
        Err(Error::Timeout)
    );
    // The expired startup query does not block the immediate inspection API.
    assert!(node.known_services().unwrap().is_empty());
    let caller = node.clone();
    let pending = thread::spawn(move || caller.service_list(Duration::from_secs(5)));
    node.cancel();
    assert_eq!(pending.join().unwrap(), Err(Error::Shutdown));
    assert_eq!(
        node.service_list(Duration::from_secs(5)),
        Err(Error::Shutdown)
    );
}

#[test]
fn ready_service_list_returns_empty_after_startup_and_reports_explicit_shutdown() {
    let mut node = Node::new(config()).unwrap();
    assert!(
        node.service_list(Duration::from_secs(5))
            .unwrap()
            .is_empty()
    );
    assert!(
        node.service_list(Duration::from_secs(1))
            .unwrap()
            .is_empty()
    );
    node.shutdown();
    assert_eq!(
        node.service_list(Duration::from_secs(5)),
        Err(Error::Shutdown)
    );
}
