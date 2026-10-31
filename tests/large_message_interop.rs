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
// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use gz_msgs::msgs::{Bytes, Vector3d};
use gz_transport::{Config, Node};
use std::{
    io::{BufRead, BufReader, Read},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[path = "support/network_clock_config.rs"]
mod ports;

const SIZE: usize = 256 * 1024;

struct Peer {
    child: Option<Child>,
    reader: Option<JoinHandle<Vec<String>>>,
    ready: mpsc::Receiver<()>,
}
impl Peer {
    fn start(settings: &Config, publisher: bool) -> Self {
        let path = if publisher {
            PathBuf::from(std::env::var_os("GZ_REFERENCE_SOURCE").unwrap())
                .join("build/bin/twoProcsPublisher_aux")
        } else {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("artifacts/large-message-peer")
        };
        let mut command = Command::new(path);
        if publisher {
            command.arg(&settings.partition);
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
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let output = child.stdout.take().unwrap();
        let (ready, received) = mpsc::sync_channel(1);
        let reader = thread::spawn(move || {
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
    fn join(mut self, require_passed: bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.child.as_mut().unwrap().try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline, "native peer did not exit");
            thread::sleep(Duration::from_millis(10));
        }
        let status = self.child.take().unwrap().wait().unwrap();
        let output = self.reader.take().unwrap().join().unwrap();
        assert!(status.success(), "{status:?}: {output:?}");
        if require_passed {
            assert!(output.iter().any(|line| line == "PASSED"), "{output:?}");
        }
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
fn large_message_original_receives_unchanged_native_publisher_payload() {
    let (settings, _reservations) = ports::config();
    let publisher = Peer::start(&settings, true);
    {
        let node = Node::new(settings).unwrap();
        // One lock observes the flag and its complete payload coherently. The
        // persistent error bit supplements the original's final-value check.
        let received = Arc::new(Mutex::new((false, Vec::new(), false)));
        let observed = received.clone();
        let _subscription = node
            .subscribe_typed_callback::<Bytes, _>("/large_msg", move |message, _| {
                let mut state = observed.lock().unwrap();
                state.0 = true;
                state.1.clone_from(&message.data);
                state.2 |= message.data.len() != SIZE || message.data.iter().any(|b| *b != b'S');
                Ok(())
            })
            .unwrap();
        thread::sleep(Duration::from_millis(2500));
        let state = received.lock().unwrap();
        assert!(state.0, "large message was not received");
        assert_eq!(state.1.len(), SIZE);
        assert_eq!(state.1, vec![b'S'; SIZE]);
        assert!(!state.2);
    }
    // Native destruction order retires the subscriber before joining the child.
    publisher.join(false);
}

#[test]
fn large_message_native_receiver_checks_rust_original_publication_schedule() {
    let (settings, _reservations) = ports::config();
    let receiver = Peer::start(&settings, false);
    // Setup only: the copied receiver retains its original 2500ms receipt wait.
    receiver.ready.recv_timeout(Duration::from_secs(5)).unwrap();
    let node = Node::new(settings).unwrap();
    let publisher = node.advertise_typed::<Vector3d>("/foo").unwrap();
    let large = node.advertise_typed::<Bytes>("/large_msg").unwrap();
    let message = Vector3d {
        x: 1.,
        y: 2.,
        z: 3.,
        ..Default::default()
    };
    let bytes = Bytes {
        data: vec![b'S'; SIZE],
        ..Default::default()
    };
    thread::sleep(Duration::from_millis(500));
    publisher.publish_typed(&message).unwrap();
    large.publish_typed(&bytes).unwrap();
    thread::sleep(Duration::from_millis(1500));
    publisher.publish_typed(&message).unwrap();
    large.publish_typed(&bytes).unwrap();
    thread::sleep(Duration::from_millis(1000));
    receiver.join(true);
}
