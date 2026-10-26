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
// Observable assertions from NodeTest.PubSubWrongTypesOnPublish.
use std::{
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use gz_msgs::{int32::Int32, prost::Message, vector3d::Vector3d};
use gz_transport::{Config, Error, Node};

#[test]
fn original_pub_sub_wrong_types_on_publish() {
    let right = Int32 {
        data: 5,
        ..Default::default()
    };
    let wrong = Vector3d {
        x: 1.0,
        y: 2.0,
        z: 3.0,
        ..Default::default()
    };
    let node = Node::new(Config {
        partition: format!("wrong-type-publish-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    })
    .unwrap();
    let publisher = node.advertise_typed::<Int32>("/foo").unwrap();
    let subscription = node.subscribe("/foo", "gz.msgs.Int32").unwrap();
    let callback_executed = AtomicBool::new(false);

    thread::scope(|scope| {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !publisher.has_connections().unwrap() {
            assert!(Instant::now() < deadline, "subscriber did not connect");
            thread::sleep(Duration::from_millis(10));
        }

        // Keep the original callback wait separate from connection readiness.
        let executed = &callback_executed;
        let callback = scope.spawn(move || {
            let message = subscription.recv_timeout(Duration::from_secs(10)).unwrap();
            assert_eq!(Int32::decode(message.data.as_slice()).unwrap().data, 5);
            executed.store(true, Ordering::Release);
        });

        assert_eq!(publisher.publish_typed(&wrong), Err(Error::InvalidFrame));
        assert!(!callback_executed.load(Ordering::Acquire));

        assert_eq!(publisher.publish_typed(&right), Ok(()));
        callback.join().unwrap();
        assert!(callback_executed.load(Ordering::Acquire));
    });
}
