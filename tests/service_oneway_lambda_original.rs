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
// Node_TEST.cc NodeTest.ServiceCallWithoutOutputAsyncLambda.
use gz_msgs::{int32::Int32, prost::Message};
use gz_transport::{Config, Error, Node};
use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

const TOPIC: &str = "/foo";
const INT32: &str = "gz.msgs.Int32";
const EMPTY: &str = "gz.msgs.Empty";
const DATA: i32 = 5;
const RUST_WAIT: Duration = Duration::from_secs(1);

fn node() -> Node {
    Node::new(Config {
        partition: format!("service-oneway-lambda-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    })
    .unwrap()
}

fn five() -> Vec<u8> {
    Int32 {
        data: DATA,
        ..Default::default()
    }
    .encode_to_vec()
}

#[test]
fn original_service_call_without_output_async_lambda() {
    let node = node();
    let executed = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_executed = executed.clone();
    let callback_calls = calls.clone();
    let _service = node
        .serve_oneway_callback(TOPIC, INT32, move |bytes| {
            let request = Int32::decode(bytes).map_err(|_| Error::InvalidFrame)?;
            if request.data != DATA {
                return Err(Error::InvalidFrame);
            }
            callback_calls.fetch_add(1, Ordering::AcqRel);
            callback_executed.store(true, Ordering::Release);
            Ok(())
        })
        .unwrap();

    node.request_oneway(TOPIC, INT32, five(), RUST_WAIT)
        .unwrap();
    assert!(executed.load(Ordering::Acquire));
    assert_eq!(calls.load(Ordering::Acquire), 1);
    assert_eq!(
        node.service_type_signatures(TOPIC).unwrap()[0].response_type,
        EMPTY
    );
    thread::sleep(Duration::from_millis(100));
    assert_eq!(calls.load(Ordering::Acquire), 1);
}

#[test]
fn callback_completion_precedes_raw_request_return_and_drop_retires_it() {
    let node = node();
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let callback_gate = gate.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_calls = calls.clone();
    let service = node
        .serve_oneway_callback(TOPIC, INT32, move |bytes| {
            if Int32::decode(bytes).map_err(|_| Error::InvalidFrame)?.data != DATA {
                return Err(Error::InvalidFrame);
            }
            entered_tx.send(()).unwrap();
            let (lock, ready) = &*callback_gate;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = ready.wait(released).unwrap();
            }
            callback_calls.fetch_add(1, Ordering::AcqRel);
            Ok(())
        })
        .unwrap();

    thread::scope(|scope| {
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        let request_node = &node;
        scope.spawn(move || {
            done_tx
                .send(request_node.request_oneway(TOPIC, INT32, five(), RUST_WAIT))
                .unwrap();
        });
        entered_rx.recv_timeout(RUST_WAIT).unwrap();
        assert!(done_rx.try_recv().is_err());
        let (lock, ready) = &*gate;
        *lock.lock().unwrap() = true;
        ready.notify_one();
        done_rx.recv_timeout(RUST_WAIT).unwrap().unwrap();
    });
    assert_eq!(calls.load(Ordering::Acquire), 1);
    drop(service);
    assert_eq!(
        node.request_oneway(TOPIC, INT32, five(), Duration::from_millis(100)),
        Err(Error::Timeout)
    );
    assert_eq!(calls.load(Ordering::Acquire), 1);
}

#[test]
fn callback_type_and_panic_errors_are_typed() {
    let node = node();
    assert!(matches!(
        node.serve_oneway_callback("invalid service", INT32, |_| Ok(())),
        Err(Error::InvalidName)
    ));
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_calls = calls.clone();
    let _service = node
        .serve_oneway_callback(TOPIC, INT32, move |_| {
            if callback_calls.fetch_add(1, Ordering::AcqRel) == 0 {
                panic!("intentional callback fault");
            }
            Ok(())
        })
        .unwrap();
    assert!(matches!(
        node.serve_oneway_callback(TOPIC, INT32, |_| Ok(())),
        Err(Error::AlreadyAdvertised)
    ));
    assert_eq!(
        node.request_oneway(
            TOPIC,
            "gz.msgs.StringMsg",
            five(),
            Duration::from_millis(100)
        ),
        Err(Error::Timeout)
    );
    assert_eq!(calls.load(Ordering::Acquire), 0);
    assert_eq!(
        node.request_oneway(TOPIC, INT32, five(), Duration::MAX),
        Err(Error::InvalidConfig)
    );
    assert_eq!(
        node.request_oneway(TOPIC, INT32, five(), Duration::ZERO),
        Err(Error::Timeout)
    );
    assert_eq!(calls.load(Ordering::Acquire), 0);
    assert_eq!(
        node.request_oneway(TOPIC, INT32, five(), RUST_WAIT),
        Err(Error::Native("one-way callback panicked".into()))
    );
    node.request_oneway(TOPIC, INT32, five(), RUST_WAIT)
        .unwrap();
    assert_eq!(calls.load(Ordering::Acquire), 2);
    assert_eq!(node.statistics().routing_errors, 1);
}

#[test]
fn shutdown_retires_callback_registration() {
    let mut node = node();
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_calls = calls.clone();
    let service = node
        .serve_oneway_callback(TOPIC, INT32, move |_| {
            callback_calls.fetch_add(1, Ordering::AcqRel);
            Ok(())
        })
        .unwrap();
    node.shutdown();
    assert_eq!(
        node.request_oneway(TOPIC, INT32, five(), RUST_WAIT),
        Err(Error::Shutdown)
    );
    drop(service);
    assert_eq!(calls.load(Ordering::Acquire), 0);
}

#[test]
fn callback_service_dispatches_other_node_requests() {
    let config = Config {
        partition: format!("service-oneway-remote-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    };
    let provider = Node::new(config.clone()).unwrap();
    let client = Node::new(config).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_calls = calls.clone();
    let _service = provider
        .serve_oneway_callback(TOPIC, INT32, move |bytes| {
            if Int32::decode(bytes).map_err(|_| Error::InvalidFrame)?.data != DATA {
                return Err(Error::InvalidFrame);
            }
            callback_calls.fetch_add(1, Ordering::AcqRel);
            Ok(())
        })
        .unwrap();
    client
        .request_oneway(TOPIC, INT32, five(), Duration::from_secs(3))
        .unwrap();
    for _ in 0..300 {
        if calls.load(Ordering::Acquire) == 1 {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(calls.load(Ordering::Acquire), 1);
}

#[test]
fn queued_callback_error_is_counted_and_following_request_runs() {
    let config = Config {
        partition: format!("service-oneway-error-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    };
    let provider = Node::new(config.clone()).unwrap();
    let client = Node::new(config).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_calls = calls.clone();
    let _service = provider
        .serve_oneway_callback(TOPIC, INT32, move |_| {
            if callback_calls.fetch_add(1, Ordering::AcqRel) == 0 {
                Err(Error::InvalidFrame)
            } else {
                Ok(())
            }
        })
        .unwrap();
    client
        .request_oneway(TOPIC, INT32, five(), Duration::from_secs(3))
        .unwrap();
    for _ in 0..300 {
        if provider.statistics().routing_errors == 1 {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(provider.statistics().routing_errors, 1);
    assert_eq!(calls.load(Ordering::Acquire), 1);
    client
        .request_oneway(TOPIC, INT32, five(), Duration::from_secs(3))
        .unwrap();
    for _ in 0..300 {
        if calls.load(Ordering::Acquire) == 2 {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(calls.load(Ordering::Acquire), 2);
    assert_eq!(provider.statistics().routing_errors, 1);
}

#[test]
fn remapped_callback_can_reenter_node_without_registry_lock() {
    let mut config = Config {
        partition: format!("service-oneway-remap-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    };
    config.add_topic_remap("/alias", "/actual").unwrap();
    let node = Arc::new(Node::new(config).unwrap());
    let nested_calls = Arc::new(AtomicUsize::new(0));
    let nested_count = nested_calls.clone();
    let _nested = node
        .serve_oneway_callback("/nested", INT32, move |_| {
            nested_count.fetch_add(1, Ordering::AcqRel);
            Ok(())
        })
        .unwrap();
    let weak_node = Arc::downgrade(&node);
    let _outer = node
        .serve_oneway_callback("/alias", INT32, move |bytes| {
            weak_node.upgrade().ok_or(Error::Shutdown)?.request_oneway(
                "/nested",
                INT32,
                bytes.to_vec(),
                RUST_WAIT,
            )
        })
        .unwrap();

    node.request_oneway("/alias", INT32, five(), RUST_WAIT)
        .unwrap();
    assert_eq!(nested_calls.load(Ordering::Acquire), 1);
    assert_eq!(node.service_type_signatures("/actual").unwrap().len(), 1);
}
