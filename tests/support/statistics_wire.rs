// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn statistics_metadata_preserves_milliseconds_sequence_wrap_and_clock_errors() {
    let sequence = AtomicU64::new(0);
    let bytes = statistics_metadata(&sequence, Ok(Duration::from_nanos(123_999_999))).unwrap();
    assert_eq!(bytes[..8], 123_u64.to_ne_bytes());
    assert_eq!(bytes[8..], 0_u64.to_ne_bytes());
    assert_eq!(sequence.load(Ordering::Relaxed), 1);
    let error = Error::Native("clock failure".into());
    assert_eq!(
        statistics_metadata(&sequence, Err(error.clone())),
        Err(error)
    );
    assert_eq!(
        statistics_metadata(&sequence, Ok(Duration::MAX)),
        Err(Error::InvalidFrame)
    );
    assert_eq!(sequence.load(Ordering::Relaxed), 1);
    sequence.store(u64::MAX, Ordering::Relaxed);
    let bytes = statistics_metadata(&sequence, Ok(Duration::from_millis(u64::MAX))).unwrap();
    assert_eq!(bytes[..8], u64::MAX.to_ne_bytes());
    assert_eq!(bytes[8..], u64::MAX.to_ne_bytes());
    assert_eq!(sequence.load(Ordering::Relaxed), 0);
    assert_eq!(
        statistics_metadata(&sequence, Ok(Duration::ZERO)).unwrap(),
        [0; 16]
    );
}

#[test]
fn statistics_mode_separates_both_local_transport_domains() {
    let ordinary = Config::default();
    let statistics = Config {
        statistics_wire: true,
        ..ordinary.clone()
    };
    assert!(MessageDomain::from_config(&ordinary) != MessageDomain::from_config(&statistics));
    assert!(
        MessageDomain::from_service_config(&ordinary)
            != MessageDomain::from_service_config(&statistics)
    );
    assert!(
        MessageDomain::from_config(&statistics) == MessageDomain::from_config(&statistics.clone())
    );
    assert!(
        MessageDomain::from_service_config(&statistics)
            == MessageDomain::from_service_config(&statistics.clone())
    );
}

// Admission-only fixture: no sockets, IO worker or synthetic wire observations.
fn admission_node(capacity: usize) -> (Node, Receiver<Command>) {
    let config = Config {
        capacity,
        statistics_wire: true,
        partition: Uuid::new_v4().to_string(),
        ..Default::default()
    };
    let (tx, rx) = mpsc::sync_channel(capacity);
    let (response_tx, _) = mpsc::sync_channel(capacity.min(64));
    let services = Arc::new(Mutex::new(Vec::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let identity = Arc::new(OnceLock::new());
    assert!(
        identity
            .set(WorkerIdentity {
                process: Uuid::new_v4().to_string(),
                node: Uuid::new_v4().to_string(),
                address: "tcp://127.0.0.1:1".into(),
                response_address: "tcp://127.0.0.1:2".into(),
                response_id: "response".into(),
                service_domain: MessageDomain::from_service_config(&config),
                services: services.clone(),
                stopped: Arc::downgrade(&stop),
            })
            .is_ok()
    );
    let sender = Sender {
        tx,
        response_tx,
        stop: stop.clone(),
        slots: Arc::new(AtomicUsize::new(0)),
        callback_threads: Arc::new(AtomicUsize::new(0)),
        callback_requests: Arc::new(AtomicUsize::new(0)),
        capacity,
        counters: Arc::new(Counters::default()),
        identity,
        publication_sequences: Some(Arc::new(Mutex::new(HashMap::new()))),
    };
    (
        Node {
            sender,
            lifetime: Arc::new(WorkerLifetime {
                stop,
                worker: Mutex::new(None),
            }),
            owner_dead: Arc::new(AtomicBool::new(false)),
            local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
            config,
            topics: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(Vec::new()),
            services,
            subscription_dispatcher: Mutex::new(Weak::new()),
        },
        rx,
    )
}
#[test]
fn statistics_history_is_retained_bounded_and_orphan_owned() {
    let (node, commands) = admission_node(1);
    let publisher = node.advertise("/first", "type").unwrap();
    assert!(matches!(commands.try_recv(), Ok(Command::Advertise(..))));
    let counter = publisher.publication.sequence.as_ref().unwrap().clone();
    statistics_metadata(&counter, Ok(Duration::ZERO)).unwrap();
    assert!(matches!(
        node.advertise("/first", "other"),
        Err(Error::AlreadyAdvertised)
    ));
    drop(publisher);
    assert!(node.advertised_topics().unwrap().is_empty());
    assert!(matches!(
        node.advertise("/second", "type"),
        Err(Error::QueueFull)
    ));
    assert_eq!(node.statistics().queue_drops, 1);
    let replacement = node.advertise("/first", "other").unwrap();
    assert!(matches!(commands.try_recv(), Ok(Command::Advertise(..))));
    assert!(Arc::ptr_eq(
        &counter,
        replacement.publication.sequence.as_ref().unwrap()
    ));
    assert_eq!(counter.load(Ordering::Relaxed), 1);
    let history = Arc::downgrade(node.sender.publication_sequences.as_ref().unwrap());
    drop(node);
    assert!(history.upgrade().is_some());
    replacement.publish([5]).unwrap();
    assert!(matches!(commands.try_recv(), Ok(Command::Publish(..))));
    assert_eq!(counter.load(Ordering::Relaxed), 1);
    drop(replacement);
    assert!(history.upgrade().is_none());
}
#[test]
fn statistics_history_rolls_back_queue_and_late_registration_failures() {
    let (node, commands) = admission_node(1);
    node.sender
        .send(Command::Publish(
            "blocked".into(),
            "type".into(),
            vec![],
            None,
        ))
        .unwrap();
    assert!(matches!(
        node.advertise("/failed", "type"),
        Err(Error::QueueFull)
    ));
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert!(
        node.sender
            .publication_sequences
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .is_empty()
    );
    assert!(commands.try_recv().is_ok());
    let publisher = node.advertise("/succeeds", "type").unwrap();
    assert!(commands.try_recv().is_ok());
    assert_eq!(
        node.sender
            .publication_sequences
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .len(),
        1
    );
    drop(publisher);
    let (node, commands) = admission_node(1);
    let cache = node.local_interest.clone();
    assert!(
        thread::spawn(move || {
            let _guard = cache.lock().unwrap();
            panic!("exercise late registration rollback");
        })
        .join()
        .is_err()
    );
    assert!(
        matches!(node.advertise("/failed","type"), Err(Error::Native(message)) if message=="local interest cache poisoned")
    );
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert!(
        node.sender
            .publication_sequences
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .is_empty()
    );
    assert!(
        matches!(commands.try_recv(), Ok(Command::Advertise(_,_,_,active,_)) if !active.load(Ordering::Acquire))
    );
}
#[test]
fn statistics_history_poison_and_shutdown_cannot_admit_more_topics() {
    let (node, _commands) = admission_node(1);
    let history = node.sender.publication_sequences.as_ref().unwrap().clone();
    assert!(
        thread::spawn(move || {
            let _guard = history.lock().unwrap();
            panic!("exercise sequence registry poison");
        })
        .join()
        .is_err()
    );
    assert!(
        matches!(node.advertise("/failed","type"), Err(Error::Native(message)) if message=="statistics sequence registry poisoned")
    );
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    node.cancel();
    assert!(matches!(
        node.advertise("/stopped", "type"),
        Err(Error::Shutdown)
    ));
    let (node, _commands) = admission_node(1);
    node.cancel();
    assert!(matches!(
        node.advertise("/stopped", "type"),
        Err(Error::Shutdown)
    ));
    assert!(
        node.sender
            .publication_sequences
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .is_empty()
    );
}

fn inproc_frames() -> (zmq::Context, zmq::Socket, zmq::Socket) {
    let context = zmq::Context::new();
    let sender = context.socket(zmq::PAIR).unwrap();
    let receiver = context.socket(zmq::PAIR).unwrap();
    sender.set_linger(0).unwrap();
    receiver.set_linger(0).unwrap();
    let address = format!("inproc://statistics-frames-{}", Uuid::new_v4());
    sender.bind(&address).unwrap();
    receiver.connect(&address).unwrap();
    (context, sender, receiver)
}
fn wire_frames(metadata: Vec<u8>) -> Vec<Vec<u8>> {
    vec![
        b"topic".to_vec(),
        b"address".to_vec(),
        vec![0, 255, 1, 0],
        b"type".to_vec(),
        metadata,
    ]
}
#[test]
fn statistics_inproc_rejects_each_bad_metadata_size_and_drains_before_valid_data() {
    let (_context, sender, receiver) = inproc_frames();
    let counters = Counters::default();
    let good = wire_frames(vec![255; 16]);
    for (index, size) in [0, 1, 15, 17, 1024].into_iter().enumerate() {
        sender
            .send_multipart(wire_frames(vec![0; size]), 0)
            .unwrap();
        sender.send_multipart(&good, 0).unwrap();
        assert!(receive_publication_frames(&receiver, true, &counters, || false).is_none());
        assert_eq!(counters.snapshot().malformed_frames, (index + 1) as u64);
        assert_eq!(
            receive_publication_frames(&receiver, true, &counters, || false),
            Some(good.clone())
        );
        assert_eq!(counters.snapshot().malformed_frames, (index + 1) as u64);
    }
}
#[test]
fn statistics_inproc_wrong_counts_and_profiles_do_not_consume_following_messages() {
    let (_context, sender, receiver) = inproc_frames();
    let counters = Counters::default();
    let good = wire_frames(vec![0; 16]);
    let mut malformed = 0;
    for count in [1, 3, 4, 6, 130] {
        let bad = (0..count)
            .map(|index| {
                if index == 4 {
                    vec![0; 16]
                } else {
                    vec![index as u8]
                }
            })
            .collect::<Vec<_>>();
        sender.send_multipart(bad, 0).unwrap();
        sender.send_multipart(&good, 0).unwrap();
        assert!(receive_publication_frames(&receiver, true, &counters, || false).is_none());
        malformed += 1;
        assert_eq!(counters.snapshot().malformed_frames, malformed);
        assert_eq!(
            receive_publication_frames(&receiver, true, &counters, || false),
            Some(good.clone())
        );
    }
    sender.send_multipart(&good, 0).unwrap();
    sender.send_multipart(&good[..4], 0).unwrap();
    assert!(receive_publication_frames(&receiver, false, &counters, || false).is_none());
    malformed += 1;
    assert_eq!(counters.snapshot().malformed_frames, malformed);
    assert_eq!(
        receive_publication_frames(&receiver, false, &counters, || false),
        Some(good[..4].to_vec())
    );
}
#[test]
fn statistics_inproc_cancellation_is_bounded_and_does_not_invent_malformed_input() {
    let (_context, sender, receiver) = inproc_frames();
    let counters = Counters::default();
    let good = wire_frames(vec![0; 16]);
    sender.send_multipart(&good, 0).unwrap();
    assert!(receive_publication_frames(&receiver, true, &counters, || true).is_none());
    assert_eq!(
        receive_publication_frames(&receiver, true, &counters, || false),
        Some(good)
    );
    sender.send_multipart(vec![vec![0; 16]; 130], 0).unwrap();
    let mut checks = 0;
    assert!(
        receive_publication_frames(&receiver, true, &counters, || {
            checks += 1;
            checks == 4
        })
        .is_none()
    );
    assert_eq!(checks, 4);
    assert_eq!(counters.snapshot().malformed_frames, 0);
    // As in the existing cancellation contract, the owner retires this socket;
    // an intentionally abandoned multipart tail is not reused for reception.
}

#[test]
fn statistics_history_counter_retires_with_its_wire_owner_not_echo_tombstones() {
    let (node, commands) = admission_node(1);
    let publisher = node.advertise("/retired", "type").unwrap();
    assert!(commands.try_recv().is_ok());
    let counter = Arc::downgrade(publisher.publication.sequence.as_ref().unwrap());
    drop(publisher);
    drop(node);
    assert!(
        counter.upgrade().is_none(),
        "echo tombstones cannot extend sequence-owner lifetime"
    );
}
