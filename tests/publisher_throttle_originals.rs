/*
 * Copyright (C) 2014-2015 Open Source Robotics Foundation
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
// NodeTest.PubThrottled and AdvertiseOptionsTest message-option assertions.
use gz_msgs::{int32::Int32, prost::Message, vector3d::Vector3d};
use gz_transport::{AdvertiseOptions, Config, Error, Node, Scope};
use std::{
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    thread,
    time::Duration,
};

#[test]
fn original_message_options_default_constructor() {
    let options = AdvertiseOptions::default();
    assert_eq!(options.scope(), Scope::All);
    assert!(!options.throttled());
    assert_eq!(options.msgs_per_sec(), u64::MAX);
}

#[test]
fn original_message_options_copy_constructor() {
    let mut first = AdvertiseOptions::default();
    first.set_scope(Scope::Host);
    first.set_msgs_per_sec(10);
    let second = first;
    assert_eq!(first, second);
}

#[test]
fn original_message_options_assignment() {
    let mut first = AdvertiseOptions::default();
    let mut second = AdvertiseOptions::default();
    assert_eq!(second, AdvertiseOptions::default());
    first.set_scope(Scope::Process);
    first.set_msgs_per_sec(10);
    second = first;
    assert_eq!(first, second);
}

#[test]
fn original_message_options_equality() {
    let mut first = AdvertiseOptions::default();
    let mut second = AdvertiseOptions::default();
    first.set_scope(Scope::Process);
    first.set_msgs_per_sec(10);
    assert!(!(first == second));
    assert!(first != second);
    second.set_scope(Scope::Process);
    second.set_msgs_per_sec(10);
    assert!(first == second);
    assert!(!(first != second));
}

#[test]
fn original_message_options_accessors() {
    let mut options = AdvertiseOptions::default();
    assert_eq!(options.scope(), Scope::All);
    options.set_scope(Scope::Host);
    assert_eq!(options.scope(), Scope::Host);
    assert!(!options.throttled());
    assert_eq!(options.msgs_per_sec(), u64::MAX);
    options.set_msgs_per_sec(10);
    assert_eq!(options.msgs_per_sec(), 10);
    assert!(options.throttled());
}

struct StopOnDrop<'a>(&'a AtomicBool);
impl Drop for StopOnDrop<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

#[test]
fn original_pub_throttled() {
    let node = Node::new(Config {
        partition: format!("throttle-{}", uuid::Uuid::new_v4()),
        ..Config::default()
    })
    .unwrap();
    let mut options = AdvertiseOptions::default();
    options.set_msgs_per_sec(1);
    let publisher = node
        .advertise_typed_with_options::<Int32>("/foo", options)
        .unwrap();
    let subscription = node.subscribe("/foo", "gz.msgs.Int32").unwrap();
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
        assert!(publisher.throttled_update_ready().unwrap());
        for _ in 0..3 {
            publisher.publish_typed(&value).unwrap();
            assert!(!publisher.throttled_update_ready().unwrap());
            thread::sleep(Duration::from_millis(100));
        }
        // Preserve the native checkpoint while the observer is still live.
        assert_eq!(count.load(Ordering::Acquire), 1);
        drop(guard);
        callback.join().unwrap();
    });
}

#[test]
fn zero_rate_suppresses_typed_and_raw_but_preserves_validation_and_shutdown() {
    let node = Node::new(Config {
        partition: format!("zero-throttle-{}", uuid::Uuid::new_v4()),
        max_message_bytes: 32,
        ..Config::default()
    })
    .unwrap();
    let mut options = AdvertiseOptions::default();
    options.set_msgs_per_sec(0);
    assert!(options.throttled());
    let publisher = node
        .advertise_typed_with_options::<Int32>("/zero", options)
        .unwrap();
    let subscription = node.subscribe("/zero", "gz.msgs.Int32").unwrap();
    assert!(!publisher.throttled_update_ready().unwrap());
    assert!(!publisher.throttled_update_ready().unwrap());
    let value = Int32 {
        data: 5,
        ..Default::default()
    };
    assert_eq!(
        publisher.publish_typed(&Vector3d::default()),
        Err(Error::InvalidFrame)
    );
    assert_eq!(publisher.publish(vec![0; 33]), Err(Error::InvalidFrame));
    publisher.publish_typed(&value).unwrap();
    publisher.publish(value.encode_to_vec()).unwrap();
    assert_eq!(
        subscription.recv_timeout(Duration::from_millis(100)),
        Err(Error::Timeout)
    );
    node.cancel();
    assert_eq!(publisher.publish_typed(&value), Err(Error::Shutdown));
    assert_eq!(
        publisher.publish(value.encode_to_vec()),
        Err(Error::Shutdown)
    );
}

#[test]
fn borrowed_publisher_typed_and_raw_calls_share_one_rate_interval() {
    let node = Node::new(Config {
        partition: format!("shared-throttle-{}", uuid::Uuid::new_v4()),
        ..Config::default()
    })
    .unwrap();
    let mut options = AdvertiseOptions::default();
    options.set_msgs_per_sec(1);
    let publisher = node
        .advertise_typed_with_options::<Int32>("/shared", options)
        .unwrap();
    let subscription = node.subscribe("/shared", "gz.msgs.Int32").unwrap();
    let borrowed = &publisher;
    let value = Int32 {
        data: 5,
        ..Default::default()
    };
    publisher.publish_typed(&value).unwrap();
    assert!(!borrowed.throttled_update_ready().unwrap());
    // Both successful calls use the same interval, through an ordinary borrow.
    // This does not add native C++ Publisher copy construction to the Rust API.
    borrowed.publish(value.encode_to_vec()).unwrap();
    assert_eq!(
        Int32::decode(
            subscription
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .data
                .as_slice()
        )
        .unwrap()
        .data,
        5
    );
    assert_eq!(
        subscription.recv_timeout(Duration::from_millis(30)),
        Err(Error::Timeout)
    );
}
