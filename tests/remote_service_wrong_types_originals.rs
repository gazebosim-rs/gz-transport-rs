/*
 * Copyright (C) 2014 Open Source Robotics Foundation
 * Copyright (C) 2016 Open Source Robotics Foundation
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
// Observable assertions from twoProcSrvCall.SrvRequestWrongReq/WrongRep.
// Also twoProcSrvCallWithoutInput.SrvRequestWrongRep (2016).
use gz_msgs::{int32::Int32, prost::Message, vector3d::Vector3d};
use gz_transport::{Config, Error, Node, ServiceTypeResolution, ServiceTypes};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdout, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

// Keep a real native echo provider alive through both original observations.
struct NativePeer {
    child: Child,
    ready_reader: Option<thread::JoinHandle<BufReader<ChildStdout>>>,
    _stdout: Option<BufReader<ChildStdout>>,
}
impl NativePeer {
    fn start(config: &Config, inputless: bool) -> Self {
        let path = std::env::var("GZ_CPP_PEER")
            .unwrap_or_else(|_| format!("{}/artifacts/cpp-peer", env!("CARGO_MANIFEST_DIR")));
        let mut child = Command::new(path)
            .args([
                "service-info-reference",
                "/foo",
                if inputless { "inputless" } else { "ordinary" },
            ])
            .env("GZ_PARTITION", &config.partition)
            .env("GZ_IP", config.interface.to_string())
            .env("GZ_DISCOVERY_MULTICAST_IP", config.multicast.to_string())
            .env("GZ_DISCOVERY_MSG_PORT", config.message_port.to_string())
            .env("GZ_DISCOVERY_SRV_PORT", config.service_port.to_string())
            .env(
                "GZ_TRANSPORT_SNDHWM",
                config.send_high_watermark.to_string(),
            )
            .env(
                "GZ_TRANSPORT_RCVHWM",
                config.receive_high_watermark.to_string(),
            )
            .env("GZ_TRANSPORT_IMPLEMENTATION", "zeromq")
            .env("GZ_TRANSPORT_TOPIC_STATISTICS", "0")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("Build the required real C++ peer with scripts/build-cpp-peer.sh");
        let stdout = child.stdout.take().unwrap();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let reader = thread::spawn(move || {
            let mut stdout = BufReader::new(stdout);
            let mut line = String::new();
            let result = stdout.read_line(&mut line).map(|_| line);
            let _ = ready_tx.send(result);
            stdout
        });
        let mut peer = Self {
            child,
            ready_reader: Some(reader),
            _stdout: None,
        };
        let ready = ready_rx
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        assert_eq!(ready.trim(), "READY");
        peer._stdout = Some(peer.ready_reader.take().unwrap().join().unwrap());
        peer
    }
    fn assert_alive(&mut self) {
        assert!(self.child.try_wait().unwrap().is_none());
    }
    fn finish(&mut self) {
        self.child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"QUIT\n")
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                return;
            }
            assert!(Instant::now() < deadline, "native provider did not stop");
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for NativePeer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.ready_reader.take() {
            let _ = reader.join();
        }
    }
}

#[derive(Clone, Copy)]
enum WrongType {
    Request,
    Response,
    InputlessResponse,
}
#[test]
fn original_two_process_service_wrong_request_type() {
    observe_wrong_type(WrongType::Request);
}
#[test]
fn original_two_process_service_wrong_response_type() {
    observe_wrong_type(WrongType::Response);
}
#[test]
fn original_two_process_inputless_service_wrong_response_type() {
    observe_wrong_type(WrongType::InputlessResponse);
}
fn observe_wrong_type(case: WrongType) {
    let (request_type, response_type, payload) = match case {
        WrongType::Request => (
            "gz.msgs.Vector3d",
            "gz.msgs.Int32",
            Vector3d {
                x: 1.,
                y: 2.,
                z: 3.,
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        WrongType::Response => (
            "gz.msgs.Int32",
            "gz.msgs.Vector3d",
            Int32 {
                data: 5,
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        WrongType::InputlessResponse => ("gz.msgs.Empty", "gz.msgs.Vector3d", Vec::new()),
    };
    let config = Config {
        partition: format!("remote-wrong-types-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    };
    let inputless = matches!(case, WrongType::InputlessResponse);
    let mut peer = NativePeer::start(&config, inputless);
    let node = Node::new(config).unwrap();
    // Supplemental discovery readiness proves an actual remote typed provider.
    assert_eq!(
        node.resolve_service_types("/foo", "", "", Duration::from_secs(10)),
        Ok(ServiceTypeResolution::Resolved(ServiceTypes {
            request_type: if inputless {
                "gz.msgs.Empty"
            } else {
                "gz.msgs.Int32"
            }
            .into(),
            response_type: "gz.msgs.Int32".into(),
        }))
    );
    peer.assert_alive();
    let executed = AtomicBool::new(false);
    let stopping = AtomicBool::new(false);
    thread::scope(|scope| {
        let (ready_tx, ready_rx) = mpsc::sync_channel(0);
        let callback_executed = &executed;
        let observer_stopping = &stopping;
        let requester = &node;
        let request_payload = &payload;
        let observer = scope.spawn(move || {
            let pending = requester
                .begin_request(
                    "/foo",
                    request_type,
                    response_type,
                    request_payload.clone(),
                    Duration::from_secs(10),
                )
                .unwrap();
            ready_tx.send(()).unwrap();
            match pending.wait(Duration::from_secs(10)) {
                Ok((bytes, result)) => {
                    // Native wrongResponse ignores both arguments; any reply
                    // trips its flag. The ordinary response retains data/result.
                    if matches!(case, WrongType::Request) {
                        assert_eq!(Int32::decode(bytes.as_slice()).unwrap().data, 5);
                        assert!(result);
                    }
                    callback_executed.store(true, Ordering::Release);
                }
                Err(Error::Shutdown) if observer_stopping.load(Ordering::Acquire) => {}
                Err(other) => panic!("unexpected response observer error: {other:?}"),
            }
        });
        ready_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        thread::sleep(Duration::from_millis(300));
        assert!(!executed.load(Ordering::Acquire));
        assert_eq!(
            node.request(
                "/foo",
                request_type,
                response_type,
                payload.clone(),
                Duration::from_millis(1000)
            ),
            Err(Error::Timeout)
        );
        peer.assert_alive();
        stopping.store(true, Ordering::Release);
        node.cancel();
        observer.join().unwrap();
        // Supplemental: late queued responses must not disappear at cleanup.
        assert!(!executed.load(Ordering::Acquire));
    });
    peer.finish();
}

#[test]
fn original_two_process_service_mixed_response_types() {
    observe_mixed_response_types(false);
}

#[test]
fn original_two_process_inputless_service_mixed_response_types() {
    observe_mixed_response_types(true);
}

// Retain the first failed assertion while retiring scoped observers on unwind.
struct CancelObserversOnDrop<'a> {
    node: &'a Node,
    stopping: &'a AtomicBool,
}
impl Drop for CancelObserversOnDrop<'_> {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        self.node.cancel();
    }
}

fn observe_mixed_response_types(inputless: bool) {
    let request_type = if inputless {
        "gz.msgs.Empty"
    } else {
        "gz.msgs.Int32"
    };
    let payload = if inputless {
        Vec::new()
    } else {
        Int32 {
            data: 5,
            ..Default::default()
        }
        .encode_to_vec()
    };
    let config = Config {
        partition: format!("remote-mixed-response-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    };
    let mut peer = NativePeer::start(&config, inputless);
    // Both originals settle for 500 ms before constructing the requester Node.
    thread::sleep(Duration::from_millis(500));
    let node = Node::new(config).unwrap();
    assert_eq!(
        node.resolve_service_types("/foo", "", "", Duration::from_secs(10)),
        Ok(ServiceTypeResolution::Resolved(ServiceTypes {
            request_type: request_type.into(),
            response_type: "gz.msgs.Int32".into(),
        }))
    );
    peer.assert_alive();
    let wrong_executed = AtomicBool::new(false);
    let good_executed = AtomicBool::new(false);
    let stopping = AtomicBool::new(false);
    thread::scope(|scope| {
        let _cancel_on_exit = CancelObserversOnDrop {
            node: &node,
            stopping: &stopping,
        };
        // The remote originals use 2000 ms, before admitting the wrong async.
        assert_eq!(
            node.request(
                "/foo",
                request_type,
                "gz.msgs.Vector3d",
                payload.clone(),
                Duration::from_millis(2000),
            ),
            Err(Error::Timeout)
        );
        let (wrong_ready_tx, wrong_ready_rx) = mpsc::sync_channel(0);
        let requester = &node;
        let request_payload = &payload;
        let wrong_flag = &wrong_executed;
        let observer_stopping = &stopping;
        let wrong_observer = scope.spawn(move || {
            let pending = requester
                .begin_request(
                    "/foo",
                    request_type,
                    "gz.msgs.Vector3d",
                    request_payload.clone(),
                    Duration::from_secs(10),
                )
                .unwrap();
            wrong_ready_tx.send(()).unwrap();
            match pending.wait(Duration::from_secs(10)) {
                Ok(_) => wrong_flag.store(true, Ordering::Release),
                Err(Error::Shutdown) if observer_stopping.load(Ordering::Acquire) => {}
                Err(other) => panic!("unexpected wrong observer error: {other:?}"),
            }
        });
        wrong_ready_rx
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        thread::sleep(Duration::from_millis(300));
        assert!(!wrong_executed.load(Ordering::Acquire));

        // Native requires synchronous success; payload/result checks supplement it.
        let (good_bytes, good_result) = node
            .request(
                "/foo",
                request_type,
                "gz.msgs.Int32",
                payload.clone(),
                Duration::from_millis(2000),
            )
            .unwrap();
        assert_eq!(Int32::decode(good_bytes.as_slice()).unwrap().data, 5);
        assert!(good_result);
        let (good_ready_tx, good_ready_rx) = mpsc::sync_channel(0);
        let good_flag = &good_executed;
        let good_observer = scope.spawn(move || {
            let pending = requester
                .begin_request(
                    "/foo",
                    request_type,
                    "gz.msgs.Int32",
                    request_payload.clone(),
                    Duration::from_secs(10),
                )
                .unwrap();
            good_ready_tx.send(()).unwrap();
            match pending.wait(Duration::from_secs(10)) {
                Ok((bytes, result)) => {
                    assert_eq!(Int32::decode(bytes.as_slice()).unwrap().data, 5);
                    assert!(result);
                    good_flag.store(true, Ordering::Release);
                }
                Err(Error::Shutdown) if observer_stopping.load(Ordering::Acquire) => {}
                Err(other) => panic!("unexpected good observer error: {other:?}"),
            }
        });
        good_ready_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        thread::sleep(Duration::from_millis(300));
        assert!(good_executed.load(Ordering::Acquire));
        peer.assert_alive();
        // Keep the wrong observer live through the later original good check.
        stopping.store(true, Ordering::Release);
        node.cancel();
        wrong_observer.join().unwrap();
        good_observer.join().unwrap();
        assert!(!wrong_executed.load(Ordering::Acquire));
    });
    peer.finish();
}
