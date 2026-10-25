// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Ported Node_TEST.cc assertions: Copyright (C) 2014 Open Source Robotics Foundation.
use gz_msgs::{int32::Int32, prost::Message};
use gz_transport::{Config, Error, Node};
use std::{
    thread,
    time::{Duration, Instant},
};

const INT32: &str = "gz.msgs.Int32";
const EMPTY: &str = "gz.msgs.Empty";
fn node() -> Node {
    Node::new(Config {
        partition: format!("service-port-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    })
    .unwrap()
}
fn five() -> Vec<u8> {
    Int32 {
        data: 5,
        ..Default::default()
    }
    .encode_to_vec()
}

fn sync_service(with_input: bool) {
    let node = node();
    let request_type = if with_input { INT32 } else { EMPTY };
    let input = if with_input { five() } else { vec![] };
    let service = node.serve("/foo", request_type, INT32).unwrap();
    let timeout = Duration::from_millis(1000);
    assert_eq!(
        node.request(
            "invalid service",
            request_type,
            INT32,
            input.clone(),
            timeout
        ),
        Err(Error::InvalidName)
    );
    thread::scope(|scope| {
        let requested = scope.spawn(|| node.request("/foo", request_type, INT32, input, timeout));
        let request = service.recv_timeout(timeout).unwrap();
        if with_input {
            assert_eq!(Int32::decode(request.data.as_slice()).unwrap().data, 5);
        } else {
            assert!(request.data.is_empty());
        }
        request.respond(five(), true).unwrap();
        let (response, result) = requested.join().unwrap().unwrap();
        assert!(result);
        assert_eq!(Int32::decode(response.as_slice()).unwrap().data, 5);
    });
}
fn sync_timeout(with_input: bool) {
    let node = node();
    let request_type = if with_input { INT32 } else { EMPTY };
    let input = if with_input { five() } else { vec![] };
    let start = Instant::now();
    let result = node.request(
        "/foo",
        request_type,
        INT32,
        input,
        Duration::from_millis(1000),
    );
    let elapsed = start.elapsed().as_millis();
    assert!(elapsed.abs_diff(1000) <= 200, "elapsed={elapsed}ms");
    assert_eq!(result, Err(Error::Timeout));
}
#[test]
fn original_service_call_sync() {
    sync_service(true);
}
#[test]
fn original_service_call_without_input_sync() {
    sync_service(false);
}
#[test]
fn original_service_call_sync_timeout() {
    sync_timeout(true);
}
#[test]
fn original_service_call_without_input_sync_timeout() {
    sync_timeout(false);
}

fn async_calls(with_input: bool, with_output: bool, batches: &[usize]) {
    let node = node();
    let request_type = if with_input { INT32 } else { EMPTY };
    let response_type = if with_output { INT32 } else { EMPTY };
    let input = if with_input { five() } else { vec![] };
    assert!(matches!(
        node.serve("invalid service", request_type, response_type),
        Err(Error::InvalidName)
    ));
    let service = node.serve("/foo", request_type, response_type).unwrap();
    assert_eq!(node.advertised_services().unwrap(), ["/foo"]);
    let timeout = Duration::from_secs(1);
    if with_output {
        assert_eq!(
            node.request(
                "invalid service",
                request_type,
                response_type,
                input.clone(),
                timeout
            ),
            Err(Error::InvalidName)
        );
    } else {
        assert_eq!(
            node.request_oneway("invalid service", request_type, input.clone(), timeout),
            Err(Error::InvalidName)
        );
    }
    for &expected in batches {
        let mut dispatched = 0;
        if with_output {
            thread::scope(|scope| {
                let mut calls = Vec::new();
                for _ in 0..expected {
                    let input = input.clone();
                    let node = &node;
                    calls.push(scope.spawn(move || {
                        node.request("/foo", request_type, response_type, input, timeout)
                    }));
                }
                for _ in 0..expected {
                    let request = service.recv_timeout(timeout).unwrap();
                    assert!(request.expects_response());
                    if with_input {
                        assert_eq!(Int32::decode(request.data.as_slice()).unwrap().data, 5);
                    } else {
                        assert!(request.data.is_empty());
                    }
                    dispatched += 1;
                    request.respond(five(), true).unwrap();
                }
                let mut completed = 0;
                for call in calls {
                    let (response, result) = call.join().unwrap().unwrap();
                    assert!(result);
                    assert_eq!(Int32::decode(response.as_slice()).unwrap().data, 5);
                    completed += 1;
                }
                assert_eq!(completed, expected);
            });
        } else {
            for _ in 0..expected {
                node.request_oneway("/foo", request_type, input.clone(), timeout)
                    .unwrap();
            }
            while let Ok(request) = service.recv_timeout(Duration::ZERO) {
                assert!(!request.expects_response());
                assert_eq!(Int32::decode(request.data.as_slice()).unwrap().data, 5);
                dispatched += 1;
            }
        }
        assert_eq!(dispatched, expected);
        assert!(matches!(
            service.recv_timeout(Duration::ZERO),
            Err(Error::Timeout)
        ));
    }
    assert_eq!(
        node.unadvertise_service("invalid service"),
        Err(Error::InvalidName)
    );
    node.unadvertise_service("/foo").unwrap();
    assert!(node.advertised_services().unwrap().is_empty());
}

#[test]
fn original_service_call_async() {
    async_calls(true, true, &[1, 1]);
}
#[test]
fn original_service_call_without_input_async() {
    async_calls(false, true, &[1, 1]);
}

#[test]
fn original_multiple_service_call_async() {
    async_calls(true, true, &[1, 3]);
}
#[test]
fn original_multiple_service_call_without_input_async() {
    async_calls(false, true, &[1, 3]);
}
#[test]
fn original_multiple_service_without_output_call_async() {
    async_calls(true, false, &[1, 3]);
}

fn resolved(request: &str, response: &str) -> gz_transport::ServiceTypeResolution {
    gz_transport::ServiceTypeResolution::Resolved(gz_transport::ServiceTypes {
        request_type: request.into(),
        response_type: response.into(),
    })
}

#[test]
fn original_resolve_service_types() {
    use gz_transport::ServiceTypeResolution::*;
    let node = node();
    let _service = node.serve("/foo", INT32, INT32).unwrap();
    assert_eq!(
        node.resolve_service_types("/foo", "", "", Duration::ZERO),
        Ok(resolved(INT32, INT32))
    );
    assert_eq!(
        node.resolve_service_types("/foo", INT32, "", Duration::ZERO),
        Ok(resolved(INT32, INT32))
    );
    let request = "custom.msgs.Type";
    let response = String::new();
    assert_eq!(
        node.resolve_service_types("/foo", request, &response, Duration::ZERO),
        Ok(IncompatibleTypes)
    );
    assert_eq!(request, "custom.msgs.Type");
    assert!(response.is_empty());
    assert_eq!(
        node.resolve_service_types("/unadvertised", request, request, Duration::ZERO),
        Ok(resolved(request, request))
    );
    assert_eq!(
        node.resolve_service_types("invalid service", "", "", Duration::ZERO),
        Err(Error::InvalidName)
    );
    let request = String::new();
    let response = String::new();
    let start = Instant::now();
    assert_eq!(
        node.resolve_service_types(
            "/unadvertised",
            &request,
            &response,
            Duration::from_millis(300)
        ),
        Ok(NoProviders)
    );
    assert!(start.elapsed().as_millis() >= 300);
    assert!(request.is_empty());
    assert!(response.is_empty());
}

#[test]
fn original_resolve_service_types_waits_for_provider() {
    let config = Config {
        partition: format!("resolve-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    };
    let (done, wait) = std::sync::mpsc::sync_channel(1);
    thread::scope(|scope| {
        let provider_config = config.clone();
        scope.spawn(move || {
            thread::sleep(Duration::from_millis(300));
            let provider = Node::new(provider_config).unwrap();
            let _service = provider.serve("/foo", INT32, INT32).unwrap();
            // Bounded cleanup also releases the provider if an assertion unwinds.
            let _ = wait.recv_timeout(Duration::from_secs(6));
        });
        let node = Node::new(config).unwrap();
        let start = Instant::now();
        assert_eq!(
            node.resolve_service_types("/foo", "", "", Duration::from_millis(5000)),
            Ok(resolved(INT32, INT32))
        );
        assert!(start.elapsed().as_millis() < 5000);
        done.send(()).unwrap();
    });
}

#[test]
fn discovered_resolve_service_types_ambiguous() {
    use gz_transport::ServiceTypeResolution::*;
    let config = Config {
        partition: format!("ambiguous-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    };
    let first = Node::new(config.clone()).unwrap();
    let second = Node::new(config).unwrap();
    let _one = first.serve("/foo", INT32, INT32).unwrap();
    let _two = second.serve("/foo", INT32, "gz.msgs.StringMsg").unwrap();
    // Native Nodes share discovery immediately; independent Rust workers must
    // first observe both real advertisements. This setup mapping is explicit.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if first.service_type_signatures("/foo").unwrap().len() == 2 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "second signature was not discovered"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        first.resolve_service_types("/foo", "", "", Duration::ZERO),
        Ok(AmbiguousTypes)
    );
    assert_eq!(
        first.resolve_service_types("/foo", "", "gz.msgs.StringMsg", Duration::ZERO),
        Ok(resolved(INT32, "gz.msgs.StringMsg"))
    );
    let request = String::new();
    assert_eq!(
        first.resolve_service_types("/foo", &request, "gz.msgs.Vector3d", Duration::ZERO),
        Ok(IncompatibleTypes)
    );
    assert!(request.is_empty());
}

#[test]
fn service_type_resolution_supports_concurrency_withdrawal_and_cancellation() {
    use gz_transport::ServiceTypeResolution::*;
    let node = node();
    let service = node.serve("/foo", INT32, INT32).unwrap();
    thread::scope(|scope| {
        let queries: Vec<_> = (0..8)
            .map(|_| scope.spawn(|| node.resolve_service_types("/foo", "", INT32, Duration::ZERO)))
            .collect();
        for query in queries {
            assert_eq!(query.join().unwrap(), Ok(resolved(INT32, INT32)));
        }
    });
    assert!(matches!(
        service.recv_timeout(Duration::from_millis(5)),
        Err(Error::Timeout)
    ));
    assert_eq!(node.service_type_signatures("/foo").unwrap().len(), 1);
    node.unadvertise_service("/foo").unwrap();
    assert!(node.service_type_signatures("/foo").unwrap().is_empty());
    assert_eq!(
        node.resolve_service_types("/foo", "", "", Duration::ZERO),
        Ok(NoProviders)
    );
    thread::scope(|scope| {
        let query =
            scope.spawn(|| node.resolve_service_types("/missing", "", "", Duration::from_secs(5)));
        // A successful command round-trip establishes that the worker is alive.
        node.service_type_signatures("/missing").unwrap();
        node.cancel();
        assert_eq!(query.join().unwrap(), Err(Error::Shutdown));
    });
}

#[test]
fn discovered_signatures_select_requests_and_deduplicate_providers() {
    use gz_transport::ServiceTypeResolution::*;
    let config = Config {
        partition: format!("select-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    };
    let first = Node::new(config.clone()).unwrap();
    let duplicate = Node::new(config.clone()).unwrap();
    let other = Node::new(config).unwrap();
    let local = first.serve("/foo", INT32, INT32).unwrap();
    let repeated = duplicate.serve("/foo", INT32, INT32).unwrap();
    let different = other.serve("/foo", "gz.msgs.StringMsg", INT32).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if first.service_type_signatures("/foo").unwrap().len() == 2
            && duplicate.service_type_signatures("/foo").unwrap().len() == 2
            && other.service_type_signatures("/foo").unwrap().len() == 2
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "provider signatures were not discovered"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        first.resolve_service_types("/foo", "", "", Duration::ZERO),
        Ok(AmbiguousTypes)
    );
    assert_eq!(
        first.resolve_service_types("/foo", "", INT32, Duration::ZERO),
        Ok(AmbiguousTypes)
    );
    assert_eq!(
        first.resolve_service_types("/foo", INT32, "", Duration::ZERO),
        Ok(resolved(INT32, INT32))
    );
    assert_eq!(
        first.resolve_service_types("/foo", "gz.msgs.StringMsg", "", Duration::ZERO),
        Ok(resolved("gz.msgs.StringMsg", INT32))
    );
    drop((local, repeated, different));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !first.service_type_signatures("/foo").unwrap().is_empty() {
        assert!(
            Instant::now() < deadline,
            "withdrawn signatures remained visible"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        first.resolve_service_types("/foo", "not.advertised", "", Duration::ZERO),
        Ok(NoProviders)
    );
}
