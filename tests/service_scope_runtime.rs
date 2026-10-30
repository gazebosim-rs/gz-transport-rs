// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use gz_msgs::{int32::Int32, prost::Message};
use gz_transport::{Config, Node, Scope, ServiceOptions};
use std::{
    net::{Ipv4Addr, UdpSocket},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
#[path = "support/service_scope_capture.rs"]
mod capture;
const INT: &str = "gz.msgs.Int32";
fn config() -> Config {
    let sockets = [
        UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap(),
        UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap(),
    ];
    let [message_port, service_port] = sockets.each_ref().map(|s| s.local_addr().unwrap().port());
    assert_ne!(message_port, service_port);
    drop(sockets);
    Config {
        partition: format!("service-scopes-{}", uuid::Uuid::new_v4()),
        message_port,
        service_port,
        ..Default::default()
    }
}
fn options(scope: Scope) -> ServiceOptions {
    let mut options = ServiceOptions::default();
    options.set_scope(scope);
    options
}
fn five() -> Vec<u8> {
    Int32 {
        data: 5,
        ..Default::default()
    }
    .encode_to_vec()
}
#[test]
fn service_scope_is_immutable_and_all_three_local_apis_retain_data() {
    for scope in [Scope::All, Scope::Host, Scope::Process] {
        let cfg = config();
        let owner = Node::new(cfg.clone()).unwrap();
        let caller = Node::new(cfg).unwrap();
        let mut opts = options(scope);
        let queue = owner.serve_with_options("/queued", INT, INT, opts).unwrap();
        opts.set_scope(if scope == Scope::All {
            Scope::Process
        } else {
            Scope::All
        });
        let info = owner.known_service_info("/queued").unwrap();
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].scope, scope);
        let pending = caller
            .begin_request("/queued", INT, INT, five(), Duration::from_secs(2))
            .unwrap();
        let request = queue.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(request.data, five());
        request.respond(five(), true).unwrap();
        assert_eq!(pending.wait(Duration::from_secs(1)), Ok((five(), true)));
        let response_calls = Arc::new(AtomicUsize::new(0));
        let entries = response_calls.clone();
        let response = owner
            .serve_callback_with_options("/callback", INT, INT, options(scope), move |bytes| {
                entries.fetch_add(1, Ordering::AcqRel);
                assert_eq!(bytes, five());
                Ok((five(), true))
            })
            .unwrap();
        let completed = Arc::new(AtomicUsize::new(0));
        let entries = completed.clone();
        let request = caller
            .request_callback(
                "/callback",
                INT,
                INT,
                five(),
                Duration::from_secs(1),
                move |bytes, result| {
                    entries.fetch_add(1, Ordering::AcqRel);
                    assert_eq!(bytes, five());
                    assert!(result);
                },
            )
            .unwrap();
        assert_eq!(completed.load(Ordering::Acquire), 1);
        request.wait(Duration::from_secs(1)).unwrap();
        assert_eq!(response_calls.load(Ordering::Acquire), 1);
        assert_eq!(
            owner.known_service_info("/callback").unwrap()[0].scope,
            scope
        );
        let oneway_calls = Arc::new(AtomicUsize::new(0));
        let entries = oneway_calls.clone();
        let oneway = owner
            .serve_oneway_callback_with_options("/oneway", INT, options(scope), move |bytes| {
                entries.fetch_add(1, Ordering::AcqRel);
                assert_eq!(bytes, five());
                Ok(())
            })
            .unwrap();
        caller
            .request_oneway("/oneway", INT, five(), Duration::from_secs(1))
            .unwrap();
        assert_eq!(oneway_calls.load(Ordering::Acquire), 1);
        assert_eq!(owner.known_service_info("/oneway").unwrap()[0].scope, scope);
        drop(oneway);
        drop(response);
        drop(queue);
        let recreated = owner
            .serve_with_options("/queued", INT, INT, options(Scope::Host))
            .unwrap();
        assert_eq!(
            owner.known_service_info("/queued").unwrap()[0].scope,
            Scope::Host
        );
        drop(recreated);
    }
}
#[test]
fn native_requests_see_all_and_host_but_not_process_services() {
    let path = std::env::var("GZ_CPP_PEER")
        .unwrap_or_else(|_| format!("{}/artifacts/cpp-peer", env!("CARGO_MANIFEST_DIR")));
    for (scope, name) in [
        (Scope::All, "all"),
        (Scope::Host, "host"),
        (Scope::Process, "process"),
    ] {
        let cfg = config();
        let owner = Node::new(cfg.clone()).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let entries = calls.clone();
        let _service = owner
            .serve_callback_with_options("/external", INT, INT, options(scope), move |bytes| {
                entries.fetch_add(1, Ordering::AcqRel);
                assert_eq!(bytes, five());
                Ok((five(), true))
            })
            .unwrap();
        let output = Command::new(&path)
            .args(["request-service-scope", "/external", name])
            .env("GZ_PARTITION", &cfg.partition)
            .env("GZ_IP", cfg.interface.to_string())
            .env("GZ_DISCOVERY_MULTICAST_IP", cfg.multicast.to_string())
            .env("GZ_DISCOVERY_MSG_PORT", cfg.message_port.to_string())
            .env("GZ_DISCOVERY_SRV_PORT", cfg.service_port.to_string())
            .env("GZ_TRANSPORT_IMPLEMENTATION", "zeromq")
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        if scope == Scope::Process {
            assert_eq!(output.stdout, b"visible=0 returned=0 result=0 data=0\n");
            assert_eq!(calls.load(Ordering::Acquire), 0);
        } else {
            assert_eq!(output.stdout, b"visible=1 returned=1 result=1 data=5\n");
            assert_eq!(calls.load(Ordering::Acquire), 1);
        }
    }
}

#[test]
fn scoped_runtime_heartbeats_retain_process_wire_records_and_filter_views() {
    use gz_msgs::discovery::discovery::Type;
    for scope in [Scope::All, Scope::Host, Scope::Process] {
        let socket = capture::listener();
        let mut cfg = config();
        cfg.service_port = socket.local_addr().unwrap().port();
        let owner = Node::new(cfg.clone()).unwrap();
        let observer = Node::new(cfg).unwrap();
        let _service = owner
            .serve_with_options("/heartbeat", INT, INT, options(scope))
            .unwrap();
        let info = owner.known_service_info("/heartbeat").unwrap();
        assert_eq!(info.len(), 1);
        let record = &info[0];
        let expected = vec![
            record.topic.clone(),
            record.address.clone(),
            record.process_uuid.clone(),
            record.node_uuid.clone(),
            (record.scope as i32).to_string(),
            record.socket_id.clone(),
            record.request_type.clone(),
            record.response_type.clone(),
        ];
        let packets = capture::collect(
            &socket,
            &record.process_uuid,
            Type::Advertise,
            Duration::from_millis(1250),
        );
        assert!(
            !packets.is_empty(),
            "heartbeat must preserve every scoped record, including Process"
        );
        for msg in &packets {
            capture::check_record(msg, &expected);
        }
        let actual = observer.known_service_info("/heartbeat").unwrap();
        if scope == Scope::Process {
            assert!(actual.is_empty());
        } else {
            assert_eq!(actual, info);
        }
    }
}
