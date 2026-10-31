// Copyright (C) 2014, 2025, 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
/*
 * Copyright (C) 2014, 2025 Open Source Robotics Foundation
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
// Pinned twoProcPubSub.PubSubTwoProcsMixedSubscribers and its complete helper.
use gz_msgs::{msgs::Vector3d, prost::Message as _};
use gz_transport::{CallbackSubscription, Config, Node};
use std::{
    io::{BufRead, BufReader, Read},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[path = "support/network_clock_config.rs"]
mod ports;

struct Peer {
    child: Option<Child>,
    reader: Option<JoinHandle<Vec<String>>>,
    ready: Receiver<()>,
}
impl Peer {
    fn start(settings: &Config, original_subscriber: bool) -> Self {
        let path = if original_subscriber {
            PathBuf::from(std::env::var_os("GZ_REFERENCE_SOURCE").unwrap())
                .join("build/bin/twoProcsPubSubMixedSubscribers_aux")
        } else {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("artifacts/mixed-subscriber-peer")
        };
        let mut command = Command::new(path);
        if original_subscriber {
            command.arg(&settings.partition).env(
                "GTEST_FILTER",
                "twoProcPubSub.PubSubTwoProcsTwoNodesSubscriber",
            );
        } else {
            command.env_remove("GTEST_FILTER");
        }
        let mut child = command
            .env("GZ_PARTITION", &settings.partition)
            .env("GZ_IP", settings.interface.to_string())
            .env("GZ_DISCOVERY_MULTICAST_IP", settings.multicast.to_string())
            .env("GZ_DISCOVERY_MSG_PORT", settings.message_port.to_string())
            .env("GZ_DISCOVERY_SRV_PORT", settings.service_port.to_string())
            .env("GZ_TRANSPORT_TOPIC_STATISTICS", "0")
            .env("GZ_TRANSPORT_IMPLEMENTATION", "zeromq")
            .env_remove("GZ_RELAY")
            .env_remove("GTEST_LIST_TESTS")
            .env("GTEST_REPEAT", "1")
            .env("GTEST_COLOR", "no")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let output = child.stdout.take().unwrap();
        let (ready, received) = mpsc::sync_channel(1);
        let reader = thread::spawn(move || {
            // Both selected helpers have finite output. Bound diagnostics too.
            BufReader::new(output)
                .take(64 * 1024)
                .lines()
                .map(|line| {
                    let line = line.unwrap();
                    if line == "READY" {
                        ready.try_send(()).unwrap();
                    }
                    line
                })
                .collect()
        });
        Self {
            child: Some(child),
            reader: Some(reader),
            ready: received,
        }
    }
    fn publisher_ready(&self) {
        self.ready.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    fn join(mut self, expected: &str) -> Vec<String> {
        let deadline = Instant::now() + Duration::from_secs(15);
        while self.child.as_mut().unwrap().try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline, "native peer did not exit");
            thread::sleep(Duration::from_millis(10));
        }
        let status = self.child.take().unwrap().wait().unwrap();
        let output = self.reader.take().unwrap().join().unwrap();
        assert!(status.success(), "{status:?}: {output:?}");
        assert!(output.iter().any(|line| line == expected), "{output:?}");
        output
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[test]
fn mixed_subscriber_original_uses_unchanged_native_receivers() {
    let (settings, _reservations) = ports::config();
    let node = Node::new(settings.clone()).unwrap();
    let publisher = node.advertise_typed::<Vector3d>("/foo").unwrap();
    assert!(!publisher.connection_snapshot().unwrap());
    let subscriber = Peer::start(&settings, true);
    let value = Vector3d {
        x: 1.0,
        y: 2.0,
        z: 3.0,
        ..Default::default()
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while !publisher.connection_snapshot().unwrap() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    for _ in 0..10 {
        publisher.publish_typed(&value).unwrap();
        thread::sleep(Duration::from_millis(500));
    }
    let _ = subscriber.join("[  PASSED  ] 1 test.");
}

struct Observed {
    flags: [AtomicBool; 4],
    entries: [AtomicUsize; 4],
    invalid: AtomicBool,
}
impl Observed {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            flags: std::array::from_fn(|_| AtomicBool::new(false)),
            entries: std::array::from_fn(|_| AtomicUsize::new(0)),
            invalid: AtomicBool::new(false),
        })
    }
    fn value(&self, index: usize, message: &Vector3d) {
        // The retained EXPECT_DOUBLE_EQ predicates allow four ULPs. All three
        // expected values are positive, so ordered positive bits suffice here.
        if ![message.x, message.y, message.z]
            .into_iter()
            .zip([1.0_f64, 2.0, 3.0])
            .all(|(actual, expected)| {
                actual > 0.0 && actual.to_bits().abs_diff(expected.to_bits()) <= 4
            })
        {
            self.invalid.store(true, Ordering::Release);
        }
        self.flags[index].store(true, Ordering::Release);
    }
    fn flags(&self) -> [bool; 4] {
        self.flags
            .each_ref()
            .map(|flag| flag.load(Ordering::Acquire))
    }
    fn entries(&self) -> [usize; 4] {
        self.entries.each_ref().map(|n| n.load(Ordering::Acquire))
    }
    fn reset(&self) {
        for flag in &self.flags {
            flag.store(false, Ordering::Release);
        }
    }
}

#[test]
fn mixed_subscriber_original_values_preserve_four_ulp_boundaries() {
    for offset in [-4_i64, 0, 4, 5] {
        let observed = Observed::new();
        let value = Vector3d {
            x: f64::from_bits(1.0_f64.to_bits().checked_add_signed(offset).unwrap()),
            y: 2.0,
            z: 3.0,
            ..Default::default()
        };
        observed.value(0, &value);
        assert_eq!(observed.invalid.load(Ordering::Acquire), offset == 5);
        assert!(observed.flags()[0]);
    }
    for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let observed = Observed::new();
        observed.value(
            0,
            &Vector3d {
                x: 1.0,
                y: invalid,
                z: 3.0,
                ..Default::default()
            },
        );
        assert!(observed.invalid.load(Ordering::Acquire));
    }
}
fn typed(node: &Node, observed: &Arc<Observed>, index: usize) -> CallbackSubscription {
    let observed = observed.clone();
    node.subscribe_typed_callback::<Vector3d, _>("/foo", move |value, info| {
        observed.entries[index].fetch_add(1, Ordering::AcqRel);
        if info.intra_process {
            observed.invalid.store(true, Ordering::Release);
        }
        observed.value(index, value);
        Ok(())
    })
    .unwrap()
}

fn join_publisher(publisher: Peer) {
    let output = publisher.join("PUBLISHED 10");
    assert!(
        output.iter().any(|line| line == "CONNECTIONS 0"),
        "{output:?}"
    );
}

fn rust_subscriber(retain_owner: bool) {
    let (settings, _reservations) = ports::config();
    // Preserve the original publisher's no-subscribers observation before the
    // subscriber process would start. This handshake only orders that setup.
    let mut publisher = Some(Peer::start(&settings, false));
    publisher.as_ref().unwrap().publisher_ready();
    let observed = Observed::new();
    let retired_entries = {
        let node = Node::new(settings).unwrap();
        // These two owned handles retain the native Node-owned legacy callbacks
        // through Node-wide unsubscribe. The two later handles map CreateSubscriber.
        let _legacy = typed(&node, &observed, 0);
        let raw = observed.clone();
        let _raw = node
            .subscribe_raw_callback("/foo", "gz.msgs.Vector3d", move |bytes, info| {
                raw.entries[1].fetch_add(1, Ordering::AcqRel);
                if info.type_name != "gz.msgs.Vector3d" || info.intra_process {
                    raw.invalid.store(true, Ordering::Release);
                }
                match Vector3d::decode(bytes) {
                    Ok(value) => raw.value(1, &value),
                    Err(_) => {
                        raw.invalid.store(true, Ordering::Release);
                        raw.flags[1].store(true, Ordering::Release);
                    }
                }
                Ok(())
            })
            .unwrap();
        let sub = typed(&node, &observed, 2);
        assert!(sub.is_active());
        let sub2 = typed(&node, &observed, 3);
        assert!(sub2.is_active());
        let mut interval = 100;
        while observed.flags() != [true; 4] {
            thread::sleep(Duration::from_millis(100));
            interval -= 1;
            if interval == 0 {
                break;
            }
        }
        assert_eq!(observed.flags(), [true; 4]);
        assert!(!observed.invalid.load(Ordering::Acquire));
        observed.reset();
        assert!(sub.unsubscribe());
        let retired_entries = observed.entries()[2];
        thread::sleep(Duration::from_millis(1000));
        assert_eq!(observed.flags(), [true, true, false, true]);
        assert_eq!(observed.entries()[2], retired_entries);
        assert!(!observed.invalid.load(Ordering::Acquire));
        observed.reset();
        node.unsubscribe("/foo").unwrap();
        let retired_entries = observed.entries();
        thread::sleep(Duration::from_millis(1000));
        assert_eq!(observed.flags(), [false; 4]);
        assert_eq!(observed.entries(), retired_entries);
        assert!(!observed.invalid.load(Ordering::Acquire));
        if retain_owner {
            // Separate stronger control: keep this Node alive so remote-interest
            // retirement cannot be explained by owner destruction or BYE.
            join_publisher(publisher.take().unwrap());
        }
        retired_entries
    };
    // The original variant destroys subscribers and Node immediately after its
    // second quiet check, before waiting for the publisher's remaining calls.
    if let Some(publisher) = publisher {
        join_publisher(publisher);
    }
    // Supplemental: all callbacks remain quiet through the remaining original
    // publications too; counters cannot hide an entry whose panic was caught.
    assert_eq!(observed.entries(), retired_entries);
    assert_eq!(observed.flags(), [false; 4]);
}

#[test]
fn mixed_subscriber_native_publisher_preserves_all_rust_callback_lifetimes() {
    rust_subscriber(false);
}

#[test]
fn mixed_subscriber_remote_interest_retires_while_node_stays_alive() {
    rust_subscriber(true);
}
