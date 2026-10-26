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
// Owned Rust receive/dispatch adaptation: Copyright (C) 2026 Open Source Robotics Foundation.
// SPDX-License-Identifier: Apache-2.0
use gz_msgs::{int32::Int32, prost::Message};
use gz_transport::{Config, Error, MessageInfo, Node, Subscription};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

const TYPE: &str = "gz.msgs.Int32";
const TOPIC: &str = "/foo";

#[derive(Debug, PartialEq, Eq)]
struct Observation {
    data: i32,
    info: Option<(String, String, String, bool)>,
}

struct MemberClient {
    completed: SyncSender<Observation>,
    calls: Arc<AtomicUsize>,
}

impl MemberClient {
    fn cb(&self, value: &Int32) {
        assert_eq!(value.data, 5);
        self.calls.fetch_add(1, Ordering::AcqRel);
        self.completed
            .try_send(Observation {
                data: value.data,
                info: None,
            })
            .unwrap();
    }

    fn cb_info(&self, value: &Int32, info: MessageInfo<'_>, partition: &str) {
        assert_eq!(value.data, 5);
        assert_eq!(info.topic, TOPIC);
        assert_eq!(info.partition, partition);
        assert_eq!(info.type_name, TYPE);
        assert!(info.intra_process);
        self.calls.fetch_add(1, Ordering::AcqRel);
        self.completed
            .try_send(Observation {
                data: value.data,
                info: Some((
                    info.topic.into(),
                    info.partition.into(),
                    info.type_name.into(),
                    info.intra_process,
                )),
            })
            .unwrap();
    }
}

struct OwnedClient {
    node: Node,
    partition: String,
    stop: Arc<AtomicBool>,
    dispatcher: Option<JoinHandle<()>>,
    completed: Option<Receiver<Observation>>,
    calls: Arc<AtomicUsize>,
}

impl OwnedClient {
    fn new() -> Self {
        let config_partition = format!("member-{}", uuid::Uuid::new_v4());
        let node = Node::new(Config {
            partition: config_partition.clone(),
            ..Default::default()
        })
        .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        Self {
            node,
            partition: format!("/{config_partition}"),
            stop: Arc::new(AtomicBool::new(false)),
            dispatcher: None,
            completed: None,
            calls,
        }
    }

    fn subscribe(&mut self, with_info: bool) {
        assert!(matches!(
            self.node.subscribe("Bad Topic", TYPE),
            Err(Error::InvalidName)
        ));
        let subscription = self.node.subscribe(TOPIC, TYPE).unwrap();
        let (completed, rx) = mpsc::sync_channel(2);
        self.completed = Some(rx);
        let client = MemberClient {
            completed,
            calls: self.calls.clone(),
        };
        let stop = self.stop.clone();
        let partition = self.partition.clone();
        self.dispatcher = Some(thread::spawn(move || {
            dispatch(subscription, client, stop, with_info, &partition)
        }));
    }

    fn send_some_data(&self) {
        assert!(matches!(
            self.node.advertise("invalid topic", TYPE),
            Err(Error::InvalidName)
        ));
        assert!(matches!(
            self.node.advertise("invalid topic", TYPE),
            Err(Error::InvalidName)
        ));
        let publisher = self.node.advertise(TOPIC, TYPE).unwrap();
        assert!(publisher.has_connections().unwrap());
        publisher.publish(int32_five()).unwrap();
    }

    fn close(mut self) -> Arc<AtomicUsize> {
        self.stop.store(true, Ordering::Release);
        self.dispatcher.take().unwrap().join().unwrap();
        self.calls.clone()
    }
}

impl Drop for OwnedClient {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(dispatcher) = self.dispatcher.take() {
            let _ = dispatcher.join();
        }
    }
}

fn dispatch(
    subscription: Subscription,
    client: MemberClient,
    stop: Arc<AtomicBool>,
    with_info: bool,
    partition: &str,
) {
    while !stop.load(Ordering::Acquire) {
        match subscription.recv_with_info_timeout(Duration::from_millis(10)) {
            Ok(message) if !stop.load(Ordering::Acquire) => {
                let value = Int32::decode(message.data.as_slice()).unwrap();
                if with_info {
                    client.cb_info(&value, message.info(), partition);
                } else {
                    client.cb(&value);
                }
            }
            Ok(_) | Err(Error::Timeout) => {}
            Err(Error::Shutdown) => break,
            Err(error) => panic!("member dispatch failed: {error}"),
        }
    }
}

fn int32_five() -> Vec<u8> {
    Int32 {
        data: 5,
        ..Default::default()
    }
    .encode_to_vec()
}

fn original_member_case(with_info: bool) {
    let mut owner = OwnedClient::new();
    owner.subscribe(with_info);
    // The pinned native originals settle subscribers for exactly 100 ms.
    thread::sleep(Duration::from_millis(100));
    owner.send_some_data();
    let observed = owner
        .completed
        .as_ref()
        .unwrap()
        .recv_timeout(Duration::from_secs(10))
        .unwrap();
    assert_eq!(observed.data, 5);
    assert_eq!(owner.calls.load(Ordering::Acquire), 1);
    if with_info {
        assert_eq!(
            observed.info,
            Some((TOPIC.into(), owner.partition.clone(), TYPE.into(), true))
        );
    } else {
        assert_eq!(observed.info, None);
    }
    thread::sleep(Duration::from_millis(100));
    assert_eq!(owner.calls.load(Ordering::Acquire), 1);
    let calls = owner.close();
    assert_eq!(calls.load(Ordering::Acquire), 1);
}

#[test]
fn original_class_member_callback_message() {
    original_member_case(false);
}

#[test]
fn original_class_member_callback_message_info() {
    original_member_case(true);
}

#[test]
fn owned_dispatch_retirement_with_surviving_publisher() {
    let mut owner = OwnedClient::new();
    owner.subscribe(false);
    let publisher = owner.node.advertise(TOPIC, TYPE).unwrap();
    assert!(publisher.has_connections().unwrap());
    publisher.publish(int32_five()).unwrap();
    assert_eq!(
        owner
            .completed
            .as_ref()
            .unwrap()
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .data,
        5
    );
    let calls = owner.close();
    publisher.publish(int32_five()).unwrap();
    thread::sleep(Duration::from_millis(100));
    assert_eq!(calls.load(Ordering::Acquire), 1);
}
