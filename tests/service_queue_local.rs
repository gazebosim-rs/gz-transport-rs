// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Supplemental admitted local queue and native scope controls; no original credit.
use gz_msgs::{int32::Int32, prost::Message};
use gz_transport::{Config, Error, Node};
use std::{
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};
const INT: &str = "gz.msgs.Int32";
const EMPTY: &str = "gz.msgs.Empty";
fn config(capacity: usize) -> Config {
    Config {
        partition: format!("queued-local-{}", uuid::Uuid::new_v4()),
        capacity,
        ..Default::default()
    }
}
fn five() -> Vec<u8> {
    Int32 {
        data: 5,
        ..Default::default()
    }
    .encode_to_vec()
}
#[test]
fn queued_cross_node_callback_runs_on_dispatcher_and_preserves_false_reply_once() {
    let cfg = config(4);
    let provider = Node::new(cfg.clone()).unwrap();
    let caller = Node::new(cfg).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let entries = calls.clone();
    let here = thread::current().id();
    let _service = provider
        .serve_callback("/foo", INT, INT, move |bytes| {
            entries.fetch_add(1, Ordering::AcqRel);
            assert_ne!(
                thread::current().id(),
                here,
                "begin_request executed user code inline"
            );
            assert_eq!(Int32::decode(bytes).unwrap().data, 5);
            Ok((five(), false))
        })
        .unwrap();
    let reply = caller
        .begin_request("/foo", INT, INT, five(), Duration::from_secs(1))
        .unwrap()
        .wait(Duration::from_secs(1))
        .unwrap();
    assert_eq!(reply, (five(), false));
    assert_eq!(calls.load(Ordering::Acquire), 1);
    thread::sleep(Duration::from_millis(100));
    assert_eq!(calls.load(Ordering::Acquire), 1);
}
#[test]
fn queued_cross_node_oneway_reports_handoff_and_provider_backpressure() {
    let cfg = config(1);
    let provider = Node::new(cfg.clone()).unwrap();
    let caller = Node::new(cfg).unwrap();
    let service = provider.serve("/foo", INT, EMPTY).unwrap();
    assert_eq!(
        caller
            .begin_request("/foo", INT, EMPTY, five(), Duration::from_secs(1))
            .unwrap()
            .wait(Duration::from_secs(1)),
        Ok((vec![], true))
    );
    assert_eq!(
        caller
            .begin_request("/foo", INT, EMPTY, five(), Duration::from_secs(1))
            .unwrap()
            .wait(Duration::from_secs(1)),
        Err(Error::QueueFull)
    );
    assert_eq!(provider.statistics().queue_drops, 1);
    assert_eq!(caller.statistics().queue_drops, 0);
    // The first completion precedes application receive: it acknowledges queue
    // handoff, not user execution or delivery to a service callback.
    let incoming = service.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(incoming.data, five());
    assert!(!incoming.expects_response());
}
#[test]
fn queued_cross_node_requests_keep_pending_capacity_and_admitted_withdrawal() {
    let cfg = config(1);
    let provider = Node::new(cfg.clone()).unwrap();
    let caller = Node::new(cfg).unwrap();
    let service = provider.serve("/foo", INT, INT).unwrap();
    let pending = caller
        .begin_request("/foo", INT, INT, five(), Duration::from_secs(1))
        .unwrap();
    let incoming = service.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(
        caller
            .begin_request("/foo", INT, INT, five(), Duration::from_secs(1))
            .unwrap()
            .wait(Duration::from_secs(1)),
        Err(Error::QueueFull)
    );
    assert_eq!(caller.statistics().queue_drops, 1);
    provider.unadvertise_service("/foo").unwrap();
    incoming.respond(five(), true).unwrap();
    assert_eq!(pending.wait(Duration::from_secs(1)), Ok((five(), true)));
    assert_eq!(
        caller
            .begin_request("/foo", INT, INT, five(), Duration::from_millis(50))
            .unwrap()
            .wait(Duration::from_secs(1)),
        Err(Error::Timeout)
    );
    assert!(matches!(
        service.recv_timeout(Duration::from_millis(20)),
        Err(Error::Shutdown)
    ));
}
#[test]
fn queued_cross_node_callback_errors_do_not_retry_or_retire_the_registration() {
    let cfg = config(4);
    let provider = Node::new(cfg.clone()).unwrap();
    let caller = Node::new(cfg).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let entries = calls.clone();
    let _service = provider
        .serve_callback("/foo", INT, INT, move |bytes| {
            let call = entries.fetch_add(1, Ordering::AcqRel);
            assert_eq!(Int32::decode(bytes).unwrap().data, 5);
            match call {
                0 => Err(Error::InvalidFrame),
                1 => panic!("intentional queued callback panic"),
                _ => Ok((five(), true)),
            }
        })
        .unwrap();
    for _ in 0..2 {
        assert_eq!(
            caller
                .begin_request("/foo", INT, INT, five(), Duration::from_millis(100))
                .unwrap()
                .wait(Duration::from_secs(1)),
            Ok((vec![], false))
        );
    }
    assert_eq!(
        caller
            .begin_request("/foo", INT, INT, five(), Duration::from_secs(1))
            .unwrap()
            .wait(Duration::from_secs(1)),
        Ok((five(), true))
    );
    thread::sleep(Duration::from_millis(100));
    assert_eq!(calls.load(Ordering::Acquire), 3);
    assert_eq!(provider.statistics().routing_errors, 2);
    assert_eq!(caller.statistics().routing_errors, 0);
}
#[test]
fn native_process_scope_service_remains_callable_from_another_local_node() {
    let path = std::env::var("GZ_CPP_PEER")
        .unwrap_or_else(|_| format!("{}/artifacts/cpp-peer", env!("CARGO_MANIFEST_DIR")));
    let output = Command::new(path)
        .args(["service-local-process-scope", "/scope", "unused"])
        .env(
            "GZ_PARTITION",
            format!("native-local-scope-{}", uuid::Uuid::new_v4()),
        )
        .env("GZ_IP", "127.0.0.1")
        .env("GZ_TRANSPORT_IMPLEMENTATION", "zeromq")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    assert_eq!(
        output.stdout,
        b"scope=Process providers=1 returned=1 result=1 data=5 calls=1 withdrawn=0\n"
    );
}

#[test]
fn queued_cross_node_shutdown_keeps_owned_drain_and_caller_cancellation() {
    let cfg = config(4);
    let mut provider = Node::new(cfg.clone()).unwrap();
    let caller = Node::new(cfg).unwrap();
    let service = provider.serve("/foo", INT, INT).unwrap();
    let pending = caller
        .begin_request("/foo", INT, INT, five(), Duration::from_millis(100))
        .unwrap();
    let admitted = service.recv_timeout(Duration::from_secs(1)).unwrap();
    provider.shutdown();
    assert_eq!(admitted.respond(five(), true), Err(Error::Shutdown));
    assert!(matches!(
        service.recv_timeout(Duration::from_millis(20)),
        Err(Error::Shutdown)
    ));
    assert_eq!(pending.wait(Duration::from_secs(1)), Err(Error::Timeout));

    let cfg = config(4);
    let provider = Node::new(cfg.clone()).unwrap();
    let mut caller = Node::new(cfg).unwrap();
    let service = provider.serve("/foo", INT, INT).unwrap();
    let pending = caller
        .begin_request("/foo", INT, INT, five(), Duration::from_secs(1))
        .unwrap();
    let admitted = service.recv_timeout(Duration::from_secs(1)).unwrap();
    caller.shutdown();
    assert_eq!(pending.wait(Duration::from_secs(1)), Err(Error::Shutdown));
    assert_eq!(admitted.respond(five(), true), Ok(()));
}

#[test]
fn queued_cross_node_payload_limits_apply_before_service_receive() {
    let mut cfg = config(4);
    cfg.max_message_bytes = 1;
    let provider = Node::new(cfg.clone()).unwrap();
    cfg.max_message_bytes = 8;
    let caller = Node::new(cfg).unwrap();
    let service = provider.serve("/foo", INT, INT).unwrap();
    assert_eq!(
        caller
            .begin_request("/foo", INT, INT, five(), Duration::from_secs(1))
            .unwrap()
            .wait(Duration::from_secs(1)),
        Err(Error::InvalidFrame)
    );
    assert!(matches!(
        service.recv_timeout(Duration::from_millis(20)),
        Err(Error::Timeout)
    ));
    let mut cfg = config(4);
    cfg.max_message_bytes = 1;
    let caller = Node::new(cfg).unwrap();
    assert!(matches!(
        caller.begin_request("/foo", INT, INT, five(), Duration::from_secs(1)),
        Err(Error::InvalidFrame)
    ));
}
