// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Ported Node_TEST.cc assertions: Copyright (C) 2014 Open Source Robotics Foundation.
use gz_transport::{Config, Node, ServiceTypeResolution as Resolution, ServiceTypes};
use std::time::Duration;
const INT32: &str = "gz.msgs.Int32";
const STRING: &str = "gz.msgs.StringMsg";
fn config() -> Config {
    Config {
        partition: format!("local-resolution-{}", uuid::Uuid::new_v4()),
        ..Config::default()
    }
}
fn resolved(request: &str, response: &str) -> Resolution {
    Resolution::Resolved(ServiceTypes {
        request_type: request.into(),
        response_type: response.into(),
    })
}
// Node_TEST.cc:2100. Keep the two actual providers and every original predicate,
// without the older Rust variant's discovered-signature readiness barrier.
#[test]
fn original_resolve_service_types_ambiguous_immediate() {
    let cfg = config();
    let first = Node::new(cfg.clone()).unwrap();
    let second = Node::new(cfg).unwrap();
    let _echo = first.serve("/foo", INT32, INT32).unwrap();
    let _mixed = second.serve("/foo", INT32, STRING).unwrap();
    assert_eq!(
        first.resolve_service_types("/foo", "", "", Duration::ZERO),
        Ok(Resolution::AmbiguousTypes)
    );
    assert_eq!(
        first.resolve_service_types("/foo", "", STRING, Duration::ZERO),
        Ok(resolved(INT32, STRING))
    );
    let request = String::new();
    assert_eq!(
        first.resolve_service_types("/foo", &request, "gz.msgs.Vector3d", Duration::ZERO),
        Ok(Resolution::IncompatibleTypes)
    );
    assert!(request.is_empty());
}
#[test]
fn local_resolution_tracks_live_leases_callbacks_and_owner_stop() {
    let cfg = config();
    let observer = Node::new(cfg.clone()).unwrap();
    let first = Node::new(cfg.clone()).unwrap();
    let second = Node::new(cfg.clone()).unwrap();
    let echo = first.serve("/foo", INT32, INT32).unwrap();
    let mixed = second.serve("/foo", INT32, STRING).unwrap();
    assert_eq!(
        observer.resolve_service_types("/foo", "", "", Duration::ZERO),
        Ok(Resolution::AmbiguousTypes)
    );
    drop(mixed);
    assert_eq!(
        observer.resolve_service_types("/foo", "", "", Duration::ZERO),
        Ok(resolved(INT32, INT32))
    );
    let _mixed = second.serve("/foo", INT32, STRING).unwrap();
    second.cancel();
    assert_eq!(
        observer.resolve_service_types("/foo", "", "", Duration::ZERO),
        Ok(resolved(INT32, INT32))
    );
    drop(second);
    drop(echo);
    assert_eq!(
        observer.resolve_service_types("/foo", "", "", Duration::ZERO),
        Ok(Resolution::NoProviders)
    );
    let callback = first
        .serve_oneway_callback("/oneway", INT32, |_| Ok(()))
        .unwrap();
    assert_eq!(
        observer.resolve_service_types("/oneway", "", "", Duration::ZERO),
        Ok(resolved(INT32, "gz.msgs.Empty"))
    );
    drop(callback);
    assert_eq!(
        observer.resolve_service_types("/oneway", "", "", Duration::ZERO),
        Ok(Resolution::NoProviders)
    );
    let _service = first.serve("/foo", INT32, INT32).unwrap();
    drop(first);
    assert_eq!(
        observer.resolve_service_types("/foo", "", "", Duration::ZERO),
        Ok(Resolution::NoProviders)
    );
}
#[test]
fn local_resolution_keeps_service_domains_and_original_namespace_remap_rules() {
    let mut cfg = config();
    cfg.namespace = "ns".into();
    cfg.add_topic_remap("old", "new").unwrap();
    let observer = Node::new(cfg.clone()).unwrap();
    let source = Node::new(cfg.clone()).unwrap();
    let _source = source.serve("old", INT32, INT32).unwrap();
    assert_eq!(
        observer.resolve_service_types("old", "", "", Duration::ZERO),
        Ok(Resolution::NoProviders)
    );
    assert_eq!(
        observer.resolve_service_types("new", "", "", Duration::ZERO),
        Ok(resolved(INT32, INT32))
    );
    let mut other_namespace = cfg.clone();
    other_namespace.namespace = "other".into();
    let other_namespace = Node::new(other_namespace).unwrap();
    let _namespace = other_namespace.serve("new", INT32, STRING).unwrap();
    let mut other_partition = cfg.clone();
    other_partition.partition.push_str("-foreign");
    let other_partition = Node::new(other_partition).unwrap();
    let _partition = other_partition.serve("new", INT32, STRING).unwrap();
    let mut other_port = cfg.clone();
    other_port.service_port += 41;
    let other_port = Node::new(other_port).unwrap();
    let _port = other_port.serve("new", INT32, STRING).unwrap();
    let mut other_group = cfg.clone();
    other_group.multicast = "239.255.0.8".parse().unwrap();
    let other_group = Node::new(other_group).unwrap();
    let _group = other_group.serve("new", INT32, STRING).unwrap();
    assert_eq!(
        observer.resolve_service_types("new", "", "", Duration::ZERO),
        Ok(resolved(INT32, INT32))
    );
    // Message channels do not split an otherwise identical service domain.
    cfg.message_port += 43;
    let same_service_domain = Node::new(cfg).unwrap();
    let _same = same_service_domain.serve("new", INT32, STRING).unwrap();
    assert_eq!(
        observer.resolve_service_types("new", "", "", Duration::ZERO),
        Ok(Resolution::AmbiguousTypes)
    );
}

#[test]
fn native_warm_service_resolution_is_immediate_and_withdrawal_is_authoritative() {
    let path = std::env::var("GZ_CPP_PEER")
        .unwrap_or_else(|_| format!("{}/artifacts/cpp-peer", env!("CARGO_MANIFEST_DIR")));
    let cfg = config();
    let output = std::process::Command::new(path)
        .args(["resolve-local-immediate", "/foo", "unused"])
        .env("GZ_PARTITION", cfg.partition)
        .env("GZ_IP", "127.0.0.1")
        .env("GZ_TRANSPORT_IMPLEMENTATION", "zeromq")
        .env("GZ_TRANSPORT_TOPIC_STATISTICS", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert_eq!(
        output.stdout,
        b"ambiguity selection incompatible withdrawal\n"
    );
}
