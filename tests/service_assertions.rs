// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Ported Node_TEST.cc assertions: Copyright (C) 2014 Open Source Robotics Foundation.
use gz_msgs::{int32::Int32, prost::Message};
use gz_transport::{Config, Error, Node};
use std::{
    thread,
    time::{Duration, Instant},
};

const INT32: &str = "gz.msgs.Int32";
const EMPTY: &str = "gz.msgs.Empty";
fn node() -> Node {
    Node::new(Config {
        partition: format!("service-port-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    })
    .unwrap()
}
fn five() -> Vec<u8> {
    Int32 {
        data: 5,
        ..Default::default()
    }
    .encode_to_vec()
}

fn sync_service(with_input: bool) {
    let node = node();
    let request_type = if with_input { INT32 } else { EMPTY };
    let input = if with_input { five() } else { vec![] };
    let service = node.serve("/foo", request_type, INT32).unwrap();
    let timeout = Duration::from_millis(1000);
    assert_eq!(
        node.request(
            "invalid service",
            request_type,
            INT32,
            input.clone(),
            timeout
        ),
        Err(Error::InvalidName)
    );
    thread::scope(|scope| {
        let requested = scope.spawn(|| node.request("/foo", request_type, INT32, input, timeout));
        let request = service.recv_timeout(timeout).unwrap();
        if with_input {
            assert_eq!(Int32::decode(request.data.as_slice()).unwrap().data, 5);
        } else {
            assert!(request.data.is_empty());
        }
        request.respond(five(), true).unwrap();
        let (response, result) = requested.join().unwrap().unwrap();
        assert!(result);
        assert_eq!(Int32::decode(response.as_slice()).unwrap().data, 5);
    });
}
fn sync_timeout(with_input: bool) {
    let node = node();
    let request_type = if with_input { INT32 } else { EMPTY };
    let input = if with_input { five() } else { vec![] };
    let start = Instant::now();
    let result = node.request(
        "/foo",
        request_type,
        INT32,
        input,
        Duration::from_millis(1000),
    );
    let elapsed = start.elapsed().as_millis();
    assert!(elapsed.abs_diff(1000) <= 200, "elapsed={elapsed}ms");
    assert_eq!(result, Err(Error::Timeout));
}
#[test]
fn original_service_call_sync() {
    sync_service(true);
}
#[test]
fn original_service_call_without_input_sync() {
    sync_service(false);
}
#[test]
fn original_service_call_sync_timeout() {
    sync_timeout(true);
}
#[test]
fn original_service_call_without_input_sync_timeout() {
    sync_timeout(false);
}
