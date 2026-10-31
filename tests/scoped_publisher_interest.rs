// Copyright (C) 2014, 2025, 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Pinned twoProcsPubSub.PubSubTwoProcsScopedPub and unchanged subscriber auxiliary.
use gz_msgs::msgs::Vector3d;
use gz_transport::Node;
use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[path = "support/network_clock_config.rs"]
mod ports;

struct Subscriber(Option<Child>);
impl Subscriber {
    fn start(partition: &str, message_port: u16, service_port: u16) -> Self {
        let source = PathBuf::from(
            std::env::var_os("GZ_REFERENCE_SOURCE")
                .expect("select the pinned native source with GZ_REFERENCE_SOURCE"),
        );
        let child = Command::new(source.join("build/bin/twoProcsPubSubSingleSubscriber_aux"))
            .arg(partition)
            .env("GZ_IP", "127.0.0.1")
            .env("GZ_DISCOVERY_MSG_PORT", message_port.to_string())
            .env("GZ_DISCOVERY_SRV_PORT", service_port.to_string())
            .env("GZ_TRANSPORT_TOPIC_STATISTICS", "0")
            .env("GZ_TRANSPORT_IMPLEMENTATION", "zeromq")
            .env_remove("GZ_RELAY")
            .env("GTEST_COLOR", "no")
            .env(
                "GTEST_FILTER",
                "twoProcPubSub.PubSubTwoProcsTwoNodesSingleSubscriber",
            )
            .env("GTEST_REPEAT", "1")
            .env_remove("GTEST_LIST_TESTS")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        Self(Some(child))
    }
    fn join(mut self) {
        // The unchanged child already has its original 100 x 100 ms receive
        // bound. This watchdog only prevents a broken child from escaping.
        let deadline = Instant::now() + Duration::from_secs(12);
        while self.0.as_mut().unwrap().try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline, "native subscriber did not exit");
            thread::sleep(Duration::from_millis(10));
        }
        let output = self.0.take().unwrap().wait_with_output().unwrap();
        assert!(output.status.success(), "{output:?}");
        let stdout = std::str::from_utf8(&output.stdout).unwrap();
        assert!(stdout.contains("[  PASSED  ] 1 test."), "{stdout}");
    }
}
impl Drop for Subscriber {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[test]
fn scoped_publisher_recreation_original_uses_actual_native_subscriber() {
    let (settings, _reservations) = ports::config();
    let node = Node::new(settings.clone()).unwrap();
    for _ in 0..2 {
        let subscriber = Subscriber::start(
            &settings.partition,
            settings.message_port,
            settings.service_port,
        );
        {
            let publisher = node.advertise_typed::<Vector3d>("/foo").unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            while !publisher.connection_snapshot().unwrap() {
                assert!(
                    Instant::now() < deadline,
                    "no subscriber within native bound"
                );
                thread::sleep(Duration::from_millis(10));
            }
            let value = Vector3d {
                x: 1.0,
                y: 2.0,
                z: 3.0,
                ..Default::default()
            };
            publisher.publish_typed(&value).unwrap();
        }
        // No flush, acknowledgement or extra wait before publisher retirement.
        thread::sleep(Duration::from_millis(100));
        subscriber.join();
    }
}
