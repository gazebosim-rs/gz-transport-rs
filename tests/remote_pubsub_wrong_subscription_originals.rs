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
// Fixed observations from the typed/raw twoProcPubSub wrong-subscription cases.
use gz_msgs::{prost::Message, vector3d::Vector3d};
use gz_transport::{Config, Error, Node};
use std::{
    process::{Child, Command},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

struct NativePublisher(Child);
impl NativePublisher {
    fn start(config: &Config) -> Self {
        let path = std::env::var("GZ_NATIVE_VECTOR_PUBLISHER")
            .expect("the pinned native twoProcsPublisher_aux is required");
        Self(
            Command::new(path)
                .arg(&config.partition)
                .env("GZ_IP", config.interface.to_string())
                .env("GZ_DISCOVERY_MULTICAST_IP", config.multicast.to_string())
                .env("GZ_DISCOVERY_MSG_PORT", config.message_port.to_string())
                .env("GZ_DISCOVERY_SRV_PORT", config.service_port.to_string())
                .env("GZ_TRANSPORT_IMPLEMENTATION", "zeromq")
                .env("GZ_TRANSPORT_TOPIC_STATISTICS", "0")
                .spawn()
                .unwrap(),
        )
    }
    fn finish(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                assert!(status.success());
                return;
            }
            assert!(Instant::now() < deadline, "native publisher did not stop");
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for NativePublisher {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

// Retire the polling observer before scope joins, including assertion unwinding.
struct StopObserverOnDrop<'a>(&'a AtomicBool);
impl Drop for StopObserverOnDrop<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

#[test]
fn original_remote_pub_sub_wrong_types_on_subscription() {
    observe_wrong_subscription(false);
}

#[test]
fn original_remote_pub_raw_sub_wrong_types_on_subscription() {
    observe_wrong_subscription(true);
}

fn observe_wrong_subscription(raw: bool) {
    let config = Config {
        partition: format!("remote-wrong-subscription-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    };
    // This is the original executable, including both fixed publication times.
    let mut publisher = NativePublisher::start(&config);
    let executed = AtomicBool::new(false);
    let stopping = AtomicBool::new(false);
    let node = Node::new(config).unwrap();
    let subscription = node.subscribe("/foo", "gz.msgs.Int32").unwrap();
    // Supplemental matching traffic proves the native publisher really delivered.
    let witness = node.subscribe("/foo", "gz.msgs.Vector3d").unwrap();
    thread::scope(|scope| {
        let _stop_on_exit = StopObserverOnDrop(&stopping);
        let (ready_tx, ready_rx) = mpsc::sync_channel(0);
        let callback_executed = &executed;
        let observer_stopping = &stopping;
        let observer = scope.spawn(move || {
            ready_tx.send(()).unwrap();
            while !observer_stopping.load(Ordering::Acquire) {
                let result = if raw {
                    subscription
                        .recv_with_info_timeout(Duration::from_millis(10))
                        .map(|_| ())
                } else {
                    subscription
                        .recv_timeout(Duration::from_millis(10))
                        .map(|_| ())
                };
                match result {
                    Ok(()) => callback_executed.store(true, Ordering::Release),
                    Err(Error::Timeout) => {}
                    Err(other) => panic!("unexpected subscription error: {other:?}"),
                }
            }
        });
        ready_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        // Both originals observe the callback flag after exactly this wait.
        thread::sleep(Duration::from_millis(1500));
        assert!(!executed.load(Ordering::Acquire));

        let message = witness.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(message.type_name, "gz.msgs.Vector3d");
        let value = Vector3d::decode(message.data.as_slice()).unwrap();
        assert_eq!((value.x, value.y, value.z), (1.0, 2.0, 3.0));
        stopping.store(true, Ordering::Release);
        observer.join().unwrap();
        assert!(!executed.load(Ordering::Acquire));
    });
    publisher.finish();
}
