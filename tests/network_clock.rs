// Copyright (C) 2018, 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Complete Clock_TEST.cc Functionality fixtures; supplements remain separate.
use gz_msgs::msgs::{Clock, Time};
use gz_transport::{ClockTimeBase, Error, NetworkClock, Node};
use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

#[path = "support/network_clock_config.rs"]
mod fixture;
use fixture::config;

fn stamp(sec: i64, nsec: i32) -> Option<Time> {
    Some(Time { sec, nsec })
}

fn original(base: ClockTimeBase) {
    let (settings, _reservations) = config();
    // This explicit Node supplies the resource owned privately by native Clock.
    let owner = Node::new(settings.clone()).unwrap();
    let clock = NetworkClock::new(&owner, "/clock", base).unwrap();
    assert!(!clock.is_ready());
    let node = Node::new(settings).unwrap();
    let publisher = node.advertise_typed::<Clock>("/clock").unwrap();
    let sleep_time = Duration::from_millis(100);
    publisher.publish_typed(&Clock::default()).unwrap();
    thread::sleep(sleep_time);
    assert!(!clock.is_ready());
    let message = match base {
        ClockTimeBase::Simulation => Clock {
            sim: stamp(54321, 12345),
            real: stamp(54321 * 2, 0),
            system: stamp(54321 * 4, 0),
            ..Default::default()
        },
        ClockTimeBase::Real => Clock {
            sim: stamp(54321 / 2, 0),
            real: stamp(54321, 12345),
            system: stamp(54321 * 2, 0),
            ..Default::default()
        },
        ClockTimeBase::System => Clock {
            sim: stamp(54321 / 4, 0),
            real: stamp(54321 / 2, 0),
            system: stamp(54321, 12345),
            ..Default::default()
        },
    };
    publisher.publish_typed(&message).unwrap();
    thread::sleep(sleep_time);
    assert!(clock.is_ready());
    assert_eq!(clock.time_ns(), 54321 * 1_000_000_000 + 12345);
    clock
        .set_time_ns(54321 * 1_000_000_000 + 12345 * 2)
        .unwrap();
    thread::sleep(sleep_time);
    assert_eq!(clock.time_ns(), 54321 * 1_000_000_000 + 12345 * 2);
}

#[test]
fn network_clock_sim_functionality_original() {
    original(ClockTimeBase::Simulation);
}
#[test]
fn network_clock_real_functionality_original() {
    original(ClockTimeBase::Real);
}
#[test]
fn network_clock_system_functionality_original() {
    original(ClockTimeBase::System);
}

fn wait(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "supplemental clock progress deadline"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

struct ReleaseOnDrop(Arc<(Mutex<bool>, Condvar)>);
impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        *self.0.0.lock().unwrap_or_else(|error| error.into_inner()) = true;
        self.0.1.notify_all();
    }
}

#[test]
fn clock_reset_backward_missing_and_rejected_messages_preserve_owned_state() {
    let (settings, _reservations) = config();
    let owner = Node::new(settings.clone()).unwrap();
    let clock = NetworkClock::new(&owner, "/clock", ClockTimeBase::Simulation).unwrap();
    let node = Node::new(settings).unwrap();
    let publisher = node.advertise_typed::<Clock>("/clock").unwrap();
    for value in [123, -321, 0, -1] {
        clock.set_time_ns(value).unwrap();
        wait(|| clock.time_ns() == value);
        assert_eq!(clock.is_ready(), value != 0);
    }
    publisher
        .publish_typed(&Clock {
            real: stamp(99, 7),
            ..Default::default()
        })
        .unwrap();
    publisher.publish_typed(&Clock::default()).unwrap();
    publisher
        .publish_typed(&Clock {
            sim: stamp(i64::MAX, 0),
            ..Default::default()
        })
        .unwrap();
    publisher.publish(vec![0xff]).unwrap();
    // Per-registration FIFO means both missing-field records precede these
    // two visible errors; neither changes the last coherent sample.
    wait(|| owner.statistics().routing_errors == 2);
    assert_eq!(clock.time_ns(), -1);
    assert!(clock.is_ready());
    publisher
        .publish_typed(&Clock {
            sim: stamp(0, i32::MIN),
            ..Default::default()
        })
        .unwrap();
    wait(|| clock.time_ns() == i64::from(i32::MIN));
    assert_eq!(owner.statistics().routing_errors, 2);
    owner.cancel();
    assert_eq!(clock.set_time_ns(999), Err(Error::Shutdown));
    assert_eq!(clock.time_ns(), i64::from(i32::MIN));
}

#[test]
fn constructor_failures_and_drop_release_original_node_admission() {
    let (mut settings, _reservations) = config();
    settings.capacity = 1;
    let node = Node::new(settings).unwrap();
    // The callback admits first; publication exhausts the same bounded slots.
    assert!(matches!(
        NetworkClock::new(&node, "/clock", ClockTimeBase::Simulation),
        Err(Error::QueueFull)
    ));
    assert!(node.subscribed_topics().unwrap().is_empty());
    assert!(node.advertised_topics().unwrap().is_empty());
    let callback = node
        .subscribe_typed_callback::<Clock, _>("/other", |_, _| Ok(()))
        .unwrap();
    drop(callback);
    let publisher = node.advertise_typed::<Clock>("/clock").unwrap();
    drop(publisher);

    let (settings, _other_reservations) = config();
    let node = Node::new(settings).unwrap();
    assert!(matches!(
        NetworkClock::new(&node, "//bad-clock", ClockTimeBase::Simulation),
        Err(Error::InvalidName)
    ));
    let publisher = node.advertise_typed::<Clock>("/clock").unwrap();
    assert!(matches!(
        NetworkClock::new(&node, "/clock", ClockTimeBase::Simulation),
        Err(Error::AlreadyAdvertised)
    ));
    assert!(node.subscribed_topics().unwrap().is_empty());
    assert_eq!(node.advertised_topics().unwrap(), ["/clock"]);
    drop(publisher);
    let clock = NetworkClock::new(&node, "/clock", ClockTimeBase::Simulation).unwrap();
    assert_eq!(node.subscribed_topics().unwrap(), ["/clock"]);
    assert_eq!(node.advertised_topics().unwrap(), ["/clock"]);
    drop(clock);
    assert!(node.subscribed_topics().unwrap().is_empty());
    assert!(node.advertised_topics().unwrap().is_empty());
    let clock = NetworkClock::new(&node, "/clock", ClockTimeBase::System).unwrap();
    drop(clock);
    node.cancel();
    assert!(matches!(
        NetworkClock::new(&node, "/clock", ClockTimeBase::Real),
        Err(Error::Shutdown)
    ));
}

#[test]
fn clock_retirement_uses_shared_dispatch_and_preserves_other_callbacks() {
    let (mut settings, _reservations) = config();
    // Four registrations can queue four bootstrap commands. Leave exactly one
    // command slot for the first publication without assuming worker progress.
    // This fixture tests shared teardown; capacity-one rollback is separate.
    settings.capacity = 5;
    let node = Node::new(settings).unwrap();
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let gate = release.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let invoked = calls.clone();
    let (entered, blocked) = mpsc::sync_channel(1);
    let other = node
        .subscribe_typed_callback::<gz_msgs::msgs::Int32, _>("/other", move |message, _| {
            invoked.fetch_add(1, Ordering::SeqCst);
            if message.data == 1 {
                entered.send(()).unwrap();
                let (lock, ready) = &*gate;
                let mut open = lock.lock().unwrap();
                while !*open {
                    open = ready.wait(open).unwrap();
                }
            }
            Ok(())
        })
        .unwrap();
    let publisher = node
        .advertise_typed::<gz_msgs::msgs::Int32>("/other")
        .unwrap();
    let clock = NetworkClock::new(&node, "/clock", ClockTimeBase::Simulation).unwrap();
    // Release selected code before handle cleanup even if an assertion fails.
    let _release_on_failure = ReleaseOnDrop(release.clone());
    publisher
        .publish_typed(&gz_msgs::msgs::Int32 {
            data: 1,
            ..Default::default()
        })
        .unwrap();
    blocked.recv_timeout(Duration::from_secs(1)).unwrap();
    let (done, retired) = mpsc::sync_channel(1);
    let retire = thread::spawn(move || {
        drop(clock);
        done.send(()).unwrap();
    });
    // Actual lease retirement proves Drop has started, not merely its thread.
    wait(|| node.subscribed_topics().unwrap() == ["/other"]);
    assert_eq!(
        retired.recv_timeout(Duration::from_millis(20)),
        Err(mpsc::RecvTimeoutError::Timeout)
    );
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    retired.recv_timeout(Duration::from_secs(1)).unwrap();
    retire.join().unwrap();
    assert!(other.is_active());
    assert_eq!(node.advertised_topics().unwrap(), ["/other"]);
    publisher
        .publish_typed(&gz_msgs::msgs::Int32 {
            data: 2,
            ..Default::default()
        })
        .unwrap();
    wait(|| calls.load(Ordering::SeqCst) == 2);
    assert_eq!(node.statistics().routing_errors, 0);
    let clock = NetworkClock::new(&node, "/clock", ClockTimeBase::System).unwrap();
    clock.set_time_ns(77).unwrap();
    wait(|| clock.time_ns() == 77);
}

#[test]
fn payload_admission_and_owner_retirement_do_not_silently_update_clock() {
    let (mut settings, _reservations) = config();
    settings.max_message_bytes = 1;
    let node = Node::new(settings).unwrap();
    let clock = NetworkClock::new(&node, "/clock", ClockTimeBase::Real).unwrap();
    // Even selected zero has a present, two-byte nested message on the wire.
    assert_eq!(clock.set_time_ns(0), Err(Error::InvalidFrame));
    assert_eq!(clock.time_ns(), 0);
    assert!(!clock.is_ready());
    assert_eq!(node.statistics().routing_errors, 0);
    drop(clock);
    drop(node);

    let (settings, _owner_reservations) = config();
    let owner = Node::new(settings).unwrap();
    let clock = NetworkClock::new(&owner, "/clock", ClockTimeBase::System).unwrap();
    clock.set_time_ns(18).unwrap();
    wait(|| clock.time_ns() == 18);
    owner.unsubscribe("/clock").unwrap();
    assert_eq!(clock.set_time_ns(19), Err(Error::Shutdown));
    assert_eq!(clock.time_ns(), 18);
    drop(clock);
    let clock = NetworkClock::new(&owner, "/clock", ClockTimeBase::System).unwrap();
    clock.set_time_ns(21).unwrap();
    wait(|| clock.time_ns() == 21);
    drop(owner);
    assert_eq!(clock.set_time_ns(22), Err(Error::Shutdown));
    assert_eq!(clock.time_ns(), 21);
    assert!(clock.is_ready()); // stored time, not a liveness predicate
}
