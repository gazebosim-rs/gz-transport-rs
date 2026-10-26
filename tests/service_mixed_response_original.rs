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
 */

// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Observable assertions from the two mixed wrong/good response originals.
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use gz_msgs::{int32::Int32, prost::Message};
use gz_transport::{Config, Error, Node};

#[derive(Clone, Copy)]
enum Input {
    Int32,
    Empty,
}

#[test]
fn original_service_two_requests_one_wrong_response() {
    observe_mixed_response(Input::Int32);
}

#[test]
fn original_service_without_input_two_requests_one_wrong_response() {
    observe_mixed_response(Input::Empty);
}

fn observe_mixed_response(input: Input) {
    let (request_type, payload) = match input {
        Input::Int32 => (
            "gz.msgs.Int32",
            Int32 {
                data: 5,
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        Input::Empty => ("gz.msgs.Empty", Vec::new()),
    };
    let node = Node::new(Config {
        partition: format!("service-mixed-response-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    })
    .unwrap();
    let service = node.serve("/foo", request_type, "gz.msgs.Int32").unwrap();
    let wrong_response_executed = AtomicBool::new(false);
    let response_executed = AtomicBool::new(false);
    let stopping = AtomicBool::new(false);

    thread::scope(|scope| {
        let (ready_tx, ready_rx) = mpsc::sync_channel(0);
        let (stop_tx, stop_rx) = mpsc::sync_channel(1);
        let provider_stopping = &stopping;
        let provider = scope.spawn(move || {
            ready_tx.send(()).unwrap();
            while stop_rx.try_recv() == Err(mpsc::TryRecvError::Empty) {
                match service.recv_timeout(Duration::from_millis(10)) {
                    Ok(request) => {
                        let reply = match input {
                            Input::Int32 => {
                                let value = Int32::decode(request.data.as_slice()).unwrap();
                                assert_eq!(value.data, 5);
                                value.encode_to_vec()
                            }
                            Input::Empty => {
                                assert!(request.data.is_empty());
                                Int32 {
                                    data: 5,
                                    ..Default::default()
                                }
                                .encode_to_vec()
                            }
                        };
                        request.respond(reply, true).unwrap();
                    }
                    Err(Error::Timeout) => {}
                    Err(Error::Shutdown) if provider_stopping.load(Ordering::Acquire) => break,
                    Err(other) => panic!("unexpected provider error: {other:?}"),
                }
            }
        });
        ready_rx.recv_timeout(Duration::from_secs(10)).unwrap();

        assert_eq!(
            node.request(
                "/foo",
                request_type,
                "gz.msgs.Vector3d",
                payload.clone(),
                Duration::from_millis(1000)
            ),
            Err(Error::Timeout)
        );
        let (observer_ready_tx, observer_ready_rx) = mpsc::sync_channel(0);
        let wrong_executed = &wrong_response_executed;
        let observer_stopping = &stopping;
        let requester = &node;
        let wrong_payload = &payload;
        let wrong_observer = scope.spawn(move || {
            // The wrong observer stays live through the later good callback.
            // Native has no supplied deadline; the owned request is bounded.
            let pending = requester
                .begin_request(
                    "/foo",
                    request_type,
                    "gz.msgs.Vector3d",
                    wrong_payload.clone(),
                    Duration::from_secs(30),
                )
                .unwrap();
            observer_ready_tx.send(()).unwrap();
            match pending.wait(Duration::from_secs(30)) {
                Ok(_) => wrong_executed.store(true, Ordering::Release),
                Err(Error::Shutdown) if observer_stopping.load(Ordering::Acquire) => {}
                Err(other) => panic!("unexpected wrong-response observer error: {other:?}"),
            }
        });
        observer_ready_rx
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        thread::sleep(Duration::from_millis(300));
        assert!(!wrong_response_executed.load(Ordering::Acquire));

        // Supplemental reply checks accompany the original synchronous success.
        let (reply, result) = node
            .request(
                "/foo",
                request_type,
                "gz.msgs.Int32",
                payload.clone(),
                Duration::from_millis(1000),
            )
            .unwrap();
        assert_eq!(Int32::decode(reply.as_slice()).unwrap().data, 5);
        assert!(result);
        let pending = node
            .begin_request(
                "/foo",
                request_type,
                "gz.msgs.Int32",
                payload.clone(),
                Duration::from_secs(30),
            )
            .unwrap();
        // Keep the original callback observation's ten seconds separate from
        // the owned asynchronous request deadline.
        let (reply, result) = pending.wait(Duration::from_secs(10)).unwrap();
        // This owned observer retains the native response callback predicates.
        let response = |bytes: &[u8], result: bool| {
            assert_eq!(Int32::decode(bytes).unwrap().data, 5);
            assert!(result);
            response_executed.store(true, Ordering::Release);
        };
        response(&reply, result);
        assert!(response_executed.load(Ordering::Acquire));

        stopping.store(true, Ordering::Release);
        stop_tx.send(()).unwrap();
        node.cancel();
        provider.join().unwrap();
        wrong_observer.join().unwrap();
        // Supplemental: a late wrong reply must not be hidden by cancellation.
        assert!(!wrong_response_executed.load(Ordering::Acquire));
    });
}
