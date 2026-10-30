// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use gz_msgs::{msgs::Clock, prost::Message};
use gz_transport::{ClockTimeBase, Config, NetworkClock, Node};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
#[path = "support/network_clock_config.rs"]
mod fixture;

#[derive(Debug, PartialEq)]
struct State {
    ready: bool,
    time: i64,
    messages: u64,
    wire: Vec<u8>,
}

fn unhex(value: &str) -> Vec<u8> {
    if value == "-" {
        return Vec::new();
    }
    assert_eq!(value.len() % 2, 0);
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).unwrap())
        .collect()
}

struct Peer {
    child: Option<Child>,
    input: Option<ChildStdin>,
    lines: Option<Receiver<String>>,
    reader: Option<JoinHandle<()>>,
}
impl Peer {
    fn new(config: &Config, base: &str) -> Self {
        let mut command = Command::new(format!(
            "{}/artifacts/network-clock-peer",
            env!("CARGO_MANIFEST_DIR")
        ));
        for (key, _) in
            std::env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("GZ_"))
        {
            command.env_remove(key);
        }
        let mut child = command
            .args([base, "/clock"])
            .env("GZ_IP", config.interface.to_string())
            .env("GZ_PARTITION", &config.partition)
            .env("GZ_DISCOVERY_MSG_PORT", config.message_port.to_string())
            .env("GZ_DISCOVERY_SRV_PORT", config.service_port.to_string())
            .env("GZ_TRANSPORT_IMPLEMENTATION", "zeromq")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (tx, lines) = mpsc::sync_channel(8);
        let reader = thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                if tx.send(line.unwrap()).is_err() {
                    break;
                }
            }
        });
        let peer = Self {
            child: Some(child),
            input: Some(input),
            lines: Some(lines),
            reader: Some(reader),
        };
        assert_eq!(peer.line(), "READY");
        peer
    }
    fn line(&self) -> String {
        self.lines
            .as_ref()
            .unwrap()
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
    }
    fn command(&mut self, command: &str) -> State {
        writeln!(self.input.as_mut().unwrap(), "{command}").unwrap();
        self.input.as_mut().unwrap().flush().unwrap();
        let line = self.line();
        let fields: Vec<_> = line.split_whitespace().collect();
        assert_eq!(fields.len(), 5, "{line}");
        assert_eq!(fields[0], "STATE");
        assert!(matches!(fields[1], "0" | "1"));
        State {
            ready: fields[1] == "1",
            time: fields[2].parse().unwrap(),
            messages: fields[3].parse().unwrap(),
            wire: unhex(fields[4]),
        }
    }
    fn finish(mut self) -> String {
        writeln!(self.input.as_mut().unwrap(), "QUIT").unwrap();
        drop(self.input.take());
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.child.as_mut().unwrap().try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline, "native clock teardown deadline");
            thread::sleep(Duration::from_millis(2));
        }
        let output = self.child.take().unwrap().wait_with_output().unwrap();
        assert!(output.status.success(), "{output:?}");
        drop(self.lines.take());
        self.reader.take().unwrap().join().unwrap();
        String::from_utf8(output.stderr).unwrap()
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        // Disconnect the bounded reader before joining, including failed tests.
        drop(self.lines.take());
        drop(self.input.take());
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
fn native_rebuilt_clock_replays_all_63_value_and_wire_states() {
    let data: gz_msgs::serde_json::Value =
        gz_msgs::serde_json::from_str(include_str!("fixtures/network-clock.json")).unwrap();
    let hash = Command::new("sha256sum")
        .arg(format!(
            "{}/tests/network_clock_peer.cc",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(hash.status.success());
    assert_eq!(
        String::from_utf8(hash.stdout)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap(),
        data["source_sha256"].as_str().unwrap()
    );
    let mut records = 0;
    for case in data["cases"].as_array().unwrap() {
        let (config, _reservations) = fixture::config();
        let mut peer = Peer::new(&config, case["base"].as_str().unwrap());
        for record in case["records"].as_array().unwrap() {
            let actual = peer.command(record["command"].as_str().unwrap());
            assert_eq!(
                actual,
                State {
                    ready: record["ready"].as_bool().unwrap(),
                    time: record["time_ns"].as_i64().unwrap(),
                    messages: record["observed_messages"].as_u64().unwrap(),
                    wire: unhex(record["wire_hex"].as_str().unwrap())
                }
            );
            records += 1;
        }
        assert_eq!(peer.finish(), case["stderr"].as_str().unwrap());
    }
    assert_eq!(records, 63);
}

fn wait_state(peer: &mut Peer, mut accepts: impl FnMut(&State) -> bool) -> State {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let state = peer.command("GET");
        if accepts(&state) {
            return state;
        }
        assert!(
            Instant::now() < deadline,
            "native clock receipt deadline: {state:?}"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn native_and_rust_clocks_exchange_actual_selected_wire_in_both_directions() {
    let values = [
        0,
        1,
        -1,
        1_000_000_001,
        -1_000_000_001,
        54_321_000_012_345,
        54_321_000_024_690,
        2_147_483_647_999_999_999,
        2_147_483_648_000_000_000,
        -2_147_483_648_000_000_000,
        -2_147_483_649_000_000_000,
        i64::MAX,
        i64::MIN,
    ];
    for (name, base) in [
        ("sim", ClockTimeBase::Simulation),
        ("real", ClockTimeBase::Real),
        ("system", ClockTimeBase::System),
    ] {
        let (config, _reservations) = fixture::config();
        let node = Node::new(config.clone()).unwrap();
        let clock = NetworkClock::new(&node, "/clock", base).unwrap();
        let mut peer = Peer::new(&config, name);
        // Supplemental cross-process setup proves real delivery; none of these
        // retries changes the separately retained original's three 100 ms waits.
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            clock.set_time_ns(987_654_321).unwrap();
            thread::sleep(Duration::from_millis(20));
            if peer.command("GET").time == 987_654_321 {
                break;
            }
            assert!(Instant::now() < deadline, "Rust-to-native startup");
        }
        // FIFO marker clears earlier warm messages before reference SET calls.
        let before = peer.command("GET").messages;
        clock.set_time_ns(123_456_789).unwrap();
        wait_state(&mut peer, |s| s.time == 123_456_789 && s.messages > before);
        loop {
            let _ = peer.command("SET 564738291");
            if clock.time_ns() == 564_738_291 {
                break;
            }
            assert!(Instant::now() < deadline, "native-to-Rust startup");
        }
        // Start opaque observation after warm FIFO traffic has been consumed.
        let receiver = node.subscribe("/clock", "gz.msgs.Clock").unwrap();
        for value in values {
            while receiver.recv_timeout(Duration::ZERO).is_ok() {}
            let expected = peer.command(&format!("SET {value}"));
            let wire = receiver
                .recv_with_info_timeout(Duration::from_secs(1))
                .unwrap();
            assert!(!wire.info().intra_process);
            assert_eq!(wire.data, expected.wire);
            let progress = Instant::now() + Duration::from_secs(1);
            while clock.time_ns() != expected.time {
                assert!(Instant::now() < progress);
                thread::sleep(Duration::from_millis(2));
            }
            assert_eq!(clock.is_ready(), expected.ready);
            clock.set_time_ns(value).unwrap();
            // A fresh native callback is required: matching an old value alone
            // would let a no-op Rust setter pass this round-trip check.
            let observed = wait_state(&mut peer, |s| s.messages > expected.messages);
            assert_eq!(observed.time, expected.time);
            assert_eq!(observed.ready, expected.ready);
            assert_eq!(observed.wire, expected.wire);
            assert_eq!(
                Clock::decode(observed.wire.as_slice()).unwrap().header,
                None
            );
        }
        assert_eq!(peer.finish(), "");
    }
}
