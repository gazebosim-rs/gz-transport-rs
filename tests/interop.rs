/*
 * Copyright (C) 2014 Open Source Robotics Foundation
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 *
*/
// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Subscriber-only assertion ports: Copyright (C) 2014 Open Source Robotics Foundation.
use gz_msgs::{prost::Message, stringmsg::StringMsg, vector3d::Vector3d};
use gz_transport::{AdvertiseOptions, Config, Error, Node, Scope, SubscribeOptions};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdout, Command, Stdio},
    sync::{Arc, Barrier},
    thread,
    time::{Duration, Instant},
};
const TYPE: &str = "gz.msgs.StringMsg";
fn data(value: &str) -> Vec<u8> {
    StringMsg {
        data: value.into(),
        ..Default::default()
    }
    .encode_to_vec()
}
fn config() -> Config {
    let id = uuid::Uuid::new_v4();
    Config {
        partition: format!("rust-{}", id),
        message_port: 10317,
        service_port: 10318,
        ..Default::default()
    }
}

// Pinned Node/PublisherPrivate ownership: the native publisher remains valid,
// publishable and visible after its Node dies, until that publisher is dropped.
#[test]
fn publisher_retains_worker_after_ordinary_node_drop() {
    let config = config();
    let observer = Node::new(config.clone()).unwrap();
    let owner = Node::new(config.clone()).unwrap();
    let publisher = owner.advertise("/survivor", TYPE).unwrap();
    let subscription = owner.subscribe("/survivor", TYPE).unwrap();
    let service = owner.serve("/owner-service", TYPE, TYPE).unwrap();
    assert_eq!(observer.known_topics().unwrap(), ["/survivor"]);
    assert_eq!(
        observer.known_topic_info("/survivor").unwrap().publishers,
        [TYPE]
    );
    assert!(publisher.has_connections().unwrap());
    assert!(publisher.connection_snapshot().unwrap());
    publisher.publish(data("queued")).unwrap();
    // Worker replies after the queued publish command, fixing the drain order.
    publisher.has_connections().unwrap();
    drop(owner);
    assert_eq!(publisher.connection_snapshot(), Ok(false));

    assert_eq!(
        subscription
            .recv_timeout(Duration::from_millis(20))
            .unwrap()
            .data,
        data("queued")
    );
    assert_eq!(
        subscription.recv_timeout(Duration::from_millis(20)),
        Err(Error::Shutdown)
    );
    assert!(matches!(
        service.recv_timeout(Duration::from_millis(20)),
        Err(Error::Shutdown)
    ));
    assert_eq!(publisher.publish(data("after")), Ok(()));
    assert_eq!(observer.known_topics().unwrap(), ["/survivor"]);
    assert_eq!(
        observer.known_topic_info("/survivor").unwrap().publishers,
        [TYPE]
    );
    let last_lease = Instant::now();
    drop(publisher);
    assert!(last_lease.elapsed() < Duration::from_secs(1));
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if observer.known_topics().unwrap().is_empty() {
            break;
        }
        assert!(Instant::now() < deadline, "last publisher remained visible");
        thread::sleep(Duration::from_millis(10));
    }
    drop(service);
    drop(subscription);
}

#[test]
fn owner_drop_retires_queued_service_reply_with_live_publisher() {
    let config = config();
    let owner = Node::new(config.clone()).unwrap();
    let publisher = owner.advertise("/survivor", TYPE).unwrap();
    let service = owner.serve("/owner-service", TYPE, TYPE).unwrap();
    let requester = Node::new(config).unwrap();
    // Discovery can finish after the requester starts. Establish visibility
    // before its unchanged 300 ms request deadline begins.
    let ready_deadline = Instant::now() + Duration::from_secs(2);
    while requester
        .service_provider_types("/owner-service")
        .unwrap()
        .is_empty()
    {
        assert!(
            Instant::now() < ready_deadline,
            "provider was not discovered"
        );
        thread::sleep(Duration::from_millis(10));
    }
    let task = thread::spawn(move || {
        requester.request(
            "/owner-service",
            TYPE,
            TYPE,
            data("request"),
            Duration::from_millis(300),
        )
    });
    let request = service.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(request.data, data("request"));
    drop(owner);
    assert_eq!(request.respond(data("reply"), true), Err(Error::Shutdown));
    assert!(matches!(
        service.recv_timeout(Duration::from_millis(20)),
        Err(Error::Shutdown)
    ));
    assert_eq!(task.join().unwrap(), Err(Error::Timeout));
    assert_eq!(publisher.publish(data("still-live")), Ok(()));
}

#[test]
fn publisher_outlives_node_and_delivers_to_native_peer() {
    let config = config();
    let owner = Node::new(config.clone()).unwrap();
    let publisher = owner.advertise("/survivor-wire", TYPE).unwrap();
    let mut native = Peer::start(&config, "subscribe", "/survivor-wire", "after-drop");
    drop(owner);
    // Native READY confirms local Subscribe admission, before its remote
    // interest reaches this Publisher. Keep the existing 100 publish attempts.
    let ready_deadline = Instant::now() + Duration::from_secs(4);
    while !publisher.has_connections().unwrap() {
        assert!(
            Instant::now() < ready_deadline,
            "native interest was not discovered"
        );
        thread::sleep(Duration::from_millis(10));
    }
    for _ in 0..100 {
        publisher.publish(data("after-drop")).unwrap();
        thread::sleep(Duration::from_millis(20));
    }
    native.success();
}

#[test]
fn final_publisher_drop_and_concurrent_snapshot_are_bounded() {
    let config = config();
    let observer = Node::new(config.clone()).unwrap();
    let owner = Node::new(config).unwrap();
    let publisher = owner.advertise("/racing", TYPE).unwrap();
    drop(owner);
    let barrier = Arc::new(Barrier::new(2));
    let dropper = thread::spawn({
        let barrier = barrier.clone();
        move || {
            barrier.wait();
            drop(publisher);
        }
    });
    barrier.wait();
    for _ in 0..32 {
        let names = observer.known_topics().unwrap();
        assert!(names.is_empty() || names == ["/racing"]);
    }
    dropper.join().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if observer.known_topics().unwrap().is_empty() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "withdrawal outlived bounded snapshot"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

// Original Node_TEST.cc duplicate-publisher assertions: Copyright (C) 2014
// Open Source Robotics Foundation. Invalid C++ handles map to explicit errors.
#[test]
fn original_advertise_two_equal_topics() {
    let config = config();
    let first = Node::new(config.clone()).unwrap();
    let second = Node::new(config).unwrap();
    let _one = first.advertise("/foo", "gz.msgs.Int32").unwrap();
    assert!(matches!(
        first.advertise("/foo", TYPE),
        Err(Error::AlreadyAdvertised)
    ));
    let _two = second.advertise("/foo", TYPE).unwrap();
}

// Node_TEST.cc:NodeTest.TopicListRemap. The native source uses /foo and /bar.
#[test]
fn original_topic_list_remap() {
    let mut config = config();
    config.add_topic_remap("/foo", "/bar").unwrap();
    let node = Node::new(config).unwrap();
    let _publisher = node.advertise("/foo", "gz.msgs.Int32").unwrap();
    let topics = node.advertised_topics().unwrap();
    assert_eq!(topics.len(), 1);
    assert_eq!(topics[0], "/bar");
}

// Preserve the same original predicates through the discovered topic query.
#[test]
fn original_topic_list_remap_discovered() {
    let mut config = config();
    config.add_topic_remap("/foo", "/bar").unwrap();
    let node = Node::new(config).unwrap();
    let _publisher = node
        .advertise_typed::<gz_msgs::int32::Int32>("/foo")
        .unwrap();

    let topics = node.known_topics().unwrap();
    assert_eq!(topics.len(), 1);
    assert_eq!(topics[0], "/bar");
}

#[test]
fn discovered_topic_list_remap_preserves_live_subscribers() {
    let mut config = config();
    config.add_topic_remap("/foo", "/bar").unwrap();
    let node = Node::new(config).unwrap();
    let publisher = node
        .advertise_typed::<gz_msgs::int32::Int32>("/foo")
        .unwrap();
    let subscription = node.subscribe("/foo", "gz.msgs.Int32").unwrap();
    assert_eq!(node.known_topics().unwrap(), ["/bar"]);
    assert_eq!(node.known_publisher_topics().unwrap(), ["/bar"]);
    assert!(node.known_services().unwrap().is_empty());

    drop(publisher);
    assert!(node.known_publisher_topics().unwrap().is_empty());
    assert_eq!(node.known_topics().unwrap(), ["/bar"]);
    drop(subscription);
    assert!(node.known_topics().unwrap().is_empty());

    let replacement = node
        .advertise_typed::<gz_msgs::int32::Int32>("/foo")
        .unwrap();
    assert_eq!(node.known_topics().unwrap(), ["/bar"]);
    drop(replacement);
    assert!(node.known_topics().unwrap().is_empty());
}

// Node_TEST.cc:NodeTest.TopicList (2014 OSRF/Apache-2.0). The native
// original creates two Nodes in one process and bounds only the second call.
#[test]
fn original_two_node_topic_list() {
    let config = config();
    let node1 = Node::new(config.clone()).unwrap();
    let node2 = Node::new(config).unwrap();
    let _pub1 = node1.advertise("topic1", "gz.msgs.Int32").unwrap();
    let _pub2 = node2.advertise("topic2", "gz.msgs.Int32").unwrap();

    let mut topics = node1.known_topics().unwrap();
    assert_eq!(topics.len(), 2);
    topics.clear();

    let start = Instant::now();
    topics = node1.known_topics().unwrap();
    let elapsed = start.elapsed();
    assert_eq!(topics.len(), 2);
    assert!(elapsed.as_millis() < 2, "second list: {elapsed:?}");
}

// Node_TEST.cc:NodeTest.AllTopicInfo (2014 OSRF/Apache-2.0). The native
// original queries immediately after four same-process Node registrations.
#[test]
fn original_four_node_all_topic_info() {
    let config = config();
    let node1 = Node::new(config.clone()).unwrap();
    let node2 = Node::new(config.clone()).unwrap();
    let node3 = Node::new(config.clone()).unwrap();
    let node4 = Node::new(config).unwrap();
    let _pub1 = node1.advertise("/topic1", "gz.msgs.Int32").unwrap();
    let _pub2 = node2.advertise("/topic2", "gz.msgs.Int32").unwrap();
    let _sub2 = node2.subscribe("/topic1", "gz.msgs.Int32").unwrap();
    let _sub3 = node3.subscribe("/topic3", "gz.msgs.Int32").unwrap();

    let topics = node4.known_topics().unwrap();
    assert_eq!(topics.len(), 3);
    for (topic, publishers, subscribers) in
        [("/topic1", 1, 1), ("/topic2", 1, 0), ("/topic3", 0, 1)]
    {
        assert!(topics.iter().any(|name| name == topic));
        let info = node4.known_topic_info(topic).unwrap();
        assert_eq!(info.publishers.len(), publishers, "{topic} publishers");
        assert_eq!(info.subscribers.len(), subscribers, "{topic} subscribers");
    }
}

#[test]
fn original_four_node_all_topic_info_ready() {
    let config = config();
    let node1 = Node::new(config.clone()).unwrap();
    let node2 = Node::new(config.clone()).unwrap();
    let node3 = Node::new(config.clone()).unwrap();
    let node4 = Node::new(config).unwrap();
    let _pub1 = node1
        .advertise_typed::<gz_msgs::int32::Int32>("/topic1")
        .unwrap();
    let _pub2 = node2
        .advertise_typed::<gz_msgs::int32::Int32>("/topic2")
        .unwrap();
    let _sub2 = node2.subscribe("/topic1", "gz.msgs.Int32").unwrap();
    let _sub3 = node3.subscribe("/topic3", "gz.msgs.Int32").unwrap();

    // Native AllTopicInfo initializes discovery before enumerating topic names.
    node4.topic_info("/topic1", Duration::from_secs(5)).unwrap();
    let topic_info: Vec<_> = node4
        .known_topics()
        .unwrap()
        .into_iter()
        .map(|topic| {
            let endpoints = node4.topic_info(&topic, Duration::from_secs(5)).unwrap();
            (topic, endpoints)
        })
        .collect();
    assert_eq!(topic_info.len(), 3);
    for (topic, publishers, subscribers) in
        [("/topic1", 1, 1), ("/topic2", 1, 0), ("/topic3", 0, 1)]
    {
        let (_, info) = topic_info
            .iter()
            .find(|(name, _)| name == topic)
            .unwrap_or_else(|| panic!("missing topic {topic}"));
        assert_eq!(info.publishers.len(), publishers, "{topic} publishers");
        assert_eq!(info.subscribers.len(), subscribers, "{topic} subscribers");
    }
}

#[test]
fn process_local_topic_info_endpoint_identity_and_lifecycle() {
    let config = config();
    let observer = Node::new(config.clone()).unwrap();
    let first = Node::new(config.clone()).unwrap();
    let second = Node::new(config.clone()).unwrap();
    let _third = Node::new(config.clone()).unwrap();
    let pub1 = first.advertise("/shared", "gz.msgs.Int32").unwrap();
    let pub2 = second.advertise("/shared", "gz.msgs.Int32").unwrap();
    let sub1 = first.subscribe("/shared", "gz.msgs.Int32").unwrap();
    let sub1_duplicate = first.subscribe("/shared", "gz.msgs.Int32").unwrap();
    let sub2 = second.subscribe("/shared", "gz.msgs.Int32").unwrap();
    let check = |publishers, subscribers| {
        let info = observer.known_topic_info("/shared").unwrap();
        assert_eq!(info.publishers, vec!["gz.msgs.Int32"; publishers]);
        assert_eq!(info.subscribers, vec!["gz.msgs.Int32"; subscribers]);
    };
    check(2, 2);
    // Allow multicast peer copies to arrive; their wire identities must not
    // double-count the same endpoints already present in the local index.
    thread::sleep(Duration::from_millis(200));
    check(2, 2);
    drop(sub1);
    check(2, 2);
    drop(sub1_duplicate);
    check(2, 1);
    drop(sub2);
    check(2, 0);
    drop(pub1);
    check(1, 0);
    drop(pub2);
    check(0, 0);
    assert!(!observer.known_topics().unwrap().contains(&"/shared".into()));
}

#[test]
fn process_local_topic_info_isolated_by_domain_remap_and_shutdown() {
    let config = config();
    let observer = Node::new(config.clone()).unwrap();
    let mut mapped = config.clone();
    mapped.add_topic_remap("/raw", "/mapped").unwrap();
    let mut publisher = Node::new(mapped.clone()).unwrap();
    let _pub = publisher.advertise("/raw", "gz.msgs.Int32").unwrap();
    let _sub = publisher.subscribe("/raw", "gz.msgs.Int32").unwrap();
    assert_eq!(
        observer.known_topic_info("/mapped").unwrap().publishers,
        ["gz.msgs.Int32"]
    );
    assert_eq!(
        observer.known_topic_info("/mapped").unwrap().subscribers,
        ["gz.msgs.Int32"]
    );
    assert_eq!(
        observer.known_topic_info("/raw").unwrap().publishers.len(),
        0
    );
    assert_eq!(
        observer.known_topic_info("/raw").unwrap().subscribers.len(),
        0
    );
    let mut foreign = mapped;
    foreign.partition.push_str("-other");
    let outside = Node::new(foreign).unwrap();
    assert!(
        outside
            .known_topic_info("/mapped")
            .unwrap()
            .publishers
            .is_empty()
    );
    assert!(
        outside
            .known_topic_info("/mapped")
            .unwrap()
            .subscribers
            .is_empty()
    );
    let mut other_port = config;
    other_port.message_port += 1;
    other_port.service_port += 1;
    let separate_domain = Node::new(other_port).unwrap();
    let separate = separate_domain.known_topic_info("/mapped").unwrap();
    assert!(separate.publishers.is_empty());
    assert!(separate.subscribers.is_empty());
    publisher.shutdown();
    assert!(
        observer
            .known_topic_info("/mapped")
            .unwrap()
            .publishers
            .is_empty()
    );
    assert!(
        observer
            .known_topic_info("/mapped")
            .unwrap()
            .subscribers
            .is_empty()
    );
}

#[test]
fn process_local_topic_list_domain_and_lifecycle() {
    let mut config = config();
    config.partition = format!("/{}/", config.partition);
    let first = Node::new(config.clone()).unwrap();
    let mut second_config = config.clone();
    second_config.partition = config.partition.trim_matches('/').into();
    second_config.set_namespace("other").unwrap();
    second_config.add_topic_remap("/raw", "/mapped").unwrap();
    let second = Node::new(second_config).unwrap();
    let first_pub = first.advertise("/same", "gz.msgs.Int32").unwrap();
    let second_pub = second.advertise("/same", "gz.msgs.Int32").unwrap();
    let mapped = second.advertise("/raw", "gz.msgs.Int32").unwrap();
    let namespaced = second.advertise("named", "gz.msgs.Int32").unwrap();
    assert_eq!(
        first.known_topics().unwrap(),
        ["/mapped", "/other/named", "/same"]
    );
    assert_eq!(
        first.known_publisher_topics().unwrap(),
        ["/mapped", "/other/named", "/same"]
    );

    let mut foreign_config = config.clone();
    foreign_config.message_port += 2;
    let foreign = Node::new(foreign_config).unwrap();
    let _foreign_pub = foreign.advertise("/foreign", "gz.msgs.Int32").unwrap();
    let mut other_partition = config.clone();
    other_partition.partition.push_str("foreign");
    let partition_peer = Node::new(other_partition).unwrap();
    let _partition_pub = partition_peer
        .advertise("/partition-foreign", "gz.msgs.Int32")
        .unwrap();
    assert_eq!(
        first.known_topics().unwrap(),
        ["/mapped", "/other/named", "/same"]
    );

    drop(second_pub);
    drop(mapped);
    drop(namespaced);
    drop(second);
    drop(first_pub);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if first.known_topics().unwrap().is_empty() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "withdrawn topic remained visible"
        );
        thread::sleep(Duration::from_millis(10));
    }
    let new_pub = first.advertise("/same", "gz.msgs.Int32").unwrap();
    assert_eq!(first.known_topics().unwrap(), ["/same"]);
    drop(new_pub);

    let mut retiring = Node::new(config).unwrap();
    let surviving_handle = retiring.advertise("/retiring", "gz.msgs.Int32").unwrap();
    assert_eq!(first.known_topics().unwrap(), ["/retiring"]);
    retiring.shutdown();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if first.known_topics().unwrap().is_empty() {
            break;
        }
        assert!(Instant::now() < deadline, "stopped Node remained visible");
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(surviving_handle.publish(vec![1]), Err(Error::Shutdown));
}

#[test]
fn process_local_topic_list_concurrent_admission_and_query() {
    let config = config();
    let query = Node::new(config.clone()).unwrap();
    let producer = thread::spawn(move || {
        let node = Node::new(config).unwrap();
        for _ in 0..32 {
            let publisher = node.advertise("/cycling", "gz.msgs.Int32").unwrap();
            thread::yield_now();
            drop(publisher);
        }
    });
    for _ in 0..64 {
        let topics = query.known_topics().unwrap();
        assert!(topics.is_empty() || topics == ["/cycling"]);
    }
    producer.join().unwrap();
}

#[test]
fn original_advertise_same_topic_same_type_on_same_node() {
    let node = Node::new(config()).unwrap();
    let _one = node.advertise("/foo", "gz.msgs.Int32").unwrap();
    assert!(matches!(
        node.advertise("/foo", "gz.msgs.Int32"),
        Err(Error::AlreadyAdvertised)
    ));
}

#[test]
fn original_advertise_relative_topic_twice() {
    let node = Node::new(config()).unwrap();
    let _one = node.advertise("relative_topic", "gz.msgs.Int32").unwrap();
    assert!(matches!(
        node.advertise("relative_topic", "gz.msgs.Int32"),
        Err(Error::AlreadyAdvertised)
    ));
}

#[test]
fn original_advertise_namespaced_topic_twice() {
    let mut config = config();
    config.set_namespace("my_ns").unwrap();
    let node = Node::new(config).unwrap();
    let _one = node.advertise("ns_topic", "gz.msgs.Int32").unwrap();
    assert!(matches!(
        node.advertise("ns_topic", "gz.msgs.Int32"),
        Err(Error::AlreadyAdvertised)
    ));
    assert!(matches!(
        node.advertise("/my_ns/ns_topic", TYPE),
        Err(Error::AlreadyAdvertised)
    ));
}
struct Peer(Child, BufReader<ChildStdout>);
impl Peer {
    fn start(config: &Config, mode: &str, topic: &str, value: &str) -> Self {
        let path = std::env::var("GZ_CPP_PEER")
            .unwrap_or_else(|_| format!("{}/artifacts/cpp-peer", env!("CARGO_MANIFEST_DIR")));
        let mut child = Command::new(path)
            .args([mode, topic, value])
            .env("GZ_PARTITION", &config.partition)
            .env("GZ_IP", config.interface.to_string())
            .env("GZ_DISCOVERY_MULTICAST_IP", config.multicast.to_string())
            .env(
                "GZ_TRANSPORT_SNDHWM",
                config.send_high_watermark.to_string(),
            )
            .env(
                "GZ_TRANSPORT_RCVHWM",
                config.receive_high_watermark.to_string(),
            )
            .env("GZ_DISCOVERY_MSG_PORT", config.message_port.to_string())
            .env("GZ_DISCOVERY_SRV_PORT", config.service_port.to_string())
            .env("GZ_TRANSPORT_IMPLEMENTATION", "zeromq")
            .env("GZ_TRANSPORT_TOPIC_STATISTICS", "0")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("Build the required real C++ peer with scripts/build-cpp-peer.sh");
        let mut ready = String::new();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        stdout.read_line(&mut ready).unwrap();
        assert_eq!(ready.trim(), "READY");
        Self(child, stdout)
    }
    fn success(&mut self) {
        assert!(self.0.wait().unwrap().success());
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

// twoProcsPubSub.cc:LocalSubscriberReceivesOnceWithRemoteSubscriber.
// Original assertion adaptation: Copyright (C) 2014 Open Source Robotics Foundation.
#[test]
fn original_local_subscriber_receives_once_with_remote_subscriber() {
    let config = config();
    let topic = "/local_and_remote";
    let mut peer = Peer::start(&config, "list-vector3d-subscriber", topic, "unused");
    let node = Node::new(config).unwrap();
    let subscription = node.subscribe(topic, "gz.msgs.Vector3d").unwrap();
    let publisher = node.advertise_typed::<Vector3d>(topic).unwrap();

    // Native waits for two distinct subscriber process UUIDs. The public
    // Rust descriptor snapshot can observe only their two type entries.
    let discovery_deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let info = node.known_topic_info(topic).unwrap();
        if info.subscribers == ["gz.msgs.Vector3d", "gz.msgs.Vector3d"] {
            break;
        }
        assert!(
            Instant::now() < discovery_deadline,
            "remote subscriber was not discovered"
        );
        thread::sleep(Duration::from_millis(50));
    }

    for index in 0..10 {
        publisher
            .publish_typed(&Vector3d {
                x: f64::from(index),
                y: 0.0,
                z: 0.0,
                ..Default::default()
            })
            .unwrap();
    }

    let receipt_deadline = Instant::now() + Duration::from_secs(5);
    for index in 0..10 {
        let delivered = loop {
            let now = Instant::now();
            assert!(
                now < receipt_deadline,
                "ten local messages were not received"
            );
            match subscription.recv_timeout(Duration::from_millis(10)) {
                Ok(delivered) => break delivered,
                Err(Error::Timeout) => continue,
                Err(error) => panic!("local subscription failed: {error:?}"),
            }
        };
        let actual = Vector3d::decode(delivered.data.as_slice()).unwrap();
        assert_eq!(actual.x, f64::from(index));
        assert_eq!(actual.y, 0.0);
        assert_eq!(actual.z, 0.0);
    }
    assert_eq!(
        subscription.recv_timeout(Duration::from_millis(300)),
        Err(Error::Timeout),
        "an extra local copy arrived after ten messages"
    );
    writeln!(peer.0.stdin.as_mut().unwrap(), "QUIT").unwrap();
    peer.success();
}

// Node_TEST.cc:IgnoreLocalMessages and
// twoProcsPubSub.cc:IgnoreLocalMessagesWithRemoteSubscriber.
// Original assertion adaptation: Copyright (C) 2014 Open Source Robotics Foundation.
#[test]
fn original_ignore_local_messages() {
    let node = Node::new(config()).unwrap();
    let publisher = node
        .advertise_typed::<gz_msgs::int32::Int32>("/foo")
        .unwrap();
    let mut options = SubscribeOptions::default();
    assert!(!options.ignore_local_messages());
    options.set_ignore_local_messages(true);
    assert!(options.ignore_local_messages());
    let ignored = node
        .subscribe_with_options("/foo", "gz.msgs.Int32", options)
        .unwrap();
    for _ in 0..3 {
        publisher
            .publish_typed(&gz_msgs::int32::Int32 {
                data: 5,
                ..Default::default()
            })
            .unwrap();
        thread::sleep(Duration::from_millis(100));
    }
    publisher.has_connections().unwrap();
    assert_eq!(
        ignored.recv_timeout(Duration::from_millis(20)),
        Err(Error::Timeout)
    );
}

#[test]
fn ignore_local_policy_is_per_subscription() {
    let node = Node::new(config()).unwrap();
    let ordinary = node.subscribe("/mixed-local", TYPE).unwrap();
    let mut options = SubscribeOptions::default();
    options.set_ignore_local_messages(true);
    let ignored = node
        .subscribe_with_options("/mixed-local", TYPE, options)
        .unwrap();
    let publisher = node.advertise("/mixed-local", TYPE).unwrap();
    publisher.publish(data("one")).unwrap();
    assert_eq!(
        ordinary.recv_timeout(Duration::from_secs(1)).unwrap().data,
        data("one")
    );
    assert_eq!(
        ignored.recv_timeout(Duration::from_millis(20)),
        Err(Error::Timeout)
    );
    drop(ordinary);
    publisher.publish(data("two")).unwrap();
    publisher.has_connections().unwrap();
    assert_eq!(
        ignored.recv_timeout(Duration::from_millis(20)),
        Err(Error::Timeout)
    );
}

#[test]
fn original_ignore_local_messages_with_remote_subscriber() {
    let config = config();
    let topic = "/ignore_local_remote";
    let mut peer = Peer::start(&config, "list-vector3d-subscriber", topic, "unused");
    let owner = Node::new(config.clone()).unwrap();
    let witness = Node::new(config).unwrap();
    let mut options = SubscribeOptions::default();
    options.set_ignore_local_messages(true);
    let ignored = owner
        .subscribe_with_options(topic, "gz.msgs.Vector3d", options)
        .unwrap();
    let observed = witness.subscribe(topic, "gz.msgs.Vector3d").unwrap();
    let publisher = owner.advertise_typed::<Vector3d>(topic).unwrap();

    // Native waits for a separate subscriber process UUID. The public Rust
    // snapshot can establish only this fixture's three endpoint descriptors.
    let discovery_deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let info = owner.known_topic_info(topic).unwrap();
        if info.subscribers == ["gz.msgs.Vector3d", "gz.msgs.Vector3d", "gz.msgs.Vector3d"] {
            break;
        }
        assert!(
            Instant::now() < discovery_deadline,
            "remote subscriber was not discovered"
        );
        thread::sleep(Duration::from_millis(50));
    }

    for _ in 0..10 {
        publisher.publish_typed(&Vector3d::default()).unwrap();
    }
    let receipt_deadline = Instant::now() + Duration::from_secs(5);
    for _ in 0..10 {
        loop {
            assert!(
                Instant::now() < receipt_deadline,
                "witness missed ten messages"
            );
            match observed.recv_timeout(Duration::from_millis(10)) {
                Ok(message) => {
                    assert_eq!(
                        Vector3d::decode(message.data.as_slice()).unwrap(),
                        Vector3d::default()
                    );
                    break;
                }
                Err(Error::Timeout) => continue,
                Err(error) => panic!("witness failed: {error:?}"),
            }
        }
    }
    assert_eq!(
        observed.recv_timeout(Duration::from_millis(300)),
        Err(Error::Timeout)
    );
    assert_eq!(
        ignored.recv_timeout(Duration::from_millis(20)),
        Err(Error::Timeout)
    );
    writeln!(peer.0.stdin.as_mut().unwrap(), "QUIT").unwrap();
    peer.success();
}

// Original twoProcsSrvCall*.cc assertions: Copyright (C) 2014 Open Source Robotics Foundation.
// The three twoProcsSrvCall*.ServiceList originals differ only in callback
// signature. Each peer is a separate native process with the matching types.
fn original_two_process_service_list(signature: &str) {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let mut native = Peer::start(&config, "service-info-reference", "/foo", signature);

    // Native waitForService polls ServiceInfo at 100 ms for up to ten seconds.
    let deadline = Instant::now() + Duration::from_secs(10);
    while node.service_type_signatures("/foo").unwrap().is_empty() {
        assert!(Instant::now() < deadline, "service /foo was not discovered");
        thread::sleep(Duration::from_millis(100));
    }

    let mut services = node.known_services().unwrap();
    assert_eq!(services.len(), 1);
    assert_eq!(services[0], "/foo");
    services.clear();

    let start = Instant::now();
    services = node.known_services().unwrap();
    let elapsed = start.elapsed();
    assert_eq!(services.len(), 1);
    assert_eq!(services[0], "/foo");
    assert!(elapsed < Duration::from_millis(2), "elapsed={elapsed:?}");

    native.0.stdin.take().unwrap().write_all(b"QUIT\n").unwrap();
    native.success();
}

#[test]
fn original_two_proc_service_list() {
    original_two_process_service_list("ordinary");
}

#[test]
fn original_two_proc_service_list_without_input() {
    original_two_process_service_list("inputless");
}

#[test]
fn original_two_proc_service_list_without_output() {
    original_two_process_service_list("outputless");
}

#[test]
fn known_lists_include_local_and_discovered_cpp_advertisements() {
    let mut config = config();
    let partition = config.partition.clone();
    config.partition = format!("/{partition}/");
    config
        .add_topic_remap("/local-topic", "/mapped-topic")
        .unwrap();
    config
        .add_topic_remap("/local-service", "/mapped-service")
        .unwrap();
    let node = Node::new(config.clone()).unwrap();
    let publisher = node.advertise("/local-topic", TYPE).unwrap();
    let service = node.serve("/local-service", TYPE, TYPE).unwrap();
    assert_eq!(node.known_publisher_topics().unwrap(), ["/mapped-topic"]);
    assert_eq!(node.known_services().unwrap(), ["/mapped-service"]);

    let mut native_config = config.clone();
    native_config.partition = partition;
    let mut remote_topic = Peer::start(&native_config, "publish", "/remote-topic", "wire");
    let mut remote_service = Peer::start(&native_config, "serve", "/remote-service", "wire");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let topics = node.known_publisher_topics().unwrap();
        let services = node.known_services().unwrap();
        if topics == ["/mapped-topic", "/remote-topic"]
            && services == ["/mapped-service", "/remote-service"]
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "topics={topics:?}; services={services:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(node.advertised_topics().unwrap(), ["/mapped-topic"]);
    assert_eq!(node.advertised_services().unwrap(), ["/mapped-service"]);

    drop(publisher);
    drop(service);
    assert_eq!(node.known_publisher_topics().unwrap(), ["/remote-topic"]);
    assert_eq!(node.known_services().unwrap(), ["/remote-service"]);
    remote_topic.success();
    remote_service.success();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let topics = node.known_publisher_topics().unwrap();
        let services = node.known_services().unwrap();
        if topics.is_empty() && services.is_empty() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "topics={topics:?}; services={services:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
    node.cancel();
    assert_eq!(node.known_publisher_topics(), Err(Error::Shutdown));
    assert_eq!(node.known_services(), Err(Error::Shutdown));
}
#[test]
fn known_topics_include_local_and_remote_subscriber_only_interests() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    assert!(node.known_topics().unwrap().is_empty());
    let _subscription = node.subscribe("/local-only", TYPE).unwrap();
    assert_eq!(node.known_topics().unwrap(), ["/local-only"]);
    assert!(node.known_publisher_topics().unwrap().is_empty());

    let mut native = Peer::start(&config, "list-subscriber", "/remote-only", "unused");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let topics = node.known_topics().unwrap();
        if topics == ["/local-only", "/remote-only"] {
            break;
        }
        assert!(Instant::now() < deadline, "topics={topics:?}");
        thread::sleep(Duration::from_millis(20));
    }
    assert!(node.known_publisher_topics().unwrap().is_empty());
    node.unsubscribe("/local-only").unwrap();
    assert_eq!(node.known_topics().unwrap(), ["/remote-only"]);

    native.0.stdin.take().unwrap().write_all(b"QUIT\n").unwrap();
    native.success();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let topics = node.known_topics().unwrap();
        if topics.is_empty() {
            break;
        }
        assert!(Instant::now() < deadline, "topics={topics:?}");
        thread::sleep(Duration::from_millis(20));
    }
    node.cancel();
    assert_eq!(node.known_topics(), Err(Error::Shutdown));
}

// Retain the existing StringMsg subscriber-only variant.
#[test]
fn original_topic_list_subscriber_only() {
    let node = Node::new(config()).unwrap();
    let _subscription = node.subscribe("/foo", TYPE).unwrap();
    let topics = node.known_topics().unwrap();
    assert_eq!(topics.len(), 1);
    assert_eq!(topics[0], "/foo");
    node.unsubscribe("/foo").unwrap();
    assert!(node.known_topics().unwrap().is_empty());
}

// Node_TEST.cc:TopicListSubscriberOnly uses cb(const msgs::Int32 &).
#[test]
fn original_int32_topic_list_subscriber_only() {
    let node = Node::new(config()).unwrap();
    let _subscription = node.subscribe("/foo", "gz.msgs.Int32").unwrap();
    let mut topics = node.known_topics().unwrap();
    assert_eq!(topics.len(), 1);
    assert_eq!(topics[0], "/foo");

    node.unsubscribe("/foo").unwrap();
    topics.clear();
    topics = node.known_topics().unwrap();
    assert!(topics.is_empty());
}

// Node_TEST.cc:NodeTest.TopicInfoSubscriberOnly, including every assertion.
// Original assertion provenance: Copyright (C) 2014 Open Source Robotics Foundation.
#[test]
fn original_topic_info_subscriber_only() {
    let node = Node::new(config()).unwrap();
    let _subscription = node.subscribe("/foo", "gz.msgs.Int32").unwrap();
    let info = node.known_topic_info("/foo").unwrap();
    assert!(info.publishers.is_empty());
    assert_eq!(info.subscribers.len(), 1);
    assert_eq!(info.subscribers[0], "gz.msgs.Int32");
}

#[test]
fn original_topic_info_subscriber_only_ready() {
    let node = Node::new(config()).unwrap();
    let _subscription = node.subscribe("/foo", "gz.msgs.Int32").unwrap();
    let info = node.topic_info("/foo", Duration::from_secs(5)).unwrap();
    assert!(info.publishers.is_empty());
    assert_eq!(info.subscribers.len(), 1);
    assert_eq!(info.subscribers[0], "gz.msgs.Int32");
}

#[test]
fn known_topic_info_uses_original_query_and_endpoint_multiplicity() {
    let mut config = config();
    config.add_topic_remap("/foo", "/bar").unwrap();
    let node = Node::new(config).unwrap();
    let _first = node.subscribe("/foo", TYPE).unwrap();
    let _duplicate = node.subscribe("/foo", TYPE).unwrap();
    let _other_type = node.subscribe("/foo", "gz.msgs.Int32").unwrap();
    let _wildcard = node.subscribe("/foo", "").unwrap();
    let _publisher = node.advertise("/foo", TYPE).unwrap();
    let unmapped = node.known_topic_info("/foo").unwrap();
    assert!(unmapped.publishers.is_empty());
    assert!(unmapped.subscribers.is_empty());
    let mapped = node.known_topic_info("/bar").unwrap();
    assert_eq!(mapped.publishers, [TYPE]);
    assert_eq!(
        mapped.subscribers,
        ["google.protobuf.Message", "gz.msgs.Int32", TYPE]
    );
    assert_eq!(
        node.known_topic_info("invalid topic"),
        Err(Error::InvalidName)
    );
    node.cancel();
    assert_eq!(node.known_topic_info("/bar"), Err(Error::Shutdown));
}

#[test]
fn known_topic_info_observes_cpp_subscriber_descriptor_and_withdrawal() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let mut native = Peer::start(&config, "list-subscriber", "/remote-info", "unused");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let info = node.known_topic_info("/remote-info").unwrap();
        if info.publishers.is_empty() && info.subscribers == [TYPE] {
            break;
        }
        assert!(Instant::now() < deadline, "info={info:?}");
        thread::sleep(Duration::from_millis(20));
    }
    native.0.stdin.take().unwrap().write_all(b"QUIT\n").unwrap();
    native.success();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let info = node.known_topic_info("/remote-info").unwrap();
        if info.publishers.is_empty() && info.subscribers.is_empty() {
            break;
        }
        assert!(Instant::now() < deadline, "info={info:?}");
        thread::sleep(Duration::from_millis(20));
    }
    let mut publisher = Peer::start(&config, "publish", "/remote-info", "wire");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let info = node.known_topic_info("/remote-info").unwrap();
        if info.publishers == [TYPE] && info.subscribers.is_empty() {
            break;
        }
        assert!(Instant::now() < deadline, "info={info:?}");
        thread::sleep(Duration::from_millis(20));
    }
    publisher.success();
}

// twoProcsPubSub.cc:twoProcPubSub.TopicInfo assertion provenance:
// Copyright (C) 2014 Open Source Robotics Foundation.
#[test]
fn original_two_process_topic_info() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let mut publisher = Peer::start(&config, "list-vector3d-publisher", "/foo", "unused");
    // Native waitForTopic uses a ten-second deadline and 100ms polling.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if node
            .known_publisher_topics()
            .unwrap()
            .iter()
            .any(|topic| topic == "/foo")
        {
            break;
        }
        assert!(Instant::now() < deadline, "remote /foo was not discovered");
        thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(node.known_topic_info("@"), Err(Error::InvalidName));
    let absent = node.known_topic_info("/bogus").unwrap();
    assert_eq!(absent.publishers.len(), 0);
    let found = node.known_topic_info("/foo").unwrap();
    assert_eq!(found.publishers.len(), 1);
    assert_eq!(found.publishers[0], "gz.msgs.Vector3d");
    publisher
        .0
        .stdin
        .take()
        .unwrap()
        .write_all(b"QUIT\n")
        .unwrap();
    publisher.success();
}

// twoProcsPubSub.cc:twoProcPubSub.AllTopicInfo assertion provenance:
// Copyright (C) 2014 Open Source Robotics Foundation.
#[test]
fn original_two_process_all_topic_info() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let mut publisher = Peer::start(&config, "list-vector3d-publisher", "/foo", "unused");
    let mut subscriber = Peer::start(
        &config,
        "list-vector3d-subscriber",
        "/subscriber_only",
        "unused",
    );
    let _local_subscription = node.subscribe("/foo", "gz.msgs.Int32").unwrap();

    // Each native waitForTopic call has a five-second deadline and 100ms poll.
    for target in ["/foo", "/subscriber_only"] {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if node
                .known_topics()
                .unwrap()
                .iter()
                .any(|topic| topic == target)
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "topic {target} was not discovered"
            );
            thread::sleep(Duration::from_millis(100));
        }
    }

    // Native AllTopicInfo collects TopicList, then TopicInfoHelper per topic.
    let info: Vec<_> = node
        .known_topics()
        .unwrap()
        .into_iter()
        .map(|topic| {
            let endpoints = node.known_topic_info(&topic).unwrap();
            (topic, endpoints)
        })
        .collect();
    let foo = info.iter().find(|(topic, _)| topic == "/foo").unwrap();
    assert_eq!(foo.1.publishers.len(), 1);
    assert_eq!(foo.1.subscribers.len(), 1);
    let subscriber_only = info
        .iter()
        .find(|(topic, _)| topic == "/subscriber_only")
        .unwrap();
    assert_eq!(subscriber_only.1.publishers.len(), 0);
    assert_eq!(subscriber_only.1.subscribers.len(), 1);

    for peer in [&mut publisher, &mut subscriber] {
        peer.0.stdin.take().unwrap().write_all(b"QUIT\n").unwrap();
        peer.success();
    }
}
#[test]
fn cpp_publisher_rust_subscriber() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let sub = node.subscribe("/wire", TYPE).unwrap();
    let _peer = Peer::start(&config, "publish", "/wire", "cpp");
    let message = sub.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(message.data, data("cpp"));
    assert_eq!(message.type_name, TYPE);
    assert_eq!(message.topic, format!("@/{}@/wire", config.partition));
}
#[test]
fn rust_publisher_cpp_subscriber_delayed_start() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let publisher = node.advertise("/wire", TYPE).unwrap();
    thread::sleep(Duration::from_millis(1100));
    let mut peer = Peer::start(&config, "subscribe", "/wire", "rust");
    for _ in 0..100 {
        publisher.publish(data("rust")).unwrap();
        thread::sleep(Duration::from_millis(20));
        if peer.0.try_wait().unwrap().is_some() {
            break;
        }
    }
    peer.success();
}
#[test]
fn rust_request_cpp_service() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let _peer = Peer::start(&config, "serve", "/echo", "-cpp");
    let (response, result) = node
        .request("/echo", TYPE, TYPE, data("rust"), Duration::from_secs(5))
        .unwrap();
    assert!(result);
    assert_eq!(response, data("rust-cpp"));
}
#[test]
fn cpp_request_rust_service() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let service = node.serve("/echo", TYPE, TYPE).unwrap();
    thread::sleep(Duration::from_millis(1100));
    let mut peer = Peer::start(&config, "request", "/echo", "cpp");
    let request = service.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(request.data, data("cpp"));
    request.respond(data("cpp-rust"), true).unwrap();
    peer.success();
}
#[test]
fn timeout_partition_shutdown_and_local_delivery() {
    let config = config();
    let mut node = Node::new(config.clone()).unwrap();
    let sub = node.subscribe("/local", TYPE).unwrap();
    let wrong = node.subscribe("/local", "wrong").unwrap();
    let publisher = node.advertise("/local", TYPE).unwrap();
    publisher.publish(data("local")).unwrap();
    assert_eq!(
        sub.recv_timeout(Duration::from_secs(1)).unwrap().data,
        data("local")
    );
    assert_eq!(
        wrong.recv_timeout(Duration::from_millis(10)),
        Err(Error::Timeout)
    );
    assert_eq!(
        node.request("/missing", TYPE, TYPE, vec![], Duration::from_millis(30)),
        Err(Error::Timeout)
    );
    let other = Node::new(Config {
        partition: "different".into(),
        ..config
    })
    .unwrap();
    let isolated = other.subscribe("/local", TYPE).unwrap();
    publisher.publish(data("private")).unwrap();
    assert_eq!(
        isolated.recv_timeout(Duration::from_millis(30)),
        Err(Error::Timeout)
    );
    let start = Instant::now();
    node.shutdown();
    assert!(start.elapsed() < Duration::from_secs(1));
    assert_eq!(publisher.publish(vec![]), Err(Error::Shutdown));
    assert_eq!(
        sub.recv_timeout(Duration::from_millis(10)).unwrap().data,
        data("private")
    );
    assert_eq!(
        sub.recv_timeout(Duration::from_millis(10)),
        Err(Error::Shutdown)
    );
}

#[test]
fn reconnect_and_delayed_responder() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let sub = node.subscribe("/reconnect", TYPE).unwrap();
    let peer = Peer::start(&config, "publish", "/reconnect", "before");
    assert_eq!(
        sub.recv_timeout(Duration::from_secs(4)).unwrap().data,
        data("before")
    );
    drop(peer);
    let _peer = Peer::start(&config, "publish", "/reconnect", "after");
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let message = sub
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
        if message.data == data("after") {
            break;
        }
    }
    thread::scope(|scope| {
        let request =
            scope.spawn(|| node.request("/late", TYPE, TYPE, data("late"), Duration::from_secs(4)));
        thread::sleep(Duration::from_millis(1100));
        let _server = Peer::start(&config, "serve", "/late", "-cpp");
        assert_eq!(request.join().unwrap(), Ok((data("late-cpp"), true)));
    });
}
#[test]
fn same_node_service_failure_and_concurrent_cancel() {
    let node = Node::new(config()).unwrap();
    let service = node.serve("/local_service", TYPE, TYPE).unwrap();
    thread::scope(|scope| {
        let request = scope.spawn(|| {
            node.request(
                "/local_service",
                TYPE,
                TYPE,
                data("input"),
                Duration::from_secs(2),
            )
        });
        let incoming = service.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(incoming.data, data("input"));
        incoming.respond(data("declined"), false).unwrap();
        assert_eq!(request.join().unwrap(), Ok((data("declined"), false)));
    });
    thread::scope(|scope| {
        let request =
            scope.spawn(|| node.request("/never", TYPE, TYPE, vec![], Duration::from_secs(30)));
        thread::sleep(Duration::from_millis(20));
        let start = Instant::now();
        node.cancel();
        assert_eq!(request.join().unwrap(), Err(Error::Shutdown));
        assert!(start.elapsed() < Duration::from_secs(1));
    });
    assert!(matches!(
        service.recv_timeout(Duration::from_secs(1)),
        Err(Error::Shutdown)
    ));
}

#[test]
fn malformed_discovery_is_ignored_and_counted() {
    // Hold a port reservation until the worker owns it, so parallel fixture
    // workers cannot steal this test's single unicast datagram on shared 10317.
    let reservation = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )
    .unwrap();
    reservation.set_reuse_address(true).unwrap();
    reservation
        .bind(&std::net::SocketAddrV4::new(std::net::Ipv4Addr::UNSPECIFIED, 0).into())
        .unwrap();
    let port = reservation
        .local_addr()
        .unwrap()
        .as_socket_ipv4()
        .unwrap()
        .port();
    let mut isolated = config();
    isolated.message_port = port;
    let node = Node::new(isolated).unwrap();
    drop(reservation);
    // A later default-port worker reproduces the competing listener that made
    // the former fixed-port fixture fail under parallel execution.
    let _competing = Node::new(config()).unwrap();
    let sender = std::net::UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    sender
        .send_to(&[255], (std::net::Ipv4Addr::LOCALHOST, port))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    while node.statistics().malformed_frames == 0 {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(node.statistics().malformed_frames, 1);
}

#[test]
fn configured_cpp_peer_partition_interface_ports_and_namespace() {
    let config = Config {
        partition: uuid::Uuid::new_v4().to_string(),
        namespace: "robot".into(),
        ..Default::default()
    }
    .with_environment(|key| match key {
        "GZ_DISCOVERY_MULTICAST_IP" => Some("239.255.0.8".into()),
        "GZ_DISCOVERY_MSG_PORT" => Some("12371".into()),
        "GZ_DISCOVERY_SRV_PORT" => Some("12372".into()),
        "GZ_TRANSPORT_SNDHWM" | "GZ_TRANSPORT_RCVHWM" => Some("16".into()),
        _ => None,
    })
    .unwrap();
    let node = Node::new(config.clone()).unwrap();
    let sub = node.subscribe("wire", TYPE).unwrap();
    let _peer = Peer::start(&config, "publish", "/robot/wire", "configured");
    let message = sub.recv_timeout(Duration::from_secs(4)).unwrap();
    assert_eq!(message.data, data("configured"));
    assert_eq!(message.topic, format!("@/{}@/robot/wire", config.partition));
}
#[test]
fn cpp_environment_integer_prefix_fallback_and_duplicate_port_differential() {
    let path = std::env::var("GZ_CPP_PEER")
        .unwrap_or_else(|_| format!("{}/artifacts/cpp-peer", env!("CARGO_MANIFEST_DIR")));
    for values in [
        [
            ("GZ_DISCOVERY_MSG_PORT", "12375tail"),
            ("GZ_DISCOVERY_SRV_PORT", "12376"),
            ("GZ_TRANSPORT_SNDHWM", "12tail"),
            ("GZ_TRANSPORT_RCVHWM", "-1"),
        ],
        [
            ("GZ_DISCOVERY_MSG_PORT", "65535"),
            ("GZ_DISCOVERY_SRV_PORT", "65535"),
            ("GZ_TRANSPORT_SNDHWM", "bad"),
            ("GZ_TRANSPORT_RCVHWM", "2147483648"),
        ],
    ] {
        let config = Config::default()
            .with_environment(|key| {
                values
                    .iter()
                    .find(|(k, _)| key == *k)
                    .map(|(_, v)| v.to_string())
            })
            .unwrap();
        let mut peer = Command::new(&path);
        peer.args(["config", "unused", "unused"])
            .env("GZ_PARTITION", "native-config")
            .env("GZ_IP", "127.0.0.1")
            .env("GZ_TRANSPORT_IMPLEMENTATION", "zeromq")
            .env("GZ_TRANSPORT_TOPIC_STATISTICS", "0");
        for (key, value) in values {
            peer.env(key, value);
        }
        let output = peer.output().unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            format!(
                "CONFIG native-config {} {} {} {}",
                config.message_port,
                config.service_port,
                config.send_high_watermark,
                config.receive_high_watermark
            )
        );
    }
}

#[test]
fn cpp_unknown_application_fields_are_received_without_reencoding() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let sub = node.subscribe("/unknown", TYPE).unwrap();
    let _peer = Peer::start(&config, "publish-unknown", "/unknown", "opaque");
    let mut expected = data("opaque");
    expected.extend([0xf8, 0x07, 0x01]);
    assert_eq!(
        sub.recv_timeout(Duration::from_secs(4)).unwrap().data,
        expected
    );
}
#[test]
fn rust_unknown_application_fields_reach_cpp_unchanged() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let publisher = node.advertise("/unknown", TYPE).unwrap();
    let mut peer = Peer::start(&config, "subscribe-unknown", "/unknown", "opaque");
    let mut bytes = data("opaque");
    bytes.extend([0xf8, 0x07, 0x01]);
    for _ in 0..100 {
        publisher.publish(bytes.clone()).unwrap();
        thread::sleep(Duration::from_millis(20));
        if peer.0.try_wait().unwrap().is_some() {
            break;
        }
    }
    peer.success();
}

#[test]
fn oneway_services_interoperate_without_reply_waiters() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let mut peer = Peer::start(&config, "serve-oneway", "/oneway_cpp", "rust");
    node.request_oneway("/oneway_cpp", TYPE, data("rust"), Duration::from_secs(5))
        .unwrap();
    peer.success(); // C++ executed the callback; it never replies for Empty.
    let service = node.serve("/oneway_rust", TYPE, "gz.msgs.Empty").unwrap();
    let mut peer = Peer::start(&config, "request-oneway", "/oneway_rust", "cpp");
    let request = service.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(request.data, data("cpp"));
    assert!(!request.expects_response());
    request.respond(Vec::new(), true).unwrap();
    peer.success();
    assert_eq!(node.statistics().reply_timeouts, 0);
}

#[test]
fn oneway_local_handoff_bounds_and_matching() {
    let config = Config {
        capacity: 2,
        max_message_bytes: 32,
        ..config()
    };
    let mut node = Node::new(config).unwrap();
    let service = node.serve("/oneway", TYPE, "gz.msgs.Empty").unwrap();
    for value in ["first", "second", "third"] {
        node.request_oneway("/oneway", TYPE, data(value), Duration::from_secs(1))
            .unwrap();
        let request = service.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(!request.expects_response());
        assert_eq!(request.data, data(value));
        request.respond(vec![0; 64], false).unwrap(); // Unused response data is ignored.
    }
    for _ in 0..2 {
        node.request_oneway("/oneway", TYPE, data("queued"), Duration::from_secs(1))
            .unwrap();
    }
    assert_eq!(
        node.request_oneway("/oneway", TYPE, data("full"), Duration::from_secs(1)),
        Err(Error::QueueFull)
    );
    assert_eq!(node.statistics().queue_drops, 1);
    assert_eq!(
        node.request_oneway("/oneway", "wrong", vec![], Duration::from_millis(25)),
        Err(Error::Timeout)
    );
    assert_eq!(
        node.request_oneway("invalid name", TYPE, vec![], Duration::from_secs(1)),
        Err(Error::InvalidName)
    );
    assert_eq!(
        node.request_oneway("/oneway", TYPE, vec![0; 33], Duration::from_secs(1)),
        Err(Error::InvalidFrame)
    );
    node.shutdown();
    assert_eq!(
        node.request_oneway("/oneway", TYPE, vec![], Duration::from_secs(1)),
        Err(Error::Shutdown)
    );
}

#[test]
fn native_host_service_accepts_other_loopback_aliases() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let peer_config = Config {
        interface: "127.0.0.2".parse().unwrap(),
        ..config
    };
    let _peer = Peer::start(&peer_config, "serve-host", "/host_echo", "-host");
    assert_eq!(
        node.request(
            "/host_echo",
            TYPE,
            TYPE,
            data("rust"),
            Duration::from_secs(5)
        ),
        Ok((data("rust-host"), true))
    );
}

#[test]
fn cpp_inspects_subscriber_only_topics_until_the_last_lease_is_dropped() {
    for (type_name, topic) in [
        ("gz.msgs.Int32", "/subscriber_only"),
        ("gz.msgs.Int32", "/unsub_test"),
        ("", "/wildcard_only"),
    ] {
        let config = config();
        let node = Node::new(config.clone()).unwrap();
        let wire_type = if type_name.is_empty() {
            "google.protobuf.Message"
        } else {
            type_name
        };
        let mut peer = Peer::start(&config, "inspect-subscriber", topic, wire_type);
        let first = node.subscribe(topic, type_name).unwrap();
        let last = node.subscribe(topic, type_name).unwrap();
        let mut line = String::new();
        peer.1.read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "FOUND");
        drop(first);
        writeln!(peer.0.stdin.as_mut().unwrap(), "KEEP").unwrap();
        line.clear();
        peer.1.read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "KEPT");
        drop(last);
        writeln!(peer.0.stdin.as_mut().unwrap(), "GONE").unwrap();
        peer.success();
    }
}

#[test]
fn original_oneway_int32_five_dispatches_once_and_rejects_invalid_service() {
    // Node_TEST.cc ServiceWithoutOutputCallAsync / srvWithoutOutput assertions.
    let node = Node::new(config()).unwrap();
    let type_name = "gz.msgs.Int32";
    assert!(matches!(
        node.serve("invalid service", type_name, "gz.msgs.Empty"),
        Err(Error::InvalidName)
    ));
    let service = node.serve("/foo", type_name, "gz.msgs.Empty").unwrap();
    let advertised = node.advertised_services().unwrap();
    assert_eq!(advertised.len(), 1);
    assert_eq!(advertised[0], "/foo");
    let request = gz_msgs::int32::Int32 {
        data: 5,
        ..Default::default()
    }
    .encode_to_vec();
    assert_eq!(
        node.request_oneway(
            "invalid service",
            type_name,
            request.clone(),
            Duration::from_secs(1)
        ),
        Err(Error::InvalidName)
    );
    node.request_oneway("/foo", type_name, request, Duration::from_secs(1))
        .unwrap();
    // A successful local handoff has queued the event, so draining now observes
    // actual delivered requests without a sleep or a fabricated callback count.
    let mut counter = 0;
    while let Ok(request) = service.recv_timeout(Duration::ZERO) {
        assert_eq!(
            gz_msgs::int32::Int32::decode(request.data.as_slice())
                .unwrap()
                .data,
            5
        );
        assert!(!request.expects_response());
        counter += 1;
    }
    assert_eq!(counter, 1);
    assert_eq!(
        node.unadvertise_service("invalid service"),
        Err(Error::InvalidName)
    );
    node.unadvertise_service("/foo").unwrap();
    assert!(node.advertised_services().unwrap().is_empty());
}

#[test]
fn advertised_service_topics_are_local_qualified_and_drop_aware() {
    let mut config = config();
    config.set_namespace("ns").unwrap();
    let node = Node::new(config.clone()).unwrap();
    let other = Node::new(config).unwrap();
    let _foreign = other.serve("/foreign", TYPE, TYPE).unwrap();
    let _independent = other.serve("relative", TYPE, TYPE).unwrap();
    let absolute = node.serve("/absolute", TYPE, TYPE).unwrap();
    let relative = node.serve("relative", TYPE, TYPE).unwrap();
    assert!(matches!(
        node.serve("relative", TYPE, TYPE),
        Err(Error::AlreadyAdvertised)
    ));
    assert!(matches!(
        node.serve("/ns/relative", "gz.msgs.Int32", "gz.msgs.Int32"),
        Err(Error::AlreadyAdvertised)
    ));
    assert_eq!(
        node.advertised_services().unwrap(),
        ["/absolute", "/ns/relative"]
    );
    node.unadvertise_service("/absent").unwrap();
    node.unadvertise_service("relative").unwrap();
    assert_eq!(node.advertised_services().unwrap(), ["/absolute"]);
    drop((relative, absolute));
    assert!(node.advertised_services().unwrap().is_empty());
    let replacement = node.serve("relative", TYPE, TYPE).unwrap();
    assert_eq!(node.advertised_services().unwrap(), ["/ns/relative"]);
    drop(replacement);
}

#[test]
fn native_local_service_enumeration_and_idempotent_withdrawal() {
    let config = config();
    let mut peer = Peer::start(&config, "service-lifecycle", "relative", "/relative");
    peer.success();
}

#[test]
fn cpp_requester_reaches_service_recreated_before_old_receiver_is_dropped() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let retired = node.serve("/recreated_service", TYPE, TYPE).unwrap();
    node.unadvertise_service("/recreated_service").unwrap();
    let replacement = node.serve("/recreated_service", TYPE, TYPE).unwrap();
    assert!(matches!(
        node.serve("/recreated_service", "gz.msgs.Int32", "gz.msgs.Int32"),
        Err(Error::AlreadyAdvertised)
    ));
    let mut peer = Peer::start(&config, "request", "/recreated_service", "cpp");
    let request = replacement.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(request.data, data("cpp"));
    request.respond(data("cpp-rust"), true).unwrap();
    peer.success();
    assert!(matches!(
        retired.recv_timeout(Duration::ZERO),
        Err(Error::Shutdown)
    ));
}

#[test]
fn native_as_valid_topic_preserves_scan_order_utf8_and_byte_boundaries() {
    let mut cases: Vec<(String, String)> = [
        ("/", ""),
        ("//", ""),
        ("///", ""),
        ("////", ""),
        ("a///b", "a/b"),
        ("a////b", "ab"),
        ("a/~/b", ""),
        ("a:~=b", ""),
        (":", ":"),
        (":=", ""),
        (":==", "="),
        ("a/", "a/"),
        ("a:", "a:"),
        ("é🦀 b", "é🦀_b"),
        ("a\u{a0}b", "a\u{a0}b"),
        ("a\tb\nc", "a\tb\nc"),
        ("a\0b", "a\0b"),
    ]
    .into_iter()
    .map(|(a, b)| (a.into(), b.into()))
    .collect();
    cases.extend([
        ("a".repeat(65535), "a".repeat(65535)),
        ("a".repeat(65536), String::new()),
        ("é".repeat(32767) + "a", "é".repeat(32767) + "a"),
        ("é".repeat(32768), String::new()),
        ("@".repeat(65536) + "a", "a".into()),
    ]);
    let peer = std::env::var("GZ_CPP_PEER")
        .unwrap_or_else(|_| format!("{}/artifacts/cpp-peer", env!("CARGO_MANIFEST_DIR")));
    for (input, expected) in cases {
        assert_eq!(gz_transport::as_valid_topic(&input), expected);
        let mut child = Command::new(&peer)
            .args(["valid-topic-bytes", "unused", "unused"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, expected.as_bytes());
    }
}

#[test]
fn native_peer_storage_confirms_address_and_channel_identity() {
    let peer = std::env::var("GZ_CPP_PEER")
        .unwrap_or_else(|_| format!("{}/artifacts/cpp-peer", env!("CARGO_MANIFEST_DIR")));
    let output = Command::new(peer)
        .args(["peer-storage", "/dual", TYPE])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"2 3 3 1 0\n");
}

#[test]
fn cpp_same_node_publishes_and_serves_the_same_topic() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let subscription = node.subscribe("/dual", TYPE).unwrap();
    let mut peer = Peer::start(&config, "dual", "/dual", "-dual");
    assert_eq!(
        subscription
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .data,
        data("-dual")
    );
    assert_eq!(
        node.request("/dual", TYPE, TYPE, data("rust"), Duration::from_secs(5))
            .unwrap(),
        (data("rust-dual"), true)
    );
    writeln!(peer.0.stdin.as_mut().unwrap(), "QUIT").unwrap();
    peer.success();
}

#[test]
fn cpp_subscriber_observes_immediately_recreated_publisher() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let mut peer = Peer::start(&config, "subscribe-recreate", "/recreated", "sample");
    let publisher = node.advertise("/recreated", TYPE).unwrap();
    thread::scope(|scope| {
        let (tx, rx) = std::sync::mpsc::sync_channel(2);
        let stdout = &mut peer.1;
        scope.spawn(move || {
            for expected in ["FIRST", "SECOND"] {
                let mut line = String::new();
                stdout.read_line(&mut line).unwrap();
                assert_eq!(line.trim(), expected);
                tx.send(expected).unwrap();
            }
        });
        let observed = |publisher: &gz_transport::Publisher, value: &str, stage| {
            let deadline = Instant::now() + Duration::from_secs(8);
            loop {
                publisher.publish(data(value)).unwrap();
                match rx.recv_timeout(Duration::from_millis(10)) {
                    Ok(actual) => {
                        assert_eq!(actual, stage);
                        break;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        assert!(Instant::now() < deadline)
                    }
                    Err(error) => panic!("native observation failed: {error}"),
                }
            }
        };
        observed(&publisher, "sample-before", "FIRST");
        assert!(matches!(
            node.advertise("/recreated", TYPE),
            Err(Error::AlreadyAdvertised)
        ));
        drop(publisher);
        let replacement = node.advertise("/recreated", TYPE).unwrap();
        observed(&replacement, "sample-after", "SECOND");
    });
    peer.success();
}

#[test]
fn cpp_and_rust_inspect_real_service_signatures_without_execution() {
    use gz_transport::{ServiceTypeResolution, ServiceTypes};
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let _provider = Peer::start(&config, "serve", "/native", "unused");
    let types = ServiceTypes {
        request_type: TYPE.into(),
        response_type: TYPE.into(),
    };
    assert_eq!(
        node.resolve_service_types("/native", "", "", Duration::from_secs(5)),
        Ok(ServiceTypeResolution::Resolved(types.clone()))
    );
    assert_eq!(node.service_type_signatures("/native").unwrap(), [types]);
    let service = node.serve("/rust", TYPE, TYPE).unwrap();
    let mut observer = Peer::start(&config, "inspect-service", "/rust", TYPE);
    observer.success();
    assert!(matches!(
        service.recv_timeout(Duration::from_millis(5)),
        Err(Error::Timeout)
    ));
}

// Node_TEST.cc assertion port: Copyright (C) 2014 Open Source Robotics Foundation.
#[test]
fn original_advertise_remapped_topic_twice() {
    let mut config = config();
    config.add_topic_remap("/remap_from", "/remap_to").unwrap();
    let node = Node::new(config).unwrap();
    let _first = node.advertise("/remap_from", "gz.msgs.Int32").unwrap();
    assert!(matches!(
        node.advertise("/remap_from", "gz.msgs.Int32"),
        Err(Error::AlreadyAdvertised)
    ));
}

#[test]
fn remapped_subscription_and_request_use_native_destination() {
    let mut config = config();
    config.add_topic_remap("/alias", "/wire").unwrap();
    let node = Node::new(config.clone()).unwrap();
    let subscription = node.subscribe("/alias", TYPE).unwrap();
    let mut peer = Peer::start(&config, "dual", "/wire", "-cpp");
    let message = subscription.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(message.topic, format!("@/{}@/wire", config.partition));
    assert_eq!(message.type_name, TYPE);
    assert_eq!(message.data, data("-cpp"));
    let (reply, success) = node
        .request("/alias", TYPE, TYPE, data("rust"), Duration::from_secs(5))
        .unwrap();
    assert!(success);
    assert_eq!(reply, data("rust-cpp"));
    writeln!(peer.0.stdin.as_mut().unwrap(), "QUIT").unwrap();
    peer.success();
}

#[test]
fn remapped_publication_and_service_reach_native_clients() {
    let mut config = config();
    config.add_topic_remap("/alias", "/wire").unwrap();
    let node = Node::new(config.clone()).unwrap();
    let publication = node.advertise("/alias", TYPE).unwrap();
    let service = node.serve("/alias", TYPE, TYPE).unwrap();
    let mut subscriber = Peer::start(&config, "subscribe", "/wire", "rust");
    let mut requester = Peer::start(&config, "request", "/wire", "client");
    let request = service.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(request.data, data("client"));
    request.respond(data("client-rust"), true).unwrap();
    requester.success();
    for _ in 0..100 {
        publication.publish(data("rust")).unwrap();
        if subscriber.0.try_wait().unwrap().is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    subscriber.success();
}

#[test]
fn remapping_inspection_and_explicit_withdrawal_retain_native_original_name() {
    use gz_transport::{ServiceTypeResolution, ServiceTypes};
    let mut config = config();
    let mut native = Peer::start(&config, "remap-service-lifecycle", "/from", "/to");
    native.success();
    config.add_topic_remap("/from", "/to").unwrap();
    config.add_topic_remap("/to", "/third").unwrap();
    let node = Node::new(config).unwrap();
    let service = node.serve("/from", TYPE, TYPE).unwrap();
    assert_eq!(node.advertised_services().unwrap(), ["/to"]);
    assert_eq!(
        node.resolve_service_types("/from", "", "", Duration::ZERO),
        Ok(ServiceTypeResolution::NoProviders)
    );
    assert_eq!(
        node.resolve_service_types("/to", "", "", Duration::ZERO),
        Ok(ServiceTypeResolution::Resolved(ServiceTypes {
            request_type: TYPE.into(),
            response_type: TYPE.into()
        }))
    );
    assert!(node.service_type_signatures("/from").unwrap().is_empty());
    assert!(matches!(
        service.recv_timeout(Duration::from_millis(5)),
        Err(Error::Timeout)
    ));
    node.unadvertise_service("/from").unwrap();
    assert_eq!(node.advertised_services().unwrap(), ["/to"]);
    assert!(matches!(
        node.serve("/from", TYPE, TYPE),
        Err(Error::AlreadyAdvertised)
    ));
    node.unadvertise_service("/to").unwrap();
    assert!(node.advertised_services().unwrap().is_empty());
    assert!(node.service_type_signatures("/to").unwrap().is_empty());
}

#[test]
fn publisher_membership_matches_native_reference_and_live_subscriber_withdrawal() {
    let config = config();
    let mut reference = Peer::start(&config, "publisher-membership", "/reference", TYPE);
    reference.success();
    let node = Node::new(config.clone()).unwrap();
    let publisher = node.advertise("/wire", TYPE).unwrap();
    assert!(!publisher.has_connections().unwrap());
    assert!(!publisher.connection_snapshot().unwrap());
    let wrong = node.subscribe("/wire", "wrong.type").unwrap();
    assert!(!publisher.has_connections().unwrap());
    assert!(!publisher.connection_snapshot().unwrap());
    let wildcard = node.subscribe("/wire", "").unwrap();
    assert!(publisher.connection_snapshot().unwrap());
    assert!(publisher.has_connections().unwrap());
    drop(wildcard);
    assert!(!publisher.connection_snapshot().unwrap());
    assert!(!publisher.has_connections().unwrap());
    let exact = node.subscribe("/wire", TYPE).unwrap();
    assert!(publisher.connection_snapshot().unwrap());
    assert!(publisher.has_connections().unwrap());
    drop(exact);
    assert!(!publisher.connection_snapshot().unwrap());
    assert!(!publisher.has_connections().unwrap());
    drop(wrong);
    let mut peer = Peer::start(&config, "subscriber-membership", "/wire", "value");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !publisher.has_connections().unwrap() {
        assert!(
            Instant::now() < deadline,
            "native registration did not arrive"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert!(publisher.connection_snapshot().unwrap());
    publisher.publish(data("value")).unwrap();
    let mut line = String::new();
    peer.1.read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "RECEIVED");
    writeln!(peer.0.stdin.as_mut().unwrap(), "DROP").unwrap();
    line.clear();
    peer.1.read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "DROPPED");
    let deadline = Instant::now() + Duration::from_secs(10);
    while publisher.has_connections().unwrap() {
        assert!(
            Instant::now() < deadline,
            "native END registration did not arrive"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert!(!publisher.connection_snapshot().unwrap());
    writeln!(peer.0.stdin.as_mut().unwrap(), "QUIT").unwrap();
    peer.success();
    node.cancel();
    assert_eq!(publisher.connection_snapshot(), Err(Error::Shutdown));
    assert_eq!(publisher.has_connections(), Err(Error::Shutdown));
}

#[test]
fn publisher_snapshot_tracks_duplicate_local_leases_and_registered_remap() {
    let mut config = config();
    config.add_topic_remap("/alias", "/wire").unwrap();
    let node = Node::new(config).unwrap();
    let first = node.subscribe("/alias", TYPE).unwrap();
    let publisher = node.advertise("/alias", TYPE).unwrap();
    assert!(publisher.connection_snapshot().unwrap());
    let second = node.subscribe("/wire", TYPE).unwrap();
    assert!(publisher.connection_snapshot().unwrap());
    drop(first);
    assert!(publisher.connection_snapshot().unwrap());
    drop(second);
    assert!(!publisher.connection_snapshot().unwrap());
    let wildcard = node.subscribe("/wire", "").unwrap();
    assert!(publisher.connection_snapshot().unwrap());
    node.unsubscribe("/wire").unwrap();
    assert!(!publisher.connection_snapshot().unwrap());
    drop(wildcard);
    assert!(!publisher.connection_snapshot().unwrap());
}

// Node_TEST.cc CreatePubSubTwoThreads/CreateSubscriber/cb assertions:
// Copyright (C) 2014 Open Source Robotics Foundation.
fn original_threaded_publication(remapping: bool) {
    scoped_threaded_publication(remapping, None);
}
fn scoped_threaded_publication(remapping: bool, scope_option: Option<Scope>) {
    let mut config = config();
    if remapping {
        config.add_topic_remap("/foo", "/bar").unwrap();
    }
    let node = Node::new(config.clone()).unwrap();
    let publication = if let Some(scope_option) = scope_option {
        let mut options = AdvertiseOptions::default();
        assert_eq!(options.scope(), Scope::All);
        options.set_scope(scope_option);
        assert_eq!(options.scope(), scope_option);
        node.advertise_with_options("/foo", "gz.msgs.Int32", options)
    } else {
        node.advertise("/foo", "gz.msgs.Int32")
    }
    .unwrap();
    let executed = std::sync::atomic::AtomicBool::new(false);
    thread::scope(|scope| {
        let executed = &executed;
        let subscriber = scope.spawn(move || {
            let node = Node::new(config).unwrap();
            let subscription = node.subscribe("/foo", "gz.msgs.Int32").unwrap();
            let message = subscription.recv_timeout(Duration::from_secs(10)).unwrap();
            assert_eq!(
                gz_msgs::int32::Int32::decode(message.data.as_slice())
                    .unwrap()
                    .data,
                5
            );
            executed.store(true, std::sync::atomic::Ordering::Release);
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        while !publication.has_connections().unwrap() {
            assert!(
                Instant::now() < deadline,
                "subscriber registration did not arrive"
            );
            thread::sleep(Duration::from_millis(10));
        }
        publication
            .publish(
                gz_msgs::int32::Int32 {
                    data: 5,
                    ..Default::default()
                }
                .encode_to_vec(),
            )
            .unwrap();
        subscriber.join().unwrap();
    });
    assert!(executed.load(std::sync::atomic::Ordering::Acquire));
}
#[test]
fn original_pub_sub_two_threads_same_topic() {
    original_threaded_publication(false);
}
#[test]
fn original_pub_sub_two_threads_same_topic_remap() {
    original_threaded_publication(true);
}
#[test]
fn original_scope_all() {
    original_threaded_publication(false);
}
#[test]
fn original_scope_all_remap() {
    original_threaded_publication(true);
}

#[test]
fn original_scope_process() {
    scoped_threaded_publication(false, Some(Scope::Process));
}
#[test]
fn original_scope_process_remap() {
    scoped_threaded_publication(true, Some(Scope::Process));
}
#[test]
fn original_scope_host() {
    scoped_threaded_publication(false, Some(Scope::Host));
}
#[test]
fn original_scope_host_remap() {
    scoped_threaded_publication(true, Some(Scope::Host));
}

#[test]
fn publication_scope_native_discovery_and_delivery() {
    for scope in [Scope::Process, Scope::Host, Scope::All] {
        let config = config();
        let node = Node::new(config.clone()).unwrap();
        let local_node = Node::new(config.clone()).unwrap();
        let local = local_node.subscribe("/scope", TYPE).unwrap();
        let mut options = AdvertiseOptions::default();
        options.set_scope(scope);
        let publisher = node
            .advertise_typed_with_options::<StringMsg>("/scope", options)
            .unwrap();
        let mut peer = Peer::start(&config, "scope-observer", "/scope", "scope");
        // Cover multiple native heartbeat cycles before checking Process absence.
        thread::sleep(Duration::from_millis(2200));
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            publisher
                .publish_typed(&StringMsg {
                    data: "scope".into(),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(
                StringMsg::decode(
                    local
                        .recv_timeout(Duration::from_secs(1))
                        .unwrap()
                        .data
                        .as_slice()
                )
                .unwrap()
                .data,
                "scope"
            );
            writeln!(peer.0.stdin.as_mut().unwrap(), "STATE").unwrap();
            let mut line = String::new();
            peer.1.read_line(&mut line).unwrap();
            let fields: Vec<_> = line.split_whitespace().collect();
            assert_eq!(fields.len(), 5, "{line}");
            assert_eq!(fields[0], "STATE");
            assert_eq!(fields[2], "1", "native payload mismatch: {line}");
            let received: usize = fields[1].parse().unwrap();
            let publishers: usize = fields[3].parse().unwrap();
            if scope == Scope::Process {
                assert_eq!(received, 0, "{line}");
                assert_eq!(publishers, 0, "{line}");
                break;
            }
            if received > 0 && publishers > 0 {
                assert_eq!(fields[4].parse::<i32>().unwrap(), i32::from(scope));
                break;
            }
            assert!(
                Instant::now() < deadline,
                "native peer did not discover/receive scoped publication"
            );
            thread::sleep(Duration::from_millis(10));
        }
        writeln!(peer.0.stdin.as_mut().unwrap(), "QUIT").unwrap();
        peer.success();
        if scope == Scope::Process {
            assert!(publisher.connection_snapshot().unwrap());
            drop(local);
            let deadline = Instant::now() + Duration::from_secs(2);
            while publisher.connection_snapshot().unwrap() {
                assert!(
                    Instant::now() < deadline,
                    "Process readiness was not retired"
                );
                thread::sleep(Duration::from_millis(10));
            }
            assert!(!publisher.has_connections().unwrap());
        }
    }
}

#[test]
fn process_scope_readiness_matches_live_domain_topic_and_type() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let mut options = AdvertiseOptions::default();
    options.set_scope(Scope::Process);
    let publisher = node
        .advertise_with_options("/scope", TYPE, options)
        .unwrap();
    let wait = |expected| {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if publisher.has_connections().unwrap() == expected
                && publisher.connection_snapshot().unwrap() == expected
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Process interest did not become {expected}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    };
    wait(false);
    let mut foreign_config = config.clone();
    foreign_config.partition.push_str("-foreign");
    let foreign_node = Node::new(foreign_config).unwrap();
    let _foreign = foreign_node.subscribe("/scope", TYPE).unwrap();
    let other = Node::new(config.clone()).unwrap();
    let _wrong_type = other.subscribe("/scope", "gz.msgs.Vector3d").unwrap();
    let _wrong_topic = other.subscribe("/different", TYPE).unwrap();
    wait(false);
    let wildcard = other.subscribe("/scope", "").unwrap();
    wait(true);
    drop(wildcard);
    wait(false);
    let matching_node = Node::new(config).unwrap();
    let _matching = matching_node.subscribe("/scope", TYPE).unwrap();
    wait(true);
    matching_node.cancel();
    wait(false);
    let own = node.subscribe("/scope", TYPE).unwrap();
    wait(true);
    drop(own);
    wait(false);
}

#[test]
fn native_peer_receives_throttled_typed_and_raw_and_actual_rate_metadata() {
    for (raw, rate) in [false, true].into_iter().flat_map(|raw| {
        [
            0,
            1,
            3,
            10,
            1_000_000_000,
            1_000_000_001,
            u64::MAX - 1,
            u64::MAX,
        ]
        .into_iter()
        .map(move |rate| (raw, rate))
    }) {
        let config = config();
        let node = Node::new(config.clone()).unwrap();
        let mut options = AdvertiseOptions::default();
        options.set_msgs_per_sec(rate);
        let publisher = node
            .advertise_typed_with_options::<StringMsg>("/throttle", options)
            .unwrap();
        let mut native = Peer::start(&config, "throttle-observer", "/throttle", "value");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !publisher.has_connections().unwrap() {
            assert!(Instant::now() < deadline, "native interest did not arrive");
            thread::sleep(Duration::from_millis(10));
        }
        let state = |native: &mut Peer| {
            writeln!(native.0.stdin.as_mut().unwrap(), "STATE").unwrap();
            let mut line = String::new();
            native.1.read_line(&mut line).unwrap();
            let fields: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
            assert_eq!(fields.len(), 7, "{line}");
            assert_eq!(fields[0], "STATE");
            assert_eq!(fields[2], "1", "native callback payload: {line}");
            assert_eq!(fields[3], "1", "native discovered provider: {line}");
            assert_eq!(fields[4], "2", "native discovered All scope: {line}");
            assert_eq!(
                fields[5].parse::<u64>().unwrap(),
                rate,
                "native discovered rate: {line}"
            );
            assert_eq!(
                fields[6],
                if rate == u64::MAX { "0" } else { "1" },
                "native discovered throttle flag: {line}"
            );
            fields[1].parse::<usize>().unwrap()
        };
        assert_eq!(state(&mut native), 0);
        let value = StringMsg {
            data: "value".into(),
            ..Default::default()
        };
        assert_eq!(publisher.throttled_update_ready().unwrap(), rate != 0);
        for _ in 0..3 {
            if raw {
                publisher.publish(value.encode_to_vec()).unwrap();
            } else {
                publisher.publish_typed(&value).unwrap();
            }
            if rate <= 3 {
                assert!(!publisher.throttled_update_ready().unwrap());
            }
            thread::sleep(Duration::from_millis(100));
        }
        let expected = match rate {
            0 => 0,
            1 | 3 => 1,
            _ => 3,
        };
        assert_eq!(state(&mut native), expected);
        eprintln!("rate-native-case raw={raw} rate={rate} count={expected}");
        writeln!(native.0.stdin.as_mut().unwrap(), "QUIT").unwrap();
        native.success();
    }
}

#[test]
fn graceful_subscriber_shutdown_with_live_lease_withdraws_membership() {
    let config = config();
    let publisher_node = Node::new(config.clone()).unwrap();
    let publisher = publisher_node.advertise("/wire", TYPE).unwrap();
    let mut subscriber_node = Node::new(config).unwrap();
    let lease = subscriber_node.subscribe("/wire", TYPE).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !publisher.has_connections().unwrap() {
        assert!(Instant::now() < deadline, "registration did not arrive");
        thread::sleep(Duration::from_millis(10));
    }
    subscriber_node.shutdown();
    assert_eq!(lease.recv_timeout(Duration::ZERO), Err(Error::Shutdown));
    let deadline = Instant::now() + Duration::from_secs(10);
    while publisher.has_connections().unwrap() {
        assert!(Instant::now() < deadline, "graceful shutdown omitted END");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn local_inventory_matches_native_namespaced_alias_lifetimes() {
    let mut config = config();
    let mut native = Peer::start(&config, "local-inventory", "alias", "target");
    native.success();
    config.set_namespace("ns").unwrap();
    config.add_topic_remap("alias", "target").unwrap();
    let node = Node::new(config).unwrap();
    let publication = node.advertise("alias", TYPE).unwrap();
    let first = node.subscribe("alias", TYPE).unwrap();
    let second = node.subscribe("/ns/target", TYPE).unwrap();
    assert_eq!(node.advertised_topics().unwrap(), ["/ns/target"]);
    assert_eq!(node.subscribed_topics().unwrap(), ["/ns/target"]);
    drop(first);
    assert_eq!(node.subscribed_topics().unwrap(), ["/ns/target"]);
    drop(second);
    assert!(node.subscribed_topics().unwrap().is_empty());
    drop(publication);
    assert!(node.advertised_topics().unwrap().is_empty());
    let orphan = node.subscribe("/orphan", TYPE).unwrap();
    assert_eq!(node.subscribed_topics().unwrap(), ["/orphan"]);
    drop(orphan);
    assert!(node.subscribed_topics().unwrap().is_empty());
}

/*
 * Copyright (C) 2014 Open Source Robotics Foundation
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
// Original Node_TEST.cc ordinary/generic callback assertions.
fn received_int32_five(
    subscription: &gz_transport::Subscription,
    executed: &mut bool,
    counter: &mut usize,
) {
    let message = subscription.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(
        gz_msgs::int32::Int32::decode(message.data.as_slice())
            .unwrap()
            .data,
        5
    );
    *executed = true;
    *counter += 1;
}
fn int32_five() -> Vec<u8> {
    gz_msgs::int32::Int32 {
        data: 5,
        ..Default::default()
    }
    .encode_to_vec()
}
fn await_publication_membership(publication: &gz_transport::Publisher) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !publication.has_connections().unwrap() {
        assert!(Instant::now() < deadline, "subscriber did not register");
        thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn original_pub_without_advertise() {
    let mut config = config();
    assert_eq!(
        config.set_namespace("invalid namespace"),
        Err(Error::InvalidName)
    );
    assert!(config.namespace.is_empty());
    let first = Node::new(config.clone()).unwrap();
    let second = Node::new(config).unwrap();
    assert!(first.advertised_topics().unwrap().is_empty());
    assert!(first.subscribed_topics().unwrap().is_empty());
    assert!(first.advertised_services().unwrap().is_empty());
    let publication1 = first.advertise("/foo", "gz.msgs.Int32").unwrap();
    assert_eq!(first.advertised_topics().unwrap(), ["/foo"]);
    let publication2 = second.advertise("/foo", "gz.msgs.Int32").unwrap();
    assert_eq!(second.advertised_topics().unwrap(), ["/foo"]);
    let subscription = second.subscribe("/foo", "gz.msgs.Int32").unwrap();
    assert_eq!(second.subscribed_topics().unwrap(), ["/foo"]);
    await_publication_membership(&publication1);
    let (mut executed, mut counter) = (false, 0);
    publication1.publish(int32_five()).unwrap();
    received_int32_five(&subscription, &mut executed, &mut counter);
    publication2.publish(int32_five()).unwrap();
    received_int32_five(&subscription, &mut executed, &mut counter);
    assert!(executed);
    assert_eq!(counter, 2);
    second.service_type_signatures("/barrier").unwrap();
    assert_eq!(
        subscription.recv_timeout(Duration::ZERO),
        Err(Error::Timeout)
    );
}
#[test]
fn original_pub_sub_same_thread() {
    let node = Node::new(config()).unwrap();
    assert!(matches!(
        node.advertise("invalid topic", "gz.msgs.Int32"),
        Err(Error::InvalidName)
    ));
    let publication = node.advertise("/foo", "gz.msgs.Int32").unwrap();
    assert!(matches!(
        node.subscribe("invalid topic", "gz.msgs.Int32"),
        Err(Error::InvalidName)
    ));
    let subscription = node.subscribe("/foo", "gz.msgs.Int32").unwrap();
    await_publication_membership(&publication);
    for _ in 0..2 {
        let (mut executed, mut counter) = (false, 0);
        publication.publish(int32_five()).unwrap();
        received_int32_five(&subscription, &mut executed, &mut counter);
        assert!(executed);
        assert_eq!(counter, 1);
    }
}
// Pinned Node_TEST.cc:1336-1405. The first publication must reach both Nodes
// without a warmup message or a second-Node network-readiness sleep.
#[test]
fn original_pub_sub_one_thread_two_subs() {
    let config = config();
    let node1 = Node::new(config.clone()).unwrap();
    let node2 = Node::new(config).unwrap();
    let publication = node1.advertise("/foo", "gz.msgs.Int32").unwrap();
    let first = node1.subscribe("/foo", "gz.msgs.Int32").unwrap();
    let second = node2.subscribe("/foo", "gz.msgs.Int32").unwrap();
    await_publication_membership(&publication);

    publication.publish(int32_five()).unwrap();
    assert_eq!(
        first.recv_timeout(Duration::from_secs(10)).unwrap().data,
        int32_five()
    );
    assert_eq!(
        second.recv_timeout(Duration::from_secs(10)).unwrap().data,
        int32_five()
    );
    assert_eq!(node1.subscribed_topics().unwrap(), ["/foo"]);

    assert_eq!(node1.unsubscribe("invalid topic"), Err(Error::InvalidName));
    node1.unsubscribe("/foo").unwrap();
    thread::sleep(Duration::from_millis(500));
    publication.publish(int32_five()).unwrap();
    assert_eq!(
        second.recv_timeout(Duration::from_secs(10)).unwrap().data,
        int32_five()
    );
    assert_eq!(first.recv_timeout(Duration::ZERO), Err(Error::Shutdown));
    assert!(node1.subscribed_topics().unwrap().is_empty());
    drop(publication);
    assert!(node1.advertised_topics().unwrap().is_empty());
}
#[test]
fn process_local_direct_delivery_respects_domain_ignore_and_owner_retirement() {
    let config = config();
    let publisher_node = Node::new(config.clone()).unwrap();
    let second_node = Node::new(config.clone()).unwrap();
    let mut other_partition = config.clone();
    other_partition.partition.push_str("-foreign");
    let foreign_node = Node::new(other_partition).unwrap();
    let publication = publisher_node.advertise("/foo", "gz.msgs.Int32").unwrap();
    let mut options = SubscribeOptions::default();
    options.set_ignore_local_messages(true);
    let ignored = publisher_node
        .subscribe_with_options("/foo", "gz.msgs.Int32", options)
        .unwrap();
    let remote = second_node.subscribe("/foo", "gz.msgs.Int32").unwrap();
    let wildcard = second_node.subscribe("/foo", "").unwrap();
    let foreign = foreign_node.subscribe("/foo", "gz.msgs.Int32").unwrap();

    publication.publish(int32_five()).unwrap();
    let received = remote
        .recv_with_info_timeout(Duration::from_secs(10))
        .unwrap();
    assert_eq!(received.data, int32_five());
    assert!(received.info().intra_process);
    assert_eq!(
        wildcard.recv_timeout(Duration::from_secs(10)).unwrap().data,
        int32_five()
    );
    thread::sleep(Duration::from_millis(500));
    assert_eq!(remote.recv_timeout(Duration::ZERO), Err(Error::Timeout));
    assert_eq!(wildcard.recv_timeout(Duration::ZERO), Err(Error::Timeout));
    assert_eq!(ignored.recv_timeout(Duration::ZERO), Err(Error::Timeout));
    assert_eq!(foreign.recv_timeout(Duration::ZERO), Err(Error::Timeout));

    drop(second_node);
    publication.publish(int32_five()).unwrap();
    assert_eq!(remote.recv_timeout(Duration::ZERO), Err(Error::Shutdown));
    assert_eq!(wildcard.recv_timeout(Duration::ZERO), Err(Error::Shutdown));
    assert_eq!(ignored.recv_timeout(Duration::ZERO), Err(Error::Timeout));
}

#[test]
fn process_local_final_target_keeps_fanout_matching_and_queue_drop_counts() {
    let mut domain = config();
    domain.capacity = 2;
    let publisher_node = Node::new(domain.clone()).unwrap();
    let first_node = Node::new(domain.clone()).unwrap();
    let second_node = Node::new(domain.clone()).unwrap();
    let mut foreign = domain.clone();
    foreign.partition.push_str("-foreign");
    let foreign_node = Node::new(foreign).unwrap();
    let publication = publisher_node
        .advertise("/fanout", "gz.msgs.Int32")
        .unwrap();
    let mut ignored_options = SubscribeOptions::default();
    ignored_options.set_ignore_local_messages(true);
    let ignored = publisher_node
        .subscribe_with_options("/fanout", "gz.msgs.Int32", ignored_options)
        .unwrap();
    let first = first_node.subscribe("/fanout", "gz.msgs.Int32").unwrap();
    let last = second_node.subscribe("/fanout", "").unwrap();
    let mismatch = second_node.subscribe("/fanout", TYPE).unwrap();
    let isolated = foreign_node.subscribe("/fanout", "gz.msgs.Int32").unwrap();
    let worker_barrier = || {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match publication.has_connections() {
                Ok(_) => break,
                Err(Error::QueueFull) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(1));
                }
                result => panic!("worker barrier failed: {result:?}"),
            }
        }
    };

    for _ in 0..2 {
        publication.publish(int32_five()).unwrap();
        worker_barrier();
    }
    let drops_before = publisher_node.statistics().queue_drops;
    publication.publish(int32_five()).unwrap();
    worker_barrier();
    assert_eq!(publisher_node.statistics().queue_drops, drops_before + 2);
    for receiver in [&first, &last] {
        for _ in 0..2 {
            let message = receiver
                .recv_with_info_timeout(Duration::from_secs(10))
                .unwrap();
            assert_eq!(message.data, int32_five());
            assert!(message.info().intra_process);
        }
        assert_eq!(receiver.recv_timeout(Duration::ZERO), Err(Error::Timeout));
    }
    thread::sleep(Duration::from_millis(500));
    assert_eq!(ignored.recv_timeout(Duration::ZERO), Err(Error::Timeout));
    assert_eq!(mismatch.recv_timeout(Duration::ZERO), Err(Error::Timeout));
    assert_eq!(isolated.recv_timeout(Duration::ZERO), Err(Error::Timeout));
}

#[test]
fn process_local_direct_delivery_suppresses_connected_wire_echo() {
    let config = config();
    let publisher_node = Node::new(config.clone()).unwrap();
    let subscriber_node = Node::new(config).unwrap();
    let publication = publisher_node
        .advertise("/private_echo", "gz.msgs.Int32")
        .unwrap();
    let subscription = subscriber_node
        .subscribe("/private_echo", "gz.msgs.Int32")
        .unwrap();

    // Give the second Node's wire subscription time to connect so this
    // supplemental control exercises both the direct and wire paths.
    thread::sleep(Duration::from_millis(1500));
    publication.publish(int32_five()).unwrap();
    let received = subscription
        .recv_with_info_timeout(Duration::from_secs(10))
        .unwrap();
    assert_eq!(received.data, int32_five());
    assert!(received.info().intra_process);
    assert_eq!(
        subscription.recv_timeout(Duration::from_millis(500)),
        Err(Error::Timeout)
    );
}

#[test]
fn process_local_new_node_subscription_invalidates_empty_target_cache() {
    let config = config();
    let publisher_node = Node::new(config.clone()).unwrap();
    let publisher = publisher_node
        .advertise("/new_interest", "gz.msgs.Int32")
        .unwrap();
    let same_node = publisher_node
        .subscribe("/new_interest", "gz.msgs.Int32")
        .unwrap();
    publisher.publish(int32_five()).unwrap();
    assert!(
        same_node
            .recv_with_info_timeout(Duration::from_secs(10))
            .unwrap()
            .info()
            .intra_process
    );

    let second_node = Node::new(config).unwrap();
    let second = second_node
        .subscribe("/new_interest", "gz.msgs.Int32")
        .unwrap();
    publisher.publish(int32_five()).unwrap();
    let received = second
        .recv_with_info_timeout(Duration::from_secs(10))
        .unwrap();
    assert_eq!(received.data, int32_five());
    assert!(received.info().intra_process);
}
#[test]
fn original_pub_sub_same_thread_generic_cb() {
    let node = Node::new(config()).unwrap();
    let publication = node.advertise("/foo", "gz.msgs.Int32").unwrap();
    let subscription = node.subscribe("/foo", "").unwrap();
    await_publication_membership(&publication);
    for _ in 0..2 {
        let mut executed = false;
        publication.publish(int32_five()).unwrap();
        received_generic_five(&subscription, &mut executed);
        assert!(executed);
    }
}

fn received_generic_five(subscription: &gz_transport::Subscription, executed: &mut bool) {
    let message = subscription.recv_timeout(Duration::from_secs(10)).unwrap();
    let descriptor = gz_msgs::descriptor(&message.type_name).unwrap();
    let dynamic =
        gz_msgs::prost_reflect::DynamicMessage::decode(descriptor, message.data.as_slice())
            .unwrap();
    let content = dynamic.to_text_format();
    assert!(content.contains(&5.to_string()));
    *executed = true;
    assert!(!message.topic_path().contains('@'));
    assert_eq!(message.topic_path(), "/foo");
}

// Original twoProcsSrvCall{,WithoutInput,WithoutOutput}.cc ServiceInfo assertions.
/*
 * Copyright (C) 2016 Open Source Robotics Foundation
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 *
*/
// Ordinary fixture retains the original 2014 notice above.
fn original_service_info_fixture(mode: &str, request_type: &str, response_type: Option<&str>) {
    let config = config();
    let mut service = Peer::start(&config, "service-info-reference", "/foo", mode);
    let node = Node::new(config).unwrap();
    // Native waitForService uses actual metadata, 10000ms and 100ms polling.
    let deadline = Instant::now() + Duration::from_secs(10);
    while node.service_provider_types("/foo").unwrap().is_empty() {
        assert!(
            Instant::now() < deadline,
            "native service was not discovered"
        );
        thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(node.service_provider_types("@"), Err(Error::InvalidName));
    assert!(node.service_provider_types("/bogus").unwrap().is_empty());
    let signatures = node.service_provider_types("/foo").unwrap();
    assert_eq!(signatures.len(), 1);
    assert_eq!(signatures[0].request_type, request_type);
    if let Some(response_type) = response_type {
        assert_eq!(signatures[0].response_type, response_type);
    }
    writeln!(service.0.stdin.as_mut().unwrap(), "QUIT").unwrap();
    service.success();
}
#[test]
fn original_two_process_service_info() {
    original_service_info_fixture("ordinary", "gz.msgs.Int32", Some("gz.msgs.Int32"));
}
#[test]
fn original_two_process_inputless_service_info() {
    original_service_info_fixture("inputless", "gz.msgs.Empty", Some("gz.msgs.Int32"));
}
#[test]
fn original_two_process_outputless_service_info() {
    original_service_info_fixture("outputless", "gz.msgs.Int32", None);
}

#[test]
fn provider_record_multiplicity_matches_native_for_equal_signatures() {
    let config = config();
    let mut first = Peer::start(&config, "service-info-reference", "/foo", "ordinary");
    let mut second = Peer::start(&config, "service-info-reference", "/foo", "ordinary");
    let mut native = Peer::start(&config, "service-info-record-count", "/foo", "2");
    native.success();
    let node = Node::new(config).unwrap();
    let expected = gz_transport::ServiceTypes {
        request_type: "gz.msgs.Int32".into(),
        response_type: "gz.msgs.Int32".into(),
    };
    let await_count = |count| {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let records = node.service_provider_types("/foo").unwrap();
            if records.len() == count {
                assert!(records.iter().all(|types| types == &expected));
                break;
            }
            assert!(
                Instant::now() < deadline,
                "provider record count did not reach {count}: {records:?}"
            );
            thread::sleep(Duration::from_millis(100));
        }
    };
    await_count(2);
    assert_eq!(
        node.service_type_signatures("/foo").unwrap(),
        std::slice::from_ref(&expected)
    );
    writeln!(first.0.stdin.as_mut().unwrap(), "QUIT").unwrap();
    first.success();
    await_count(1);
    assert_eq!(
        node.service_type_signatures("/foo").unwrap(),
        std::slice::from_ref(&expected)
    );
    writeln!(second.0.stdin.as_mut().unwrap(), "QUIT").unwrap();
    second.success();
    await_count(0);
    assert!(node.service_type_signatures("/foo").unwrap().is_empty());
}

#[test]
fn explicit_unsubscribe_matches_native_original_name_and_node_ownership() {
    let mut config = config();
    config.add_topic_remap("/alias", "/canonical").unwrap();
    let mut reference = Peer::start(&config, "unsubscribe-reference", "/alias", "/canonical");
    reference.success();
    let first = Node::new(config.clone()).unwrap();
    let second = Node::new(config).unwrap();
    let receivers = [
        first.subscribe("/alias", "gz.msgs.Int32").unwrap(),
        first.subscribe("/alias", "gz.msgs.Int32").unwrap(),
        second.subscribe("/alias", "gz.msgs.Int32").unwrap(),
    ];
    assert_eq!(first.subscribed_topics().unwrap(), ["/canonical"]);
    assert_eq!(second.subscribed_topics().unwrap(), ["/canonical"]);
    assert_eq!(first.unsubscribe("invalid topic"), Err(Error::InvalidName));
    first.unsubscribe("/absent").unwrap();
    assert_eq!(first.subscribed_topics().unwrap(), ["/canonical"]);
    first.unsubscribe("/alias").unwrap();
    assert_eq!(first.subscribed_topics().unwrap(), ["/canonical"]);
    first.unsubscribe("/canonical").unwrap();
    assert!(first.subscribed_topics().unwrap().is_empty());
    assert_eq!(second.subscribed_topics().unwrap(), ["/canonical"]);
    first.unsubscribe("/canonical").unwrap();
    let replacement = first.subscribe("/alias", "gz.msgs.Int32").unwrap();
    assert_eq!(first.subscribed_topics().unwrap(), ["/canonical"]);
    first.unsubscribe("/canonical").unwrap();
    second.unsubscribe("/canonical").unwrap();
    assert!(first.subscribed_topics().unwrap().is_empty());
    assert!(second.subscribed_topics().unwrap().is_empty());
    drop((receivers, replacement));
}

#[test]
fn typed_publication_reaches_actual_cpp_subscriber() {
    let config = config();
    let mut subscriber = Peer::start(&config, "subscribe", "/typed", "typed value");
    let node = Node::new(config).unwrap();
    let publisher = node.advertise_typed::<StringMsg>("/typed").unwrap();
    let value = StringMsg {
        data: "typed value".into(),
        ..Default::default()
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while subscriber.0.try_wait().unwrap().is_none() {
        assert!(
            Instant::now() < deadline,
            "native typed subscriber did not complete"
        );
        publisher.publish_typed(&value).unwrap();
        thread::sleep(Duration::from_millis(20));
    }
    subscriber.success();
}

// Node_TEST.cc:PubSubSameThreadMessageInfo and cbInfo value observations.
// The complete original 2014 Apache/OSRF notice is retained above.
fn received_five_with_info(
    subscription: &gz_transport::Subscription,
    partition: &str,
    executed: &mut bool,
    counter: &mut usize,
) {
    let message = subscription
        .recv_with_info_timeout(Duration::from_secs(10))
        .unwrap();
    let info = message.info();
    assert_eq!(info.topic, "/foo");
    assert_eq!(
        gz_msgs::int32::Int32::decode(message.data.as_slice())
            .unwrap()
            .data,
        5
    );
    assert_eq!(info.partition, format!("/{partition}"));
    assert_eq!(
        info.type_name,
        <gz_msgs::int32::Int32 as gz_msgs::CanonicalMessage>::PROTOBUF_NAME
    );
    assert!(info.intra_process);
    *executed = true;
    *counter += 1;
}
#[test]
fn original_pub_sub_same_thread_message_info() {
    let config = config();
    let partition = config.partition.clone();
    let node = Node::new(config).unwrap();
    let publisher = node
        .advertise_typed::<gz_msgs::int32::Int32>("/foo")
        .unwrap();
    let subscription = node.subscribe("/foo", "gz.msgs.Int32").unwrap();
    await_publication_membership(&publisher);
    let (mut executed, mut counter) = (false, 0);
    for _ in 0..2 {
        assert!(!executed);
        assert_eq!(counter, 0);
        publisher.publish(int32_five()).unwrap();
        received_five_with_info(&subscription, &partition, &mut executed, &mut counter);
        assert!(executed);
        assert_eq!(counter, 1);
        executed = false;
        counter = 0;
    }
}

// Node_TEST.cc raw metadata cases share the same two single-publication sequence.
// SubscribeRaw's default generic descriptor maps to an empty Rust type filter;
// rawCbInfo's pointer/size parse maps to decoding the complete received byte slice.
fn raw_message_info_original(raw_publication: bool, raw_subscription: bool) {
    let config = config();
    let partition = config.partition.clone();
    let node = Node::new(config).unwrap();
    let publisher = node
        .advertise_typed::<gz_msgs::int32::Int32>("/foo")
        .unwrap();
    let type_name = if raw_subscription {
        ""
    } else {
        "gz.msgs.Int32"
    };
    let subscription = node.subscribe("/foo", type_name).unwrap();
    await_publication_membership(&publisher);
    let value = gz_msgs::int32::Int32 {
        data: 5,
        ..Default::default()
    };
    let (mut executed, mut counter) = (false, 0);
    for _ in 0..2 {
        assert!(!executed);
        assert_eq!(counter, 0);
        if raw_publication {
            publisher.publish(value.encode_to_vec()).unwrap();
        } else {
            publisher.publish_typed(&value).unwrap();
        }
        received_five_with_info(&subscription, &partition, &mut executed, &mut counter);
        assert!(executed);
        assert_eq!(counter, 1);
        executed = false;
        counter = 0;
    }
}

#[test]
fn original_raw_pub_sub_same_thread_message_info() {
    raw_message_info_original(true, false);
}

#[test]
fn original_raw_pub_raw_sub_same_thread_message_info() {
    raw_message_info_original(true, true);
}

#[test]
fn original_pub_raw_sub_same_thread_message_info() {
    raw_message_info_original(false, true);
}

#[test]
fn same_process_two_node_origin_matches_native_direct_delivery() {
    let config = config();
    let mut native = Peer::start(&config, "message-info-two-nodes", "/foo", "5");
    let mut origin = String::new();
    native.1.read_line(&mut origin).unwrap();
    assert_eq!(origin.trim(), "true");
    native.success();
    let partition = config.partition.clone();
    let publisher_node = Node::new(config.clone()).unwrap();
    let subscriber_node = Node::new(config).unwrap();
    let publisher = publisher_node
        .advertise_typed::<gz_msgs::int32::Int32>("/foo")
        .unwrap();
    let subscription = subscriber_node.subscribe("/foo", "gz.msgs.Int32").unwrap();
    // Native MessageInfo marks a second Node in this process as intra-process.
    let deadline = Instant::now() + Duration::from_secs(10);
    let message = loop {
        publisher.publish(int32_five()).unwrap();
        match subscription.recv_with_info_timeout(Duration::from_millis(100)) {
            Ok(message) => break message,
            Err(Error::Timeout) => assert!(Instant::now() < deadline),
            other => panic!("unexpected receive: {other:?}"),
        }
    };
    assert_eq!(message.info().topic, "/foo");
    assert_eq!(message.info().partition, format!("/{partition}"));
    assert_eq!(message.info().type_name, "gz.msgs.Int32");
    assert_eq!(
        gz_msgs::int32::Int32::decode(message.data.as_slice())
            .unwrap()
            .data,
        5
    );
    assert!(message.info().intra_process);
}

#[test]
fn remote_message_info_survives_retirement_recreation_and_shutdown() {
    let config = config();
    let partition = config.partition.clone();
    let first = {
        let node = Node::new(config.clone()).unwrap();
        let old = node.subscribe("/foo", "gz.msgs.Int32").unwrap();
        let mut publisher = Peer::start(&config, "message-info-publisher", "/foo", "5");
        let first = old.recv_with_info_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(
            gz_msgs::int32::Int32::decode(first.data.as_slice())
                .unwrap()
                .data,
            5
        );
        assert!(!first.info().intra_process);
        node.unsubscribe("/foo").unwrap();
        let replacement = node.subscribe("/foo", "gz.msgs.Int32").unwrap();
        writeln!(publisher.0.stdin.as_mut().unwrap(), "6").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let message = replacement
                .recv_with_info_timeout(Duration::from_secs(1))
                .unwrap();
            assert_eq!(message.info().topic, "/foo");
            assert_eq!(message.info().partition, format!("/{partition}"));
            assert_eq!(message.info().type_name, "gz.msgs.Int32");
            assert!(!message.info().intra_process);
            let data = gz_msgs::int32::Int32::decode(message.data.as_slice())
                .unwrap()
                .data;
            if data == 6 {
                break;
            }
            assert_eq!(data, 5);
            assert!(Instant::now() < deadline);
        }
        loop {
            match old.recv_with_info_timeout(Duration::ZERO) {
                Ok(message) => {
                    assert_eq!(
                        gz_msgs::int32::Int32::decode(message.data.as_slice())
                            .unwrap()
                            .data,
                        5
                    );
                    assert!(!message.info().intra_process);
                }
                Err(Error::Shutdown) => break,
                other => panic!("retired sender survived replacement: {other:?}"),
            }
        }
        writeln!(publisher.0.stdin.as_mut().unwrap(), "QUIT").unwrap();
        publisher.success();
        first
    };
    assert_eq!(first.info().topic, "/foo");
    assert_eq!(first.info().partition, format!("/{partition}"));
    assert_eq!(first.info().type_name, "gz.msgs.Int32");
    assert!(!first.info().intra_process);
    assert_eq!(
        gz_msgs::int32::Int32::decode(first.into_message().data.as_slice())
            .unwrap()
            .data,
        5
    );
}

#[test]
fn cpp_observes_rust_publication_as_remote_with_exact_metadata() {
    let config = config();
    let mut subscriber = Peer::start(&config, "message-info-subscribe", "/foo", "5");
    let node = Node::new(config).unwrap();
    let publisher = node
        .advertise_typed::<gz_msgs::int32::Int32>("/foo")
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while subscriber.0.try_wait().unwrap().is_none() {
        assert!(
            Instant::now() < deadline,
            "native metadata callback did not finish"
        );
        publisher.publish(int32_five()).unwrap();
        thread::sleep(Duration::from_millis(20));
    }
    subscriber.success();
}

#[test]
fn message_info_partition_spelling_matches_native() {
    let peer = std::env::var("GZ_CPP_PEER")
        .unwrap_or_else(|_| format!("{}/artifacts/cpp-peer", env!("CARGO_MANIFEST_DIR")));
    for (partition, expected) in [("", ""), ("/metadata/", "/metadata"), ("café", "/café")] {
        let config = Config {
            partition: partition.into(),
            namespace: "/ns/".into(),
            ..config()
        };
        let qualified = gz_transport::qualified_name(partition, "/ns/", "tóp").unwrap();
        let native = Command::new(&peer)
            .args(["message-info-decompose", &qualified, expected])
            .output()
            .unwrap();
        assert!(native.status.success());
        assert!(native.stderr.is_empty());
        assert_eq!(
            String::from_utf8(native.stdout).unwrap(),
            format!("/ns/tóp\n{expected}\n")
        );
        let node = Node::new(config).unwrap();
        let publisher = node.advertise("tóp", "opaque.Type").unwrap();
        let subscription = node.subscribe("tóp", "opaque.Type").unwrap();
        await_publication_membership(&publisher);
        publisher.publish(vec![0, 255, 128]).unwrap();
        let received = subscription
            .recv_with_info_timeout(Duration::from_secs(10))
            .unwrap();
        assert_eq!(received.info().topic, "/ns/tóp");
        assert_eq!(received.info().partition, expected);
        assert_eq!(received.info().type_name, "opaque.Type");
        assert!(received.info().intra_process);
        assert_eq!(received.data, [0, 255, 128]);
    }
}

// twoProcsPubSub.cc:twoProcPubSub.TopicList.
// Original assertion sequence: Copyright (C) 2014 Open Source Robotics Foundation.
#[test]
fn original_two_process_topic_list() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let mut publisher = Peer::start(&config, "list-two-publishers", "/foo", "unused");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if node.known_topics().unwrap().contains(&"/foo".to_owned()) {
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(100));
    }
    let mut topics = node.known_topics().unwrap();
    assert!(!topics.is_empty());
    assert!(topics.contains(&"/foo".to_owned()));
    topics.clear();
    let start = Instant::now();
    topics = node.known_topics().unwrap();
    let elapsed = start.elapsed();
    assert!(!topics.is_empty());
    assert!(topics.contains(&"/foo".to_owned()));
    assert!(elapsed < Duration::from_millis(2), "elapsed={elapsed:?}");
    writeln!(publisher.0.stdin.as_mut().unwrap(), "QUIT").unwrap();
    publisher.success();
}

#[test]
fn subscriber_rate_admission_covers_real_cpp_wire_and_same_node_delivery() {
    let config = config();
    let node = Node::new(config.clone()).unwrap();
    let mut options = SubscribeOptions::default();
    options.set_msgs_per_sec(0);
    let zero = node
        .subscribe_with_options("/subscriber_rate", TYPE, options)
        .unwrap();
    options.set_msgs_per_sec(1);
    let limited = node
        .subscribe_with_options("/subscriber_rate", TYPE, options)
        .unwrap();
    let witness = node.subscribe("/subscriber_rate", TYPE).unwrap();
    let local = node.advertise("/subscriber_rate", TYPE).unwrap();
    let mut peer = Peer::start(&config, "publish", "/subscriber_rate", "cpp-rate");
    let first = limited
        .recv_with_info_timeout(Duration::from_secs(5))
        .unwrap();
    assert!(!first.info().intra_process);
    assert_eq!(first.data, data("cpp-rate"));
    assert_eq!(
        witness.recv_timeout(Duration::from_secs(2)).unwrap().data,
        data("cpp-rate")
    );
    local.publish(data("local-rate")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let message = witness.recv_timeout(Duration::from_millis(100)).unwrap();
        if message.data == data("local-rate") {
            break;
        }
        assert_eq!(message.data, data("cpp-rate"));
        assert!(Instant::now() < deadline);
    }
    // The first actual wire admission and local publication use one budget.
    // These bounded checkpoints do not assert a settled callback count.
    assert_eq!(
        limited.recv_timeout(Duration::from_millis(100)),
        Err(Error::Timeout)
    );
    assert_eq!(
        zero.recv_timeout(Duration::from_millis(100)),
        Err(Error::Timeout)
    );
    assert_eq!(node.statistics().subscription_clock_errors, 0);
    peer.success();
}

// Supplemental startup checks reuse the unchanged held native endpoints.
#[test]
fn ready_topic_info_observes_preexisting_cpp_endpoints_on_first_query() {
    for (mode, publication) in [
        ("list-vector3d-publisher", true),
        ("list-subscriber", false),
    ] {
        let config = config();
        // The peer's existing READY token follows successful registration;
        // construct the fresh Rust query Node only after that token.
        let mut native = Peer::start(&config, mode, "/ready-info", "unused");
        let node = Node::new(config).unwrap();
        let first = node
            .topic_info("/ready-info", Duration::from_secs(5))
            .unwrap();
        if publication {
            assert_eq!(first.publishers, ["gz.msgs.Vector3d"]);
            assert!(first.subscribers.is_empty());
        } else {
            assert!(first.publishers.is_empty());
            assert_eq!(first.subscribers, [TYPE]);
        }
        assert_eq!(
            node.topic_info("/ready-info", Duration::from_secs(5))
                .unwrap(),
            first
        );
        native.0.stdin.take().unwrap().write_all(b"QUIT\n").unwrap();
        native.success();
    }
}

// Native ServiceList waits for service discovery startup without SUBSCRIBERS_REQ.
#[test]
fn ready_service_list_matches_fresh_native_queries_for_held_providers_and_absence() {
    for kind in ["ordinary", "inputless", "outputless", "absent"] {
        let cfg = config();
        let mut provider = (kind != "absent")
            .then(|| Peer::start(&cfg, "service-info-reference", "/ready-service", kind));
        let mut native = Peer::start(&cfg, "service-list-startup", "/ready-service", "unused");
        let node = Node::new(cfg).unwrap();
        let expected = if kind == "absent" {
            Vec::new()
        } else {
            vec!["/ready-service".to_owned()]
        };
        let first = node.service_list(Duration::from_secs(5)).unwrap();
        assert_eq!(first, expected, "{kind}");
        // A warm query must not restart the two-interval startup wait.
        assert_eq!(node.service_list(Duration::from_secs(1)).unwrap(), first);
        for _ in 0..2 {
            let mut result = String::new();
            native.1.read_line(&mut result).unwrap();
            let expected_line = if kind == "absent" {
                "0\n"
            } else {
                "1 /ready-service\n"
            };
            assert_eq!(result, expected_line, "native {kind}");
        }
        native.success();
        if let Some(ref mut provider) = provider {
            provider
                .0
                .stdin
                .take()
                .unwrap()
                .write_all(b"QUIT\n")
                .unwrap();
            provider.success();
            let deadline = Instant::now() + Duration::from_secs(5);
            while !node
                .service_list(Duration::from_secs(1))
                .unwrap()
                .is_empty()
            {
                assert!(
                    Instant::now() < deadline,
                    "retired native service stayed visible"
                );
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

#[test]
fn owned_service_info_matches_every_native_provider_field() {
    for (kind, count) in [
        ("ordinary", 1),
        ("ordinary-host", 1),
        ("inputless", 1),
        ("outputless", 1),
        ("ordinary", 2),
        ("absent", 0),
    ] {
        let cfg = config();
        let mut providers: Vec<_> = (0..count)
            .map(|_| Peer::start(&cfg, "service-info-reference", "/full-info", kind))
            .collect();
        let mut native = Peer::start(&cfg, "service-info-details", "/full-info", "unused");
        let node = Node::new(cfg.clone()).unwrap();
        // Service startup is separate from message topic_info preparation.
        node.service_list(Duration::from_secs(5)).unwrap();
        let actual = node.known_service_info("/full-info").unwrap();
        let mut line = String::new();
        native.1.read_line(&mut line).unwrap();
        assert_eq!(
            line,
            format!("{} {count}\n", usize::from(count != 0)),
            "{kind}"
        );
        let mut expected = Vec::new();
        for _ in 0..count {
            let fields: Vec<_> = (0..8)
                .map(|_| {
                    let mut line = String::new();
                    assert!(native.1.read_line(&mut line).unwrap() > 0);
                    line.strip_suffix('\n').unwrap().to_owned()
                })
                .collect();
            expected.push(gz_transport::ServiceProviderInfo {
                topic: fields[0].clone(),
                address: fields[1].clone(),
                process_uuid: fields[2].clone(),
                node_uuid: fields[3].clone(),
                scope: Scope::try_from(fields[4].parse::<i32>().unwrap()).unwrap(),
                socket_id: fields[5].clone(),
                request_type: fields[6].clone(),
                response_type: fields[7].clone(),
            });
        }
        expected.sort();
        assert_eq!(actual, expected, "{kind}");
        assert_eq!(actual.len(), count);
        assert_eq!(node.known_service_info("/full-info").unwrap(), actual);
        assert_eq!(
            node.service_provider_types("/full-info").unwrap().len(),
            count
        );
        if count == 2 {
            assert_ne!(actual[0].process_uuid, actual[1].process_uuid);
            assert_eq!(node.service_type_signatures("/full-info").unwrap().len(), 1);
        }
        native.success();
        for provider in &mut providers {
            provider
                .0
                .stdin
                .take()
                .unwrap()
                .write_all(b"QUIT\n")
                .unwrap();
            provider.success();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while !node.known_service_info("/full-info").unwrap().is_empty() {
            assert!(Instant::now() < deadline, "native provider not retired");
            thread::sleep(Duration::from_millis(10));
        }
        // Owned observations do not keep the remote services registered.
        assert_eq!(actual, expected);
    }
}

#[test]
fn owned_topic_publishers_match_every_native_publication_field() {
    for (mode, value, count) in [
        ("list-vector3d-publisher", "unused", 1),
        ("list-option-publisher", "all:18446744073709551615", 1),
        ("list-option-publisher", "host:7", 1),
        ("list-option-publisher", "all:0", 1),
        ("list-option-publisher", "all:7", 2),
        ("absent", "unused", 0),
    ] {
        let cfg = config();
        let mut providers: Vec<_> = (0..count)
            .map(|_| Peer::start(&cfg, mode, "/publication-info", value))
            .collect();
        let mut native = Peer::start(
            &cfg,
            "topic-publisher-details",
            "/publication-info",
            "unused",
        );
        let node = Node::new(cfg).unwrap();
        node.topic_info("/publication-info", Duration::from_secs(5))
            .unwrap();
        let actual = node.known_topic_publishers("/publication-info").unwrap();
        let mut header = String::new();
        native.1.read_line(&mut header).unwrap();
        assert_eq!(header, format!("1 {count}\n"));
        let mut expected = Vec::new();
        for _ in 0..count {
            let fields: Vec<_> = (0..9)
                .map(|_| {
                    let mut line = String::new();
                    assert!(native.1.read_line(&mut line).unwrap() > 0);
                    line.strip_suffix('\n').unwrap().to_owned()
                })
                .collect();
            expected.push(gz_transport::TopicPublisherInfo {
                topic: fields[0].clone(),
                address: fields[1].clone(),
                control_address: fields[2].clone(),
                process_uuid: fields[3].clone(),
                node_uuid: fields[4].clone(),
                scope: Scope::try_from(fields[5].parse::<i32>().unwrap()).unwrap(),
                type_name: fields[6].clone(),
                throttled: fields[7] == "1",
                msgs_per_sec: fields[8].parse().unwrap(),
            });
        }
        expected.sort();
        assert_eq!(actual, expected, "{mode}/{value}");
        assert_eq!(actual.len(), count);
        assert_eq!(
            node.known_topic_publishers("/publication-info").unwrap(),
            actual
        );
        if count == 2 {
            assert_ne!(actual[0].process_uuid, actual[1].process_uuid);
            assert_eq!(actual[0].type_name, actual[1].type_name);
        }
        native.success();
        for provider in &mut providers {
            provider
                .0
                .stdin
                .take()
                .unwrap()
                .write_all(b"QUIT\n")
                .unwrap();
            provider.success();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while !node
            .known_topic_publishers("/publication-info")
            .unwrap()
            .is_empty()
        {
            assert!(Instant::now() < deadline, "native publisher stayed visible");
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(actual, expected);
    }
}
