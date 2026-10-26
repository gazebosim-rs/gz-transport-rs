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
// Observable assertions from NodeTest.PubSubWrongTypesTwoSubscribers.
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use gz_msgs::{int32::Int32, prost::Message};
use gz_transport::{Config, Error, Node};

#[test]
fn original_pub_sub_wrong_types_two_subscribers() {
    let value = Int32 {
        data: 5,
        ..Default::default()
    };
    let partition = format!("wrong-two-subscribers-{}", uuid::Uuid::new_v4());
    let node1 = Node::new(Config {
        partition: partition.clone(),
        ..Default::default()
    })
    .unwrap();
    let node2 = Node::new(Config {
        partition,
        ..Default::default()
    })
    .unwrap();
    let publisher = node1.advertise_typed::<Int32>("/foo").unwrap();
    let matching = node1.subscribe("/foo", "gz.msgs.Int32").unwrap();
    let mismatching = node2.subscribe("/foo", "gz.msgs.Vector3d").unwrap();
    let good_executed = AtomicBool::new(false);
    let bad_executed = AtomicBool::new(false);

    thread::scope(|scope| {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !publisher.has_connections().unwrap() {
            assert!(
                Instant::now() < deadline,
                "matching subscriber did not connect"
            );
            thread::sleep(Duration::from_millis(10));
        }

        let (ready_tx, ready_rx) = mpsc::sync_channel(0);
        let (stop_tx, stop_rx) = mpsc::sync_channel(1);
        let bad_flag = &bad_executed;
        let bad_callback = scope.spawn(move || {
            ready_tx.send(()).unwrap();
            // Retain this subscription through the good callback and quiet window.
            while stop_rx.try_recv() == Err(mpsc::TryRecvError::Empty) {
                match mismatching.recv_timeout(Duration::from_millis(10)) {
                    Ok(_) => bad_flag.store(true, Ordering::Release),
                    Err(Error::Timeout) => {}
                    Err(other) => panic!("unexpected mismatching subscriber error: {other:?}"),
                }
            }
        });
        ready_rx.recv_timeout(Duration::from_secs(10)).unwrap();

        // The original callback wait has a fresh bound after connection readiness.
        let good_flag = &good_executed;
        let good_callback = scope.spawn(move || {
            let message = matching.recv_timeout(Duration::from_secs(10)).unwrap();
            assert_eq!(Int32::decode(message.data.as_slice()).unwrap().data, 5);
            good_flag.store(true, Ordering::Release);
        });

        assert_eq!(publisher.publish_typed(&value), Ok(()));
        good_callback.join().unwrap();
        assert!(good_executed.load(Ordering::Acquire));
        assert!(!bad_executed.load(Ordering::Acquire));

        // Supplemental quiet check retains the bad subscription after good delivery.
        thread::sleep(Duration::from_millis(300));
        stop_tx.send(()).unwrap();
        bad_callback.join().unwrap();
        assert!(!bad_executed.load(Ordering::Acquire));
    });
}
