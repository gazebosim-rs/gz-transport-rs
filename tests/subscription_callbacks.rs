// Copyright (C) 2014, 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Complete typed Lambda/LambdaMessageInfo setups from pinned Node_TEST.cc.
use gz_msgs::msgs::{Int32, Vector3d};
use gz_transport::{Config, Error, Node, SubscribeOptions};
use std::{
    collections::HashSet,
    net::UdpSocket,
    sync::{Arc, Condvar, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

fn config() -> Config {
    let sockets = [
        UdpSocket::bind(("127.0.0.1", 0)).unwrap(),
        UdpSocket::bind(("127.0.0.1", 0)).unwrap(),
    ];
    let ports = sockets
        .each_ref()
        .map(|socket| socket.local_addr().unwrap().port());
    assert_ne!(ports[0], ports[1]);
    drop(sockets);
    Config {
        partition: format!("callback-{}", uuid::Uuid::new_v4()),
        message_port: ports[0],
        service_port: ports[1],
        ..Default::default()
    }
}
fn original(with_info: bool) {
    let settings = config();
    let partition = format!("/{}", settings.partition);
    let value = Int32 {
        data: 5,
        ..Default::default()
    };
    let node = Node::new(settings).unwrap();
    let publisher = node.advertise_typed::<Int32>("/foo").unwrap();
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let callback_gate = gate.clone();
    let caller = thread::current().id();
    let _callback = node
        .subscribe_typed_callback::<Int32, _>("/foo", move |message, info| {
            if with_info {
                assert_eq!(info.topic, "/foo");
            }
            assert_eq!(message.data, 5);
            if with_info {
                assert_eq!(info.partition, partition);
                assert_eq!(info.type_name, "gz.msgs.Int32");
                assert!(info.intra_process);
            }
            assert_ne!(thread::current().id(), caller);
            *callback_gate.0.lock().unwrap() = true;
            callback_gate.1.notify_all();
            Ok(())
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !publisher.has_connections().unwrap() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    // Preserve native's different lock placement: metadata case locks before
    // Publish; ordinary lambda publishes before taking its condition lock.
    let executed = if with_info {
        let executed = gate.0.lock().unwrap();
        publisher.publish_typed(&value).unwrap();
        executed
    } else {
        publisher.publish_typed(&value).unwrap();
        gate.0.lock().unwrap()
    };
    let (executed, _) = gate
        .1
        .wait_timeout_while(executed, Duration::from_secs(1), |executed| !*executed)
        .unwrap();
    assert!(*executed);
}
#[test]
fn original_pub_sub_same_thread_lambda_callback() {
    original(false);
}
#[test]
fn original_pub_sub_same_thread_lambda_message_info_callback() {
    original(true);
}

#[test]
fn callbacks_share_one_demand_dispatcher_and_recreate_after_last_drop() {
    let node = Node::new(config()).unwrap();
    let publisher = node.advertise_typed::<Int32>("/foo").unwrap();
    let (done, observed) = mpsc::channel();
    let mut handles = Vec::new();
    for index in 0..32 {
        let done = done.clone();
        handles.push(
            node.subscribe_typed_callback::<Int32, _>("/foo", move |message, _| {
                assert_eq!(message.data, 5);
                done.send((thread::current().id(), index)).unwrap();
                Ok(())
            })
            .unwrap(),
        );
    }
    publisher
        .publish_typed(&Int32 {
            data: 5,
            ..Default::default()
        })
        .unwrap();
    let observations = (0..32)
        .map(|_| observed.recv_timeout(Duration::from_secs(1)).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        observations
            .iter()
            .map(|(_, index)| *index)
            .collect::<HashSet<_>>(),
        (0..32).collect()
    );
    let ids = observations
        .iter()
        .map(|(id, _)| *id)
        .collect::<HashSet<_>>();
    assert_eq!(ids.len(), 1);
    assert!(!ids.contains(&thread::current().id()));
    drop(handles);
    assert!(node.subscribed_topics().unwrap().is_empty());
    assert!(!publisher.has_connections().unwrap());
    let handle = node
        .subscribe_typed_callback::<Int32, _>("/foo", move |_, _| {
            done.send((thread::current().id(), 32)).unwrap();
            Ok(())
        })
        .unwrap();
    publisher
        .publish_typed(&Int32 {
            data: 5,
            ..Default::default()
        })
        .unwrap();
    let (recreated, index) = observed.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(index, 32);
    assert!(!ids.contains(&recreated));
    drop(handle);
}

#[test]
fn malformed_and_wrong_type_never_enter_user_callback_then_valid_data_progresses() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let node = Node::new(config()).unwrap();
    let entered = Arc::new(AtomicUsize::new(0));
    let input = node.advertise_typed::<Int32>("/typed").unwrap();
    let wrong = node.advertise_typed::<Vector3d>("/wrong").unwrap();
    let calls = entered.clone();
    let _wrong_callback = node
        .subscribe_typed_callback::<Int32, _>("/wrong", move |_, _| {
            calls.fetch_add(1, Ordering::AcqRel);
            panic!("wrong type entered user code")
        })
        .unwrap();
    let calls = entered.clone();
    let (done, observed) = mpsc::channel();
    let _callback = node
        .subscribe_typed_callback::<Int32, _>("/typed", move |value, _| {
            calls.fetch_add(1, Ordering::AcqRel);
            assert_eq!(value.data, 5);
            done.send(()).unwrap();
            Ok(())
        })
        .unwrap();
    input.publish(vec![0xff]).unwrap();
    wrong
        .publish_typed(&Vector3d {
            x: 1.0,
            y: 2.0,
            z: 3.0,
            ..Default::default()
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    while node.statistics().routing_errors == 0 {
        assert!(
            Instant::now() < deadline,
            "malformed payload was not rejected"
        );
        thread::sleep(Duration::from_millis(1));
    }
    thread::sleep(Duration::from_millis(300));
    assert_eq!(entered.load(Ordering::Acquire), 0);
    input
        .publish_typed(&Int32 {
            data: 5,
            ..Default::default()
        })
        .unwrap();
    observed.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(entered.load(Ordering::Acquire), 1);
    assert_eq!(node.statistics().routing_errors, 1);
}

#[test]
fn option_filtering_self_drop_and_node_unsubscribe_retire_callbacks() {
    let node = Arc::new(Node::new(config()).unwrap());
    let publisher = node.advertise_typed::<Int32>("/foo").unwrap();
    let mut options = SubscribeOptions::default();
    options.set_ignore_local_messages(true);
    let (done, observed) = mpsc::channel();
    let callback_done = done.clone();
    let ignored = node
        .subscribe_typed_callback_with_options::<Int32, _>("/foo", options, move |_, _| {
            callback_done.send(()).unwrap();
            Ok(())
        })
        .unwrap();
    publisher
        .publish_typed(&Int32 {
            data: 5,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        observed.recv_timeout(Duration::from_millis(300)),
        Err(mpsc::RecvTimeoutError::Timeout)
    );
    drop(ignored);
    let holder = Arc::new(Mutex::new(None::<gz_transport::CallbackSubscription>));
    let callback_holder = holder.clone();
    let handle = node
        .subscribe_typed_callback::<Int32, _>("/foo", move |value, _| {
            assert_eq!(value.data, 5);
            let handle = callback_holder.lock().unwrap().take();
            drop(handle);
            done.send(()).unwrap();
            Ok(())
        })
        .unwrap();
    *holder.lock().unwrap() = Some(handle);
    publisher
        .publish_typed(&Int32 {
            data: 5,
            ..Default::default()
        })
        .unwrap();
    observed.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(holder.lock().unwrap().is_none());
    assert!(node.subscribed_topics().unwrap().is_empty());
    let (done, observed) = mpsc::channel();
    let handle = node
        .subscribe_typed_callback::<Int32, _>("/foo", move |_, _| {
            done.send(()).unwrap();
            Ok(())
        })
        .unwrap();
    node.unsubscribe("/foo").unwrap();
    assert_eq!(
        observed.recv_timeout(Duration::from_secs(1)),
        Err(mpsc::RecvTimeoutError::Disconnected)
    );
    drop(handle);
    publisher
        .publish_typed(&Int32 {
            data: 5,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        observed.recv_timeout(Duration::from_millis(20)),
        Err(mpsc::RecvTimeoutError::Disconnected)
    );
}

#[test]
fn explicit_shutdown_and_ordinary_node_drop_retire_callback_captures() {
    struct End(mpsc::Sender<()>);
    impl Drop for End {
        fn drop(&mut self) {
            self.0.send(()).unwrap();
        }
    }
    for explicit in [false, true] {
        let mut node = Node::new(config()).unwrap();
        let (done, observed) = mpsc::channel();
        let end = End(done);
        let handle = node
            .subscribe_typed_callback::<Int32, _>("/foo", move |_, _| {
                std::hint::black_box(&end);
                Ok(())
            })
            .unwrap();
        if explicit {
            node.shutdown();
        }
        drop(node);
        observed.recv_timeout(Duration::from_secs(1)).unwrap();
        drop(handle);
    }
    let mut node = Node::new(config()).unwrap();
    node.shutdown();
    assert!(matches!(
        node.subscribe_typed_callback::<Int32, _>("/foo", |_, _| Ok(())),
        Err(Error::Shutdown)
    ));
    assert!(matches!(
        node.subscribe_typed_callback::<Int32, _>("invalid topic", |_, _| Ok(())),
        Err(Error::InvalidName)
    ));
}

struct ReleaseGate(Arc<(Mutex<bool>, Condvar)>);
impl Drop for ReleaseGate {
    fn drop(&mut self) {
        *self.0.0.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.0.1.notify_all();
    }
}
#[test]
fn blocked_callback_keeps_io_and_other_node_dispatchers_progressing() {
    let settings = config();
    let node = Node::new(settings.clone()).unwrap();
    let publisher = node.advertise_typed::<Int32>("/foo").unwrap();
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let callback_gate = gate.clone();
    let (entered, observed) = mpsc::channel();
    let callback = node
        .subscribe_typed_callback::<Int32, _>("/foo", move |value, _| {
            assert_eq!(value.data, 5);
            entered.send(thread::current().id()).unwrap();
            let mut released = callback_gate.0.lock().unwrap();
            while !*released {
                released = callback_gate.1.wait(released).unwrap();
            }
            Ok(())
        })
        .unwrap();
    let release = ReleaseGate(gate);
    publisher
        .publish_typed(&Int32 {
            data: 5,
            ..Default::default()
        })
        .unwrap();
    let blocked_thread = observed.recv_timeout(Duration::from_secs(1)).unwrap();
    // This command requires a real reply from the IO worker, unlike the
    // immediate local registry views. User code holds no IO/registry guard.
    assert!(node.known_services().unwrap().is_empty());
    let other = Node::new(settings).unwrap();
    let publication = other.advertise_typed::<Int32>("/other").unwrap();
    let (done, completed) = mpsc::channel();
    let _other_callback = other
        .subscribe_typed_callback::<Int32, _>("/other", move |_, _| {
            done.send(thread::current().id()).unwrap();
            Ok(())
        })
        .unwrap();
    publication
        .publish_typed(&Int32 {
            data: 5,
            ..Default::default()
        })
        .unwrap();
    let other_thread = completed.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_ne!(other_thread, blocked_thread);
    drop(release);
    drop(callback);
}

#[test]
fn actual_cpp_publication_enters_typed_callback_with_owned_metadata() {
    use gz_msgs::msgs::StringMsg;
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
    };
    struct Peer(Child);
    impl Drop for Peer {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let settings = config();
    let partition = format!("/{}", settings.partition);
    let node = Node::new(settings.clone()).unwrap();
    let (done, observed) = mpsc::channel();
    let callback = node
        .subscribe_typed_callback::<StringMsg, _>("/cpp", move |message, info| {
            assert_eq!(message.data, "native-callback");
            assert_eq!(info.topic, "/cpp");
            assert_eq!(info.partition, partition);
            assert_eq!(info.type_name, "gz.msgs.StringMsg");
            assert!(!info.intra_process);
            done.send(()).unwrap();
            Ok(())
        })
        .unwrap();
    let path = std::env::var("GZ_CPP_PEER")
        .unwrap_or_else(|_| format!("{}/artifacts/cpp-peer", env!("CARGO_MANIFEST_DIR")));
    let mut peer = Peer(
        Command::new(path)
            .args(["publish", "/cpp", "native-callback"])
            .env("GZ_PARTITION", &settings.partition)
            .env("GZ_IP", settings.interface.to_string())
            .env("GZ_DISCOVERY_MULTICAST_IP", settings.multicast.to_string())
            .env("GZ_DISCOVERY_MSG_PORT", settings.message_port.to_string())
            .env("GZ_DISCOVERY_SRV_PORT", settings.service_port.to_string())
            .env("GZ_TRANSPORT_IMPLEMENTATION", "zeromq")
            .env("GZ_TRANSPORT_TOPIC_STATISTICS", "0")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let stdout = peer.0.stdout.take().unwrap();
    let (ready, wait) = mpsc::channel();
    let reading = thread::spawn(move || {
        let mut output = BufReader::new(stdout);
        let mut line = String::new();
        let read = output.read_line(&mut line);
        let _ = ready.send((read, line));
        output
    });
    let status = wait.recv_timeout(Duration::from_secs(5));
    if status.is_err() {
        let _ = peer.0.kill();
        let _ = peer.0.wait();
    }
    let _output = reading.join().unwrap();
    let (read, line) = status.unwrap();
    read.unwrap();
    assert_eq!(line.trim(), "READY");
    observed.recv_timeout(Duration::from_secs(5)).unwrap();
    drop(callback);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = peer.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline, "native publisher did not exit");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn blocked_callback_preserves_fifo_and_bounded_queue_drop_accounting() {
    let mut settings = config();
    settings.capacity = 4;
    let node = Node::new(settings).unwrap();
    let publisher = node.advertise_typed::<Int32>("/fifo").unwrap();
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let callback_gate = gate.clone();
    let (entered, waiting) = mpsc::channel();
    let (done, observed) = mpsc::channel();
    let callback = node
        .subscribe_typed_callback::<Int32, _>("/fifo", move |value, _| {
            if value.data == 0 {
                entered.send(()).unwrap();
                let mut release = callback_gate.0.lock().unwrap();
                while !*release {
                    release = callback_gate.1.wait(release).unwrap();
                }
            }
            done.send(value.data).unwrap();
            Ok(())
        })
        .unwrap();
    let release = ReleaseGate(gate);
    publisher.publish_typed(&Int32::default()).unwrap();
    waiting.recv_timeout(Duration::from_secs(1)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut command_full = 0;
    for data in 1..=6 {
        loop {
            match publisher.publish_typed(&Int32 {
                data,
                ..Default::default()
            }) {
                Ok(()) => break,
                Err(Error::QueueFull) => command_full += 1,
                Err(error) => panic!("unexpected publication error: {error}"),
            }
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
    }
    // Drain every admitted Publish through the real IO worker while user code
    // is blocked. Expected admission rejections are counted, not retried away.
    loop {
        match node.known_services() {
            Ok(services) => {
                assert!(services.is_empty());
                break;
            }
            Err(Error::QueueFull) => command_full += 1,
            Err(error) => panic!("unexpected barrier error: {error}"),
        }
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
    assert_eq!(node.statistics().queue_drops, command_full + 2);
    drop(release);
    for expected in 0..=4 {
        assert_eq!(
            observed.recv_timeout(Duration::from_secs(1)).unwrap(),
            expected
        );
    }
    assert_eq!(
        observed.recv_timeout(Duration::from_millis(20)),
        Err(mpsc::RecvTimeoutError::Timeout)
    );
    drop(callback);
}

#[test]
fn cancel_during_invocation_finishes_selected_work_and_discards_queued_work() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let node = Node::new(config()).unwrap();
    let publisher = node.advertise_typed::<Int32>("/cancel").unwrap();
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let callback_gate = gate.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_calls = calls.clone();
    let (entered, waiting) = mpsc::channel();
    let (done, completed) = mpsc::channel();
    let callback = node
        .subscribe_typed_callback::<Int32, _>("/cancel", move |value, _| {
            callback_calls.fetch_add(1, Ordering::AcqRel);
            assert_eq!(value.data, 5);
            entered.send(()).unwrap();
            let mut release = callback_gate.0.lock().unwrap();
            while !*release {
                release = callback_gate.1.wait(release).unwrap();
            }
            done.send(()).unwrap();
            Ok(())
        })
        .unwrap();
    let release = ReleaseGate(gate);
    publisher
        .publish_typed(&Int32 {
            data: 5,
            ..Default::default()
        })
        .unwrap();
    waiting.recv_timeout(Duration::from_secs(1)).unwrap();
    publisher
        .publish_typed(&Int32 {
            data: 6,
            ..Default::default()
        })
        .unwrap();
    assert!(node.known_services().unwrap().is_empty());
    callback.cancel();
    assert!(node.subscribed_topics().unwrap().is_empty());
    assert_eq!(
        completed.recv_timeout(Duration::from_millis(20)),
        Err(mpsc::RecvTimeoutError::Timeout)
    );
    drop(release);
    completed.recv_timeout(Duration::from_secs(1)).unwrap();
    drop(callback);
    assert_eq!(calls.load(Ordering::Acquire), 1);
}

#[test]
fn native_typed_shared_dispatch_and_raw_inline_boundaries_remain_explicit() {
    use std::process::{Command, Stdio};
    let settings = config();
    let mut child = Command::new(format!(
        "{}/artifacts/subscription-callback-peer",
        env!("CARGO_MANIFEST_DIR")
    ))
    .env("GZ_PARTITION", &settings.partition)
    .env("GZ_IP", settings.interface.to_string())
    .env("GZ_DISCOVERY_MULTICAST_IP", settings.multicast.to_string())
    .env("GZ_DISCOVERY_MSG_PORT", settings.message_port.to_string())
    .env("GZ_DISCOVERY_SRV_PORT", settings.service_port.to_string())
    .env("GZ_TRANSPORT_IMPLEMENTATION", "zeromq")
    .env("GZ_TRANSPORT_TOPIC_STATISTICS", "0")
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("native callback boundary probe timed out");
        }
        thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).unwrap();
    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 3);
    assert!(
        lines[0]
            .starts_with("many callbacks=32 callback_threads=1 caller_thread=0 threads_before=")
    );
    assert_eq!(
        lines[1],
        "retire queued_removed=1 blocked_other_node=1 other_after_release=1"
    );
    assert_eq!(
        lines[2],
        "self_unsubscribe=1 raw_inline=1 raw_callback_before_return=1"
    );
}
