// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use gz_msgs::{int32::Int32, prost::Message, stringmsg::StringMsg};
use gz_transport::{Config, Error, Node};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};
const INT: &str = "gz.msgs.Int32";
const EMPTY: &str = "gz.msgs.Empty";
fn config() -> Config {
    Config {
        partition: format!("oneway-local-{}", uuid::Uuid::new_v4()),
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
fn cross_node_oneway_callback_finishes_inline_once() {
    let cfg = config();
    let provider = Node::new(cfg.clone()).unwrap();
    let caller = Node::new(cfg).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let entries = calls.clone();
    let completed = Arc::new(AtomicBool::new(false));
    let done = completed.clone();
    let here = thread::current().id();
    let _service = provider
        .serve_oneway_callback("/foo", INT, move |bytes| {
            entries.fetch_add(1, Ordering::AcqRel);
            assert_eq!(Int32::decode(bytes).unwrap().data, 5);
            assert_eq!(thread::current().id(), here);
            done.store(true, Ordering::Release);
            Ok(())
        })
        .unwrap();
    caller
        .request_oneway("/foo", INT, five(), Duration::from_secs(1))
        .unwrap();
    assert!(completed.load(Ordering::Acquire));
    assert_eq!(calls.load(Ordering::Acquire), 1);
    thread::sleep(Duration::from_millis(100));
    assert_eq!(calls.load(Ordering::Acquire), 1);
}
#[test]
fn cross_node_oneway_errors_and_panics_are_final_and_leave_registration_live() {
    let cfg = config();
    let provider = Node::new(cfg.clone()).unwrap();
    let caller = Node::new(cfg).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let entries = calls.clone();
    let service = provider
        .serve_oneway_callback("/foo", INT, move |bytes| {
            let call = entries.fetch_add(1, Ordering::AcqRel);
            assert_eq!(Int32::decode(bytes).unwrap().data, 5);
            match call {
                0 => Err(Error::InvalidFrame),
                1 => panic!("intentional cross-Node callback panic"),
                _ => Ok(()),
            }
        })
        .unwrap();
    assert_eq!(
        caller.request_oneway("/foo", INT, five(), Duration::from_secs(1)),
        Err(Error::InvalidFrame)
    );
    assert_eq!(
        caller.request_oneway("/foo", INT, five(), Duration::from_secs(1)),
        Err(Error::Native("one-way callback panicked".into()))
    );
    caller
        .request_oneway("/foo", INT, five(), Duration::from_secs(1))
        .unwrap();
    assert_eq!(calls.load(Ordering::Acquire), 3);
    assert_eq!(provider.statistics().routing_errors, 2);
    assert_eq!(caller.statistics().routing_errors, 0);
    thread::sleep(Duration::from_millis(100));
    assert_eq!(calls.load(Ordering::Acquire), 3);
    drop(service);
    // A stale wire advertisement may still admit a send. It cannot resurrect
    // the retired user callback; this test asserts execution, not convergence.
    let _ = caller.request_oneway("/foo", INT, five(), Duration::from_millis(100));
    thread::sleep(Duration::from_millis(100));
    assert_eq!(calls.load(Ordering::Acquire), 3);
}
#[test]
fn one_way_preserves_own_queue_and_callback_before_other_providers() {
    let cfg = config();
    let foreign = Node::new(cfg.clone()).unwrap();
    let caller = Node::new(cfg).unwrap();
    let foreign_calls = Arc::new(AtomicUsize::new(0));
    let count = foreign_calls.clone();
    let _foreign = foreign
        .serve_oneway_callback("/owned", INT, move |_| {
            count.fetch_add(1, Ordering::AcqRel);
            Ok(())
        })
        .unwrap();
    let queued = caller.serve("/owned", INT, EMPTY).unwrap();
    caller
        .request_oneway("/owned", INT, five(), Duration::from_secs(1))
        .unwrap();
    let request = queued.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(Int32::decode(request.data.as_slice()).unwrap().data, 5);
    assert!(!request.expects_response());
    assert_eq!(foreign_calls.load(Ordering::Acquire), 0);
    drop(queued);
    let own_calls = Arc::new(AtomicUsize::new(0));
    let count = own_calls.clone();
    let _own = caller
        .serve_oneway_callback("/owned", INT, move |_| {
            count.fetch_add(1, Ordering::AcqRel);
            Ok(())
        })
        .unwrap();
    caller
        .request_oneway("/owned", INT, five(), Duration::from_secs(1))
        .unwrap();
    assert_eq!(own_calls.load(Ordering::Acquire), 1);
    assert_eq!(foreign_calls.load(Ordering::Acquire), 0);
}
#[test]
fn native_two_node_oneway_callback_boundary() {
    let peer = std::env::var("GZ_CPP_PEER")
        .unwrap_or_else(|_| format!("{}/artifacts/cpp-peer", env!("CARGO_MANIFEST_DIR")));
    let result = std::process::Command::new(peer)
        .args(["oneway-local-immediate", "/foo", "unused"])
        .env("GZ_PARTITION", config().partition)
        .env("GZ_IP", "127.0.0.1")
        .env("GZ_TRANSPORT_IMPLEMENTATION", "zeromq")
        .env("GZ_TRANSPORT_TOPIC_STATISTICS", "0")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(result.stdout,b"inline=1 caller-thread=1 data=5 wrong-type=quiet withdrawal=quiet invalid-name=rejected\n");
    assert_eq!(result.stderr, b"Service [Bad Srv] is not valid.\n");
}

struct NativeChild(std::process::Child);
impl Drop for NativeChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn native_oneway(mode: &str, topic: &str, value: &str, cfg: &Config) -> NativeChild {
    use std::io::{BufRead, BufReader};
    let path = std::env::var("GZ_CPP_PEER")
        .unwrap_or_else(|_| format!("{}/artifacts/cpp-peer", env!("CARGO_MANIFEST_DIR")));
    let mut child = NativeChild(
        std::process::Command::new(path)
            .args([mode, topic, value])
            .env("GZ_PARTITION", &cfg.partition)
            .env("GZ_IP", "127.0.0.1")
            .env("GZ_TRANSPORT_IMPLEMENTATION", "zeromq")
            .env("GZ_TRANSPORT_TOPIC_STATISTICS", "0")
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut ready = String::new();
    BufReader::new(child.0.stdout.take().unwrap())
        .read_line(&mut ready)
        .unwrap();
    assert_eq!(ready.trim(), "READY");
    child
}
#[test]
fn one_way_callbacks_and_requests_keep_both_native_wire_directions() {
    let cfg = config();
    let node = Node::new(cfg.clone()).unwrap();
    let mut native = native_oneway("serve-oneway", "/outgoing", "wire-five", &cfg);
    node.request_oneway(
        "/outgoing",
        "gz.msgs.StringMsg",
        StringMsg {
            data: "wire-five".into(),
            ..Default::default()
        }
        .encode_to_vec(),
        Duration::from_secs(8),
    )
    .unwrap();
    // The native observer's exit verifies execution; the API handoff itself
    // deliberately provides no delivery acknowledgement.
    assert!(native.0.wait().unwrap().success());
    drop(native);
    let calls = Arc::new(AtomicUsize::new(0));
    let entries = calls.clone();
    let (complete, completion) = std::sync::mpsc::sync_channel(1);
    let _service = node
        .serve_oneway_callback("/incoming", "gz.msgs.StringMsg", move |bytes| {
            entries.fetch_add(1, Ordering::AcqRel);
            assert_eq!(StringMsg::decode(bytes).unwrap().data, "native-five");
            complete.send(()).unwrap();
            Ok(())
        })
        .unwrap();
    let mut native = native_oneway("request-oneway", "/incoming", "native-five", &cfg);
    completion.recv_timeout(Duration::from_secs(8)).unwrap();
    assert!(native.0.wait().unwrap().success());
    assert_eq!(calls.load(Ordering::Acquire), 1);
}
