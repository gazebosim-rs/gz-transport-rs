/*
 * Copyright (C) 2014 Open Source Robotics Foundation
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 *
 */
// Owned Rust request/dispatch adaptation: Copyright (C) 2026 Open Source Robotics Foundation.
// SPDX-License-Identifier: Apache-2.0
// Pinned Node_TEST.cc NodeTest.ClassMemberCallbackService and
// NodeTest.ClassMemberCallbackServiceWithoutInput, including MyTestClass.
use gz_msgs::{int32::Int32, prost::Message, vector3d::Vector3d};
use gz_transport::{Config, Error, Node, Service};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

const TOPIC: &str = "/foo";
const INT32: &str = "gz.msgs.Int32";
const EMPTY: &str = "gz.msgs.Empty";
const VECTOR3D: &str = "gz.msgs.Vector3d";
const TIMEOUT: Duration = Duration::from_millis(500);
const DATA: i32 = 5;

#[derive(Clone, Copy)]
enum Kind {
    Echo,
    WithoutInput,
}
struct Client {
    node: Node,
    callback_srv_executed: AtomicBool,
    response_executed: AtomicBool,
}
impl Client {
    fn new() -> Self {
        Self {
            node: Node::new(Config {
                partition: format!("member-service-{}", uuid::Uuid::new_v4()),
                ..Default::default()
            })
            .unwrap(),
            callback_srv_executed: AtomicBool::new(false),
            response_executed: AtomicBool::new(false),
        }
    }
    fn reset(&self) {
        self.callback_srv_executed.store(false, Ordering::Release);
        self.response_executed.store(false, Ordering::Release);
    }
    fn echo(&self, request: &Int32) -> Int32 {
        assert_eq!(request.data, DATA);
        self.callback_srv_executed.store(true, Ordering::Release);
        Int32 {
            data: request.data,
            ..Default::default()
        }
    }
    fn without_input(&self) -> Int32 {
        self.callback_srv_executed.store(true, Ordering::Release);
        Int32 {
            data: DATA,
            ..Default::default()
        }
    }
    fn echo_response(&self, response: &Int32, result: bool) {
        assert_eq!(response.data, DATA);
        assert!(result);
        self.response_executed.store(true, Ordering::Release);
    }
    fn without_input_response(&self, response: &Int32, result: bool) {
        assert_eq!(response.data, DATA);
        assert!(result);
        self.response_executed.store(true, Ordering::Release);
    }
    fn dispatch(&self, service: Service, stop: &AtomicBool, kind: Kind) {
        while !stop.load(Ordering::Acquire) {
            match service.recv_timeout(Duration::from_millis(10)) {
                Ok(request) => {
                    let response = match kind {
                        Kind::Echo => {
                            let value = Int32::decode(request.data.as_slice()).unwrap();
                            self.echo(&value)
                        }
                        Kind::WithoutInput => {
                            assert!(request.data.is_empty());
                            self.without_input()
                        }
                    };
                    request.respond(response.encode_to_vec(), true).unwrap();
                }
                Err(Error::Timeout) => {}
                Err(Error::Shutdown) => return,
                Err(error) => panic!("service receive: {error}"),
            }
        }
    }
}
struct StopOnDrop<'a>(&'a AtomicBool);
impl Drop for StopOnDrop<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
fn request_data(with_input: bool) -> Vec<u8> {
    if with_input {
        Int32 {
            data: DATA,
            ..Default::default()
        }
        .encode_to_vec()
    } else {
        Vec::new()
    }
}
fn check_reply(response: &[u8], result: bool) {
    assert!(result);
    assert_eq!(Int32::decode(response).unwrap().data, DATA);
}
fn original_case(with_input: bool) {
    let client = Client::new();
    let kind = if with_input {
        Kind::Echo
    } else {
        Kind::WithoutInput
    };
    let request_type = if with_input { INT32 } else { EMPTY };
    let request_data = request_data(with_input);
    client.reset();
    assert!(matches!(
        client.node.serve("Bad Srv", request_type, INT32),
        Err(Error::InvalidName)
    ));
    let service = client.node.serve(TOPIC, request_type, INT32).unwrap();

    let stop = AtomicBool::new(false);
    thread::scope(|scope| {
        let _stop_on_panic = StopOnDrop(&stop);
        let dispatch = scope.spawn(|| client.dispatch(service, &stop, kind));

        let (response, result) = client
            .node
            .request(TOPIC, request_type, INT32, request_data.clone(), TIMEOUT)
            .unwrap();
        check_reply(&response, result);
        assert!(client.callback_srv_executed.load(Ordering::Acquire));

        client.reset();
        let pending = client
            .node
            .begin_request(TOPIC, request_type, INT32, request_data.clone(), TIMEOUT)
            .unwrap();
        let (response_done_tx, response_done_rx) = std::sync::mpsc::sync_channel(1);
        let response_client = &client;
        let response_dispatch = scope.spawn(move || {
            let (response, result) = pending.wait(TIMEOUT).unwrap();
            let value = Int32::decode(response.as_slice()).unwrap();
            if with_input {
                response_client.echo_response(&value, result);
            } else {
                response_client.without_input_response(&value, result);
            }
            response_done_tx.send(()).unwrap();
        });
        response_done_rx.recv_timeout(TIMEOUT).unwrap();
        assert!(client.response_executed.load(Ordering::Acquire));
        response_dispatch.join().unwrap();

        if with_input {
            client.reset();
            let raw_request = Int32::decode(request_data.as_slice())
                .unwrap()
                .encode_to_vec();
            let (raw_response, result) = client
                .node
                .request(TOPIC, INT32, INT32, raw_request, TIMEOUT)
                .unwrap();
            assert_eq!(Int32::decode(raw_response.as_slice()).unwrap().data, DATA);
            assert!(result);
            assert!(client.callback_srv_executed.load(Ordering::Acquire));
        }

        client.reset();
        if with_input {
            let wrong_request = Vector3d::default().encode_to_vec();
            assert_eq!(
                client
                    .node
                    .request(TOPIC, VECTOR3D, INT32, wrong_request.clone(), TIMEOUT),
                Err(Error::Timeout)
            );
            assert_eq!(
                client
                    .node
                    .request(TOPIC, INT32, VECTOR3D, request_data.clone(), TIMEOUT),
                Err(Error::Timeout)
            );
            let wrong_input = client
                .node
                .begin_request(TOPIC, VECTOR3D, INT32, wrong_request, TIMEOUT)
                .unwrap();
            let wrong_output = client
                .node
                .begin_request(TOPIC, INT32, VECTOR3D, request_data, TIMEOUT)
                .unwrap();
            thread::sleep(Duration::from_millis(300));
            assert!(!client.callback_srv_executed.load(Ordering::Acquire));
            drop((wrong_input, wrong_output));
        } else {
            assert_eq!(
                client
                    .node
                    .request(TOPIC, EMPTY, VECTOR3D, Vec::new(), TIMEOUT),
                Err(Error::Timeout)
            );
            let wrong_output = client
                .node
                .begin_request(TOPIC, EMPTY, VECTOR3D, Vec::new(), TIMEOUT)
                .unwrap();
            thread::sleep(Duration::from_millis(300));
            assert!(!client.callback_srv_executed.load(Ordering::Acquire));
            drop(wrong_output);
        }
        stop.store(true, Ordering::Release);
        dispatch.join().unwrap();
    });
}
#[test]
fn original_class_member_callback_service() {
    original_case(true);
}
#[test]
fn original_class_member_callback_service_without_input() {
    original_case(false);
}

#[test]
fn dropped_pending_response_cannot_take_a_later_request_reply() {
    let client = Client::new();
    let service = client.node.serve(TOPIC, INT32, INT32).unwrap();
    let data = |value| {
        Int32 {
            data: value,
            ..Default::default()
        }
        .encode_to_vec()
    };
    assert!(matches!(
        client
            .node
            .begin_request("Bad Srv", INT32, INT32, data(1), TIMEOUT),
        Err(Error::InvalidName)
    ));
    let abandoned = client
        .node
        .begin_request(TOPIC, INT32, INT32, data(1), TIMEOUT)
        .unwrap();
    drop(abandoned);
    let retained = client
        .node
        .begin_request(TOPIC, INT32, INT32, data(2), TIMEOUT)
        .unwrap();
    for expected in [1, 2] {
        let incoming = service.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(
            Int32::decode(incoming.data.as_slice()).unwrap().data,
            expected
        );
        incoming.respond(data(expected), true).unwrap();
    }
    let (reply, success) = retained.wait(TIMEOUT).unwrap();
    assert!(success);
    assert_eq!(Int32::decode(reply.as_slice()).unwrap().data, 2);
}
