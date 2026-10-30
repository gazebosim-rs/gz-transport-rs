// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Native source characterization uses a controlled interval, never new defaults.
use gz_msgs::discovery::discovery::Type;
#[path = "support/service_scope_capture.rs"]
mod capture;
use capture::{check_record, collect, listener, receive};
use std::{
    io::{BufRead, BufReader, Write},
    net::{Ipv4Addr, UdpSocket},
    process::{Child, ChildStdout, Command, Stdio},
    time::{Duration, Instant},
};
struct Native(Child, BufReader<ChildStdout>);
impl Native {
    fn line(&mut self) -> String {
        let mut line = String::new();
        assert_ne!(self.1.read_line(&mut line).unwrap(), 0);
        line.trim_end().into()
    }
    fn command(&mut self, command: &str, expected: &str) {
        writeln!(self.0.stdin.as_mut().unwrap(), "{command}").unwrap();
        assert_eq!(self.line(), expected);
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn unused_ports() -> [u16; 2] {
    let sockets = [
        UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap(),
        UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap(),
    ];
    let ports = sockets.each_ref().map(|s| s.local_addr().unwrap().port());
    drop(sockets);
    ports
}
#[test]
fn native_service_scope_packets_and_actual_local_data_are_characterized() {
    for (name, scope) in [("all", 2), ("host", 1), ("process", 0)] {
        let socket = listener();
        let control = socket.local_addr().unwrap().port();
        let [message, data] = unused_ports();
        assert!(control != message && control != data && message != data);
        let path = std::env::var("GZ_SCOPE_CPP_PEER").unwrap_or_else(|_| {
            format!(
                "{}/artifacts/service-scope-peer",
                env!("CARGO_MANIFEST_DIR")
            )
        });
        let mut child = Command::new(path)
            .args([name, "/scope", &control.to_string()])
            .env(
                "GZ_PARTITION",
                format!("scope-control-{}", uuid::Uuid::new_v4()),
            )
            .env("GZ_IP", "127.0.0.1")
            .env("GZ_DISCOVERY_MSG_PORT", message.to_string())
            .env("GZ_DISCOVERY_SRV_PORT", data.to_string())
            .env("GZ_TRANSPORT_IMPLEMENTATION", "zeromq")
            .env("GZ_TRANSPORT_TOPIC_STATISTICS", "0")
            .env("GZ_VERBOSE", "0")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut native = Native(child, BufReader::new(stdout));
        let fields: Vec<_> = (0..8).map(|_| native.line()).collect();
        assert_eq!(fields[4], scope.to_string());
        assert_eq!(fields[6], "gz.msgs.Int32");
        assert_eq!(fields[7], "gz.msgs.Int32");
        assert_eq!(native.line(), "READY");
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(msg) = receive(&socket)
                && msg.process_uuid == fields[2]
                && msg.r#type == Type::Heartbeat as i32
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "no initial empty native heartbeat"
            );
        }
        native.command("ADVERTISE", "ADVERTISED");
        let initial = collect(
            &socket,
            &fields[2],
            Type::Advertise,
            Duration::from_millis(150),
        );
        assert_eq!(initial.len(), usize::from(scope != 0));
        for msg in &initial {
            check_record(msg, &fields);
        }
        native.command("REQUEST", "REQUESTED");
        let requested = collect(
            &socket,
            &fields[2],
            Type::Advertise,
            Duration::from_millis(150),
        );
        assert_eq!(requested.len(), usize::from(scope != 0));
        for msg in &requested {
            check_record(msg, &fields);
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        let heartbeat_record = loop {
            if let Some(msg) = receive(&socket)
                && msg.process_uuid == fields[2]
                && msg.r#type == Type::Advertise as i32
            {
                break msg;
            }
            assert!(
                Instant::now() < deadline,
                "no scoped native heartbeat advertisement"
            );
        };
        check_record(&heartbeat_record, &fields);
        native.command("CALL", "CALLED 1 1 5 1");
        native.command("UNADVERTISE", "UNADVERTISED");
        let removed = collect(
            &socket,
            &fields[2],
            Type::Unadvertise,
            Duration::from_millis(150),
        );
        assert_eq!(removed.len(), usize::from(scope != 0));
        for msg in &removed {
            check_record(msg, &fields);
        }
        writeln!(native.0.stdin.as_mut().unwrap(), "QUIT").unwrap();
        assert!(native.0.wait().unwrap().success());
    }
}
