// Copyright (C) 2014, 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Retained NodeTest.SigIntTermination / SigTermTermination and their complete
// createInfinitePublisher helper from pinned src/Node_TEST.cc 2149–2224.
#![cfg(unix)]

use gz_msgs::msgs::Int32;
use gz_transport::{Config, Node};
use signal_hook::{
    consts::{SIGINT, SIGTERM},
    flag, low_level,
};
use socket2::{Domain, Protocol, Socket, Type};
use std::{
    net::{Ipv4Addr, SocketAddrV4},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

fn config() -> (Config, [Socket; 2]) {
    let reserve = || {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).unwrap();
        socket.set_reuse_address(true).unwrap();
        socket
            .bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into())
            .unwrap();
        #[cfg(target_os = "linux")]
        socket.set_multicast_all_v4(false).unwrap();
        socket
    };
    // Hold both reusable reservations until the child worker has retired.
    let reservations = [reserve(), reserve()];
    let ports = reservations.each_ref().map(|socket| {
        socket
            .local_addr()
            .unwrap()
            .as_socket_ipv4()
            .unwrap()
            .port()
    });
    assert_ne!(ports[0], ports[1]);
    (
        Config {
            partition: uuid::Uuid::new_v4().to_string(),
            message_port: ports[0],
            service_port: ports[1],
            ..Default::default()
        },
        reservations,
    )
}

#[test]
fn signal_publisher_child() {
    let signal = match std::env::var("GZ_SIGNAL_ORIGINAL").as_deref() {
        Ok("SIGINT") => SIGINT,
        Ok("SIGTERM") => SIGTERM,
        _ => return,
    };
    // The application owns this action. Transport installs no implicit handler.
    // A child isolates signal-hook's process-wide registration from other tests.
    let terminate = Arc::new(AtomicBool::new(false));
    let action = flag::register(signal, terminate.clone()).unwrap();
    let publisher_thread = thread::spawn(move || {
        terminate.store(false, Ordering::SeqCst);
        let (settings, _reservations) = config();
        let message = Int32 {
            data: 5,
            ..Default::default()
        };
        let node = Node::new(settings).unwrap();
        let publisher = node.advertise_typed::<Int32>("/foo").unwrap();
        let mut count = 0;
        loop {
            publisher.publish_typed(&message).unwrap();
            count += 1;
            thread::sleep(Duration::from_millis(10));
            if terminate.load(Ordering::SeqCst) {
                break;
            }
        }
        assert!(count < 200, "publisher required {count} iterations");
        terminate.store(false, Ordering::SeqCst);
        count
    });
    thread::sleep(Duration::from_millis(500));
    low_level::raise(signal).unwrap();
    let count = publisher_thread.join().unwrap();
    assert!(low_level::unregister(action));
    println!("SIGNAL_PUBLISHER_JOINED {signal} {count}");
}

fn original(signal: &str) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "signal_publisher_child", "--nocapture"])
        .env("GZ_SIGNAL_ORIGINAL", signal)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Only the harness has a watchdog; the original publishing loop and its
    // <200 assertion have no substitute iteration limit or readiness wait.
    let deadline = Instant::now() + Duration::from_secs(6);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!("{signal} child did not join: {output:?}");
        }
        thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{signal}: {output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("SIGNAL_PUBLISHER_JOINED"), "{stdout}");
    assert!(
        stdout.contains("test signal_publisher_child ... ok"),
        "{stdout}"
    );
}

#[test]
fn sigint_termination_original() {
    original("SIGINT");
}

#[test]
fn sigterm_termination_original() {
    original("SIGTERM");
}
