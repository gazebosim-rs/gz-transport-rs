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
// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Node_TEST.cc ServiceCallAsyncLambda and ServiceCallWithoutInputAsyncLambda.
use gz_msgs::{int32::Int32, prost::Message};
use gz_transport::{Config, Node};
use std::{thread, time::Duration};

const TOPIC: &str = "/foo";
const INT32: &str = "gz.msgs.Int32";
const EMPTY: &str = "gz.msgs.Empty";
const DATA: i32 = 5;
// An owned-request safety bound; the pinned local C++ originals have none.
const RUST_WAIT: Duration = Duration::from_secs(1);

fn node() -> Node {
    Node::new(Config {
        partition: format!("service-lambda-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    })
    .unwrap()
}

fn request_with_response(
    node: &Node,
    request_type: &str,
    data: Vec<u8>,
    callback: impl FnOnce(&Int32, bool),
) {
    // The native local responder and reply callback run before Request returns.
    // This owned adapter preserves that post-call observation for its caller.
    let (response, result) = node
        .request(TOPIC, request_type, INT32, data, RUST_WAIT)
        .unwrap();
    callback(&Int32::decode(response.as_slice()).unwrap(), result);
}

fn original_case(with_input: bool) {
    let node = node();
    let request_type = if with_input { INT32 } else { EMPTY };
    let service = node.serve(TOPIC, request_type, INT32).unwrap();
    let mut executed = false;
    let mut response_calls = 0;
    let data = if with_input {
        Int32 {
            data: DATA,
            ..Default::default()
        }
        .encode_to_vec()
    } else {
        Vec::new()
    };

    thread::scope(|scope| {
        let provider = scope.spawn(move || {
            let request = service.recv_timeout(RUST_WAIT).unwrap();
            let reply = if with_input {
                let value = Int32::decode(request.data.as_slice()).unwrap();
                let adv_cb = |req: &Int32| {
                    assert_eq!(req.data, DATA);
                    Int32 {
                        data: req.data,
                        ..Default::default()
                    }
                };
                adv_cb(&value)
            } else {
                assert!(request.data.is_empty());
                let adv_cb = || Int32 {
                    data: DATA,
                    ..Default::default()
                };
                adv_cb()
            };
            request.respond(reply.encode_to_vec(), true).unwrap();
        });

        let req_cb = |rep: &Int32, result: bool| {
            assert_eq!(rep.data, DATA);
            assert!(result);
            executed = true;
            response_calls += 1;
        };
        request_with_response(&node, request_type, data, req_cb);
        assert!(executed);
        assert_eq!(response_calls, 1);
        provider.join().unwrap();
    });
}

#[test]
fn original_service_call_async_lambda() {
    original_case(true);
}

#[test]
fn original_service_call_without_input_async_lambda() {
    original_case(false);
}
