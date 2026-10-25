// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use gz_transport::{Config, Error, Node};
use std::{
    net::UdpSocket,
    thread,
    time::{Duration, Instant},
};

fn port() -> u16 {
    UdpSocket::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[test]
fn dense_publish_and_fresh_client_service_request() {
    let config = Config {
        partition: format!("probe-{}", uuid::Uuid::new_v4()),
        message_port: port(),
        service_port: port(),
        ..Config::default()
    };
    let server = Node::new(config.clone()).unwrap();
    let service = server
        .serve(
            "/world/test/control",
            "gz.msgs.WorldControl",
            "gz.msgs.Boolean",
        )
        .unwrap();
    let clock = server
        .advertise("/world/test/clock", "gz.msgs.Clock")
        .unwrap();
    let pose = server
        .advertise("/world/test/pose/info", "gz.msgs.Pose_V")
        .unwrap();
    let observer = Node::new(config.clone()).unwrap();
    let sub = observer
        .subscribe("/world/test/clock", "gz.msgs.Clock")
        .unwrap();
    let ready = Instant::now() + Duration::from_secs(5);
    while !clock.has_connections().unwrap() {
        assert!(Instant::now() < ready);
        thread::sleep(Duration::from_millis(10));
    }
    clock.publish(vec![0; 80]).unwrap();
    sub.recv_timeout(Duration::from_secs(5)).unwrap();
    let started = Instant::now();
    let result = thread::spawn(move || {
        let client = Node::new(config).unwrap();
        client.request(
            "/world/test/control",
            "gz.msgs.WorldControl",
            "gz.msgs.Boolean",
            vec![0x80],
            Duration::from_secs(5),
        )
    });
    let mut publishes = 0usize;
    let mut failures = 0usize;
    let mut received = 0usize;
    let mut response = None;
    let deadline = started + Duration::from_secs(6);
    while !result.is_finished() && Instant::now() < deadline {
        for publication in [&clock, &pose] {
            if publication.publish(vec![0; 80]).is_err() {
                failures += 1;
            }
            publishes += 1;
        }
        match service.recv_timeout(Duration::ZERO) {
            Ok(request) => {
                received += 1;
                response = Some(request.respond(vec![1], true));
            }
            Err(Error::Timeout) => {}
            Err(error) => panic!("service receive: {error:?}"),
        }
        thread::yield_now();
    }
    let outcome = result.join().unwrap();
    println!(
        "elapsed={:?} publishes={publishes} failures={failures} received={received} response={response:?} server_stats={:?} result={outcome:?}",
        started.elapsed(),
        server.statistics()
    );
    assert!(
        failures > 0,
        "publication queue did not reach the overload boundary"
    );
    assert_eq!(received, 1);
    assert_eq!(response, Some(Ok(())));
    assert_eq!(outcome, Ok((vec![1], true)));
}
