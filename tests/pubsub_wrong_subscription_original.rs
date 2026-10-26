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
// Observable assertions from NodeTest.PubSubWrongTypesOnSubscription.
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use gz_msgs::vector3d::Vector3d;
use gz_transport::{Config, Error, Node};

#[test]
fn original_pub_sub_wrong_types_on_subscription() {
    let wrong_for_callback = Vector3d {
        x: 1.0,
        y: 2.0,
        z: 3.0,
        ..Default::default()
    };
    let node = Node::new(Config {
        partition: format!("wrong-type-subscription-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    })
    .unwrap();
    let publisher = node.advertise_typed::<Vector3d>("/foo").unwrap();
    let subscription = node.subscribe("/foo", "gz.msgs.Int32").unwrap();
    let callback_executed = AtomicBool::new(false);

    thread::scope(|scope| {
        let (ready_tx, ready_rx) = mpsc::sync_channel(0);
        let executed = &callback_executed;
        let callback = scope.spawn(move || {
            ready_tx.send(()).unwrap();
            match subscription.recv_timeout(Duration::from_millis(300)) {
                Ok(_) => executed.store(true, Ordering::Release),
                Err(Error::Timeout) => {}
                Err(other) => panic!("unexpected subscriber error: {other:?}"),
            }
        });
        ready_rx.recv_timeout(Duration::from_secs(10)).unwrap();

        // Preserve the original 100 ms subscription-settling interval.
        thread::sleep(Duration::from_millis(100));
        assert_eq!(publisher.publish_typed(&wrong_for_callback), Ok(()));
        assert!(!callback_executed.load(Ordering::Acquire));

        // Supplemental quiet check: the original only checks immediately.
        callback.join().unwrap();
        assert!(!callback_executed.load(Ordering::Acquire));
    });
}
