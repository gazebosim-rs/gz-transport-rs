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
// Observable assertions from the three local wrong-type service originals.
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use gz_msgs::{int32::Int32, prost::Message, vector3d::Vector3d};
use gz_transport::{Config, Error, Node};

#[derive(Clone, Copy)]
enum WrongType {
    Request,
    Response,
    InputlessResponse,
}

#[test]
fn original_service_request_wrong_request_type() {
    observe_wrong_type(WrongType::Request);
}

#[test]
fn original_service_request_wrong_response_type() {
    observe_wrong_type(WrongType::Response);
}

#[test]
fn original_service_without_input_request_wrong_response_type() {
    observe_wrong_type(WrongType::InputlessResponse);
}

fn observe_wrong_type(case: WrongType) {
    let (provider_request_type, request_type, response_type, payload) = match case {
        WrongType::Request => (
            "gz.msgs.Int32",
            "gz.msgs.Vector3d",
            "gz.msgs.Int32",
            Vector3d {
                x: 1.,
                y: 2.,
                z: 3.,
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        WrongType::Response => (
            "gz.msgs.Int32",
            "gz.msgs.Int32",
            "gz.msgs.Vector3d",
            Int32 {
                data: 5,
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        WrongType::InputlessResponse => (
            "gz.msgs.Empty",
            "gz.msgs.Empty",
            "gz.msgs.Vector3d",
            Vec::new(),
        ),
    };
    let node = Node::new(Config {
        partition: format!("service-wrong-request-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    })
    .unwrap();
    let service = node
        .serve("/foo", provider_request_type, "gz.msgs.Int32")
        .unwrap();
    let callback_executed = AtomicBool::new(false);
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
                        let reply = match case {
                            WrongType::Request | WrongType::Response => {
                                let input = Int32::decode(request.data.as_slice()).unwrap();
                                assert_eq!(input.data, 5);
                                input.encode_to_vec()
                            }
                            WrongType::InputlessResponse => {
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

        // Admission maps the native asynchronous true return. Both owned
        // observers remain live through the original checkpoints.
        let (observer_ready_tx, observer_ready_rx) = mpsc::sync_channel(0);
        let executed = &callback_executed;
        let observer_stopping = &stopping;
        let requester = &node;
        let request_payload = &payload;
        let observer = scope.spawn(move || {
            let pending = requester
                .begin_request(
                    "/foo",
                    request_type,
                    response_type,
                    request_payload.clone(),
                    Duration::from_secs(10),
                )
                .unwrap();
            observer_ready_tx.send(()).unwrap();
            match pending.wait(Duration::from_secs(10)) {
                Ok((bytes, result)) => {
                    // The native wrongResponse ignores both arguments. Any
                    // response must trip the flag in the wrong-response cases.
                    if matches!(case, WrongType::Request) {
                        assert_eq!(Int32::decode(bytes.as_slice()).unwrap().data, 5);
                        assert!(result);
                    }
                    executed.store(true, Ordering::Release);
                }
                Err(Error::Shutdown) if observer_stopping.load(Ordering::Acquire) => {}
                Err(other) => panic!("unexpected asynchronous request error: {other:?}"),
            }
        });
        observer_ready_rx
            .recv_timeout(Duration::from_secs(10))
            .unwrap();

        thread::sleep(Duration::from_millis(300));
        assert!(!callback_executed.load(Ordering::Acquire));
        assert_eq!(
            node.request(
                "/foo",
                request_type,
                response_type,
                payload.clone(),
                Duration::from_millis(1000),
            ),
            Err(Error::Timeout)
        );

        stopping.store(true, Ordering::Release);
        stop_tx.send(()).unwrap();
        node.cancel();
        provider.join().unwrap();
        observer.join().unwrap();
        // Supplemental: even a late admitted response must not be hidden at shutdown.
        assert!(!callback_executed.load(Ordering::Acquire));
    });
}
