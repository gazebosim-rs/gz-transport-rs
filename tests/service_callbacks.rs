// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Node_TEST.cc member-service assertions: Copyright (C) 2014 Open Source Robotics Foundation.
use gz_msgs::{int32::Int32, prost::Message, stringmsg::StringMsg, vector3d::Vector3d};
use gz_transport::{Config, Error, Node};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};
const INT: &str = "gz.msgs.Int32";
const EMPTY: &str = "gz.msgs.Empty";
const WRONG: &str = "gz.msgs.Vector3d";
const STRING: &str = "gz.msgs.StringMsg";
const TIMEOUT: Duration = Duration::from_millis(500);
fn config() -> Config {
    Config {
        partition: format!("callback-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    }
}
struct Client {
    node: Node,
    service_entries: AtomicUsize,
    service: AtomicBool,
    response: AtomicBool,
}
impl Client {
    fn echo(&self, bytes: &[u8], with_input: bool) -> Result<(Vec<u8>, bool), Error> {
        // Native EXPECT_EQ is nonfatal. A caught Rust assertion panic must not
        // hide an unexpected wrong-type invocation from the quiet control.
        self.service_entries.fetch_add(1, Ordering::AcqRel);
        if with_input {
            assert_eq!(Int32::decode(bytes).unwrap().data, 5);
        } else {
            assert!(bytes.is_empty());
        }
        self.service.store(true, Ordering::Release);
        Ok((
            Int32 {
                data: 5,
                ..Default::default()
            }
            .encode_to_vec(),
            true,
        ))
    }
    fn reply(&self, bytes: Vec<u8>, result: bool) {
        assert!(result);
        assert_eq!(Int32::decode(bytes.as_slice()).unwrap().data, 5);
        self.response.store(true, Ordering::Release);
    }
    fn reset(&self) {
        self.service_entries.store(0, Ordering::Release);
        self.service.store(false, Ordering::Release);
        self.response.store(false, Ordering::Release);
    }
}
fn original(with_input: bool) {
    let client = Arc::new(Client {
        node: Node::new(config()).unwrap(),
        service_entries: AtomicUsize::new(0),
        service: AtomicBool::new(false),
        response: AtomicBool::new(false),
    });
    let node = &client.node;
    let request_type = if with_input { INT } else { EMPTY };
    let data = if with_input {
        Int32 {
            data: 5,
            ..Default::default()
        }
        .encode_to_vec()
    } else {
        Vec::new()
    };
    assert!(matches!(
        node.serve_callback("Bad Srv", request_type, INT, |_| unreachable!()),
        Err(Error::InvalidName)
    ));
    let state = client.clone();
    let _service = node
        .serve_callback("/foo", request_type, INT, move |bytes| {
            state.echo(bytes, with_input)
        })
        .unwrap();
    let (reply, result) = node
        .request("/foo", request_type, INT, data.clone(), TIMEOUT)
        .unwrap();
    client.reply(reply, result);
    assert!(client.service.load(Ordering::Acquire));
    client.reset();
    let state = client.clone();
    let request = node
        .request_callback(
            "/foo",
            request_type,
            INT,
            data.clone(),
            TIMEOUT,
            move |reply, result| state.reply(reply, result),
        )
        .unwrap();
    // The native assertion is immediate, with no readiness or completion wait.
    assert!(client.response.load(Ordering::Acquire));
    assert!(client.service.load(Ordering::Acquire));
    assert_eq!(request.wait(TIMEOUT), Ok(()));
    if with_input {
        client.reset();
        let raw = Int32::decode(data.as_slice()).unwrap().encode_to_vec();
        let (reply, result) = node.request("/foo", INT, INT, raw, TIMEOUT).unwrap();
        assert_eq!(Int32::decode(reply.as_slice()).unwrap().data, 5);
        assert!(result);
        assert!(client.service.load(Ordering::Acquire));
    }
    client.reset();
    let wrong_data = Vector3d::default().encode_to_vec();
    if with_input {
        assert_eq!(
            node.request("/foo", WRONG, INT, wrong_data.clone(), TIMEOUT),
            Err(Error::Timeout)
        );
    }
    assert_eq!(
        node.request("/foo", request_type, WRONG, data.clone(), TIMEOUT),
        Err(Error::Timeout)
    );
    let wrong_callback_entries = Arc::new(AtomicUsize::new(0));
    let wrong_input = if with_input {
        let entries = wrong_callback_entries.clone();
        Some(
            node.request_callback("/foo", WRONG, INT, wrong_data, TIMEOUT, move |_, _| {
                entries.fetch_add(1, Ordering::AcqRel);
                panic!("wrong request callback")
            })
            .unwrap(),
        )
    } else {
        None
    };
    let entries = wrong_callback_entries.clone();
    let wrong_output = node
        .request_callback("/foo", request_type, WRONG, data, TIMEOUT, move |_, _| {
            entries.fetch_add(1, Ordering::AcqRel);
            panic!("wrong response callback")
        })
        .unwrap();
    thread::sleep(Duration::from_millis(300));
    assert_eq!(wrong_callback_entries.load(Ordering::Acquire), 0);
    assert!(!client.service.load(Ordering::Acquire));
    assert_eq!(client.service_entries.load(Ordering::Acquire), 0);
    assert!(!client.response.load(Ordering::Acquire));
    drop((wrong_input, wrong_output));
}
#[test]
fn original_member_service_callbacks_are_immediate() {
    original(true);
}
#[test]
fn original_member_service_without_input_callbacks_are_immediate() {
    original(false);
}
#[test]
fn local_callbacks_keep_domain_remap_and_exactly_once_semantics() {
    let mut cfg = config();
    cfg.namespace = "ns".into();
    cfg.add_topic_remap("old", "new").unwrap();
    let provider = Node::new(cfg.clone()).unwrap();
    let caller = Node::new(cfg.clone()).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let here = thread::current().id();
    let _service = provider
        .serve_callback("old", INT, INT, move |bytes| {
            assert_eq!(thread::current().id(), here);
            count.fetch_add(1, Ordering::Relaxed);
            Ok((bytes.to_vec(), true))
        })
        .unwrap();
    let response = Arc::new(AtomicBool::new(false));
    let seen = response.clone();
    let request = caller
        .request_callback("old", INT, INT, vec![8, 5], TIMEOUT, move |bytes, ok| {
            assert_eq!(thread::current().id(), here);
            assert_eq!(bytes, [8, 5]);
            assert!(ok);
            seen.store(true, Ordering::Release);
        })
        .unwrap();
    assert!(response.load(Ordering::Acquire));
    request.wait(TIMEOUT).unwrap();
    thread::sleep(Duration::from_millis(100));
    assert_eq!(calls.load(Ordering::Acquire), 1);
    for dimension in 0..2 {
        let mut other = cfg.clone();
        match dimension {
            0 => other.partition.push_str("-other"),
            1 => other.namespace = "other".into(),
            _ => unreachable!(),
        }
        let other = Node::new(other).unwrap();
        let pending = other
            .request_callback(
                "old",
                INT,
                INT,
                vec![],
                Duration::from_millis(50),
                |_, _| panic!("cross domain"),
            )
            .unwrap();
        assert_eq!(pending.wait(TIMEOUT), Err(Error::Timeout));
    }
    assert_eq!(calls.load(Ordering::Acquire), 1);
}
struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn peer(mode: &str, cfg: &Config) -> ChildGuard {
    let path = std::env::var("GZ_CPP_PEER")
        .unwrap_or_else(|_| format!("{}/artifacts/cpp-peer", env!("CARGO_MANIFEST_DIR")));
    ChildGuard(
        Command::new(path)
            .args([mode, "/callback-native", "hello"])
            .env("GZ_PARTITION", &cfg.partition)
            .env("GZ_IP", "127.0.0.1")
            .env("GZ_TRANSPORT_IMPLEMENTATION", "zeromq")
            .env("GZ_TRANSPORT_TOPIC_STATISTICS", "0")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    )
}
#[test]
fn callbacks_exchange_real_native_requests_and_responses() {
    let cfg = config();
    let node = Node::new(cfg.clone()).unwrap();
    let mut native = peer("serve", &cfg);
    let mut line = String::new();
    BufReader::new(native.0.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert_eq!(line.trim(), "READY");
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let handle = node
        .request_callback(
            "/callback-native",
            STRING,
            STRING,
            StringMsg {
                data: "hello".into(),
                ..Default::default()
            }
            .encode_to_vec(),
            Duration::from_secs(8),
            move |data, ok| {
                tx.send((StringMsg::decode(data.as_slice()).unwrap().data, ok))
                    .unwrap();
            },
        )
        .unwrap();
    handle.wait(Duration::from_secs(9)).unwrap();
    assert_eq!(rx.recv().unwrap(), ("hellohello".into(), true));
    drop(native);
    let _service = node
        .serve_callback("/callback-native", STRING, STRING, |bytes| {
            let mut value = StringMsg::decode(bytes).unwrap();
            value.data.push_str("-rust");
            Ok((value.encode_to_vec(), true))
        })
        .unwrap();
    let mut native = peer("request", &cfg);
    assert!(native.0.wait().unwrap().success());
}
