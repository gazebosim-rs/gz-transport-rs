/*
 * Copyright (C) 2014, 2016 Open Source Robotics Foundation
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
// NodeTest.SubThrottled and the three SubscribeOptionsTest rate cases.
use gz_msgs::{int32::Int32, prost::Message};
use gz_transport::{Config, Error, Node, SubscribeOptions};
use std::{
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    thread,
    time::Duration,
};

#[test]
fn original_subscribe_options_copy_constructor() {
    let mut first = SubscribeOptions::default();
    first.set_msgs_per_sec(2);
    assert_eq!(first.msgs_per_sec(), 2);
    let second = first;
    assert_eq!(second.msgs_per_sec(), first.msgs_per_sec());
}

#[test]
fn original_subscribe_options_accessors() {
    let mut options = SubscribeOptions::default();
    assert_eq!(options.msgs_per_sec(), u64::MAX);
    options.set_msgs_per_sec(3);
    assert_eq!(options.msgs_per_sec(), 3);
}

#[test]
fn original_subscribe_options_throttled() {
    let mut options = SubscribeOptions::default();
    assert!(!options.throttled());
    options.set_msgs_per_sec(3);
    assert!(options.throttled());
}

struct StopOnDrop<'a>(&'a AtomicBool);
impl Drop for StopOnDrop<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

#[test]
fn original_sub_throttled() {
    let node = Node::new(Config {
        partition: format!("subscriber-throttle-{}", uuid::Uuid::new_v4()),
        ..Config::default()
    })
    .unwrap();
    let publisher = node.advertise_typed::<Int32>("/foo").unwrap();
    let mut options = SubscribeOptions::default();
    options.set_msgs_per_sec(1);
    let subscription = node
        .subscribe_with_options("/foo", "gz.msgs.Int32", options)
        .unwrap();
    let count = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let value = Int32 {
        data: 5,
        ..Default::default()
    };
    thread::scope(|scope| {
        let counter = &count;
        let stopped = &stop;
        let callback = scope.spawn(move || {
            while !stopped.load(Ordering::Acquire) {
                match subscription.recv_timeout(Duration::from_millis(10)) {
                    Ok(message) => {
                        assert_eq!(Int32::decode(message.data.as_slice()).unwrap().data, 5);
                        counter.fetch_add(1, Ordering::Release);
                    }
                    Err(Error::Timeout) => {}
                    Err(error) => panic!("callback receive failed: {error:?}"),
                }
            }
        });
        let guard = StopOnDrop(&stop);
        for _ in 0..3 {
            assert!(publisher.throttled_update_ready().unwrap());
            publisher.publish_typed(&value).unwrap();
            thread::sleep(Duration::from_millis(100));
        }
        // Preserve the original checkpoint while the observer remains live.
        assert_eq!(count.load(Ordering::Acquire), 1);
        drop(guard);
        callback.join().unwrap();
    });
}

#[test]
fn one_registration_shares_local_and_other_node_admission_but_new_receivers_start_independently() {
    let config = Config {
        partition: format!("shared-subscription-{}", uuid::Uuid::new_v4()),
        ..Config::default()
    };
    let owner = Node::new(config.clone()).unwrap();
    let peer = Node::new(config).unwrap();
    let local = owner.advertise_typed::<Int32>("/shared").unwrap();
    let foreign = peer.advertise_typed::<Int32>("/shared").unwrap();
    let mut options = SubscribeOptions::default();
    options.set_msgs_per_sec(1);
    let limited = owner
        .subscribe_with_options("/shared", "gz.msgs.Int32", options)
        .unwrap();
    let witness = owner.subscribe("/shared", "gz.msgs.Int32").unwrap();
    local
        .publish_typed(&Int32 {
            data: 1,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        Int32::decode(
            limited
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .data
                .as_slice()
        )
        .unwrap()
        .data,
        1
    );
    assert_eq!(
        Int32::decode(
            witness
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .data
                .as_slice()
        )
        .unwrap()
        .data,
        1
    );
    foreign
        .publish_typed(&Int32 {
            data: 2,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        Int32::decode(
            witness
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .data
                .as_slice()
        )
        .unwrap()
        .data,
        2
    );
    assert_eq!(
        limited.recv_timeout(Duration::from_millis(30)),
        Err(Error::Timeout)
    );
    let fresh = owner
        .subscribe_with_options("/shared", "gz.msgs.Int32", options)
        .unwrap();
    foreign
        .publish_typed(&Int32 {
            data: 3,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        Int32::decode(
            fresh
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .data
                .as_slice()
        )
        .unwrap()
        .data,
        3
    );
    assert_eq!(
        Int32::decode(
            witness
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .data
                .as_slice()
        )
        .unwrap()
        .data,
        3
    );
    assert_eq!(
        limited.recv_timeout(Duration::from_millis(30)),
        Err(Error::Timeout)
    );
    assert_eq!(owner.statistics().subscription_clock_errors, 0);
}
