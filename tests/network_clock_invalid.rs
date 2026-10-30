// Copyright (C) 2018, 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Supplemental admission control for the documented bad-clock API translation.
use gz_transport::{ClockTimeBase, Error, NetworkClock, Node};
#[path = "support/network_clock_config.rs"]
mod fixture;

#[test]
fn invalid_native_clock_topic_rejects_before_consuming_clock_admission() {
    let (mut config, _reservations) = fixture::config();
    // Exactly two registrations/commands for the later valid clock. No third
    // publication/query is assumed to fit before its worker drains commands.
    config.capacity = 2;
    let mut node = Node::new(config).unwrap();
    assert!(matches!(
        NetworkClock::new(&node, "//bad-clock", ClockTimeBase::Simulation),
        Err(Error::InvalidName)
    ));
    assert!(node.advertised_topics().unwrap().is_empty());
    let clock = NetworkClock::new(&node, "/clock", ClockTimeBase::Simulation).unwrap();
    assert!(!clock.is_ready());
    assert_eq!(clock.time_ns(), 0);
    assert_eq!(node.advertised_topics().unwrap(), ["/clock"]);
    drop(clock);
    assert!(node.advertised_topics().unwrap().is_empty());
    node.shutdown();
}
