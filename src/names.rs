// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use crate::Error;

// Gazebo TopicUtils.cc: byte lengths, ASCII space, and ':=' are intentional.
fn valid(name: &str) -> bool {
    name.len() <= 65535
        && name != "/"
        && !["~", " ", "//", "@", ":="].iter().any(|s| name.contains(s))
}

/// Resolve a topic exactly as Gazebo TopicUtils::FullyQualifiedName does.
pub fn qualified_name(partition: &str, namespace: &str, topic: &str) -> Result<String, Error> {
    if !valid(partition) || !valid(namespace) || !valid(topic) || topic.is_empty() {
        return Err(Error::InvalidName);
    }
    let partition = partition.trim_matches('/');
    let namespace = namespace.trim_matches('/');
    let topic = topic.trim_end_matches('/');
    let path = if topic.starts_with('/') || namespace.is_empty() {
        format!("/{}", topic.trim_start_matches('/'))
    } else {
        format!("/{namespace}/{topic}")
    };
    let partition = if partition.is_empty() {
        String::new()
    } else {
        format!("/{partition}")
    };
    let name = format!("@{partition}@{path}");
    if name.len() > 65535 {
        Err(Error::InvalidName)
    } else {
        Ok(name)
    }
}
