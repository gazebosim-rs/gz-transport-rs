// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use crate::Error;

// Gazebo TopicUtils.cc: byte lengths, ASCII space, and ':=' are intentional.
pub(crate) fn valid(name: &str) -> bool {
    name.len() <= 65535
        && name != "/"
        && !["~", " ", "//", "@", ":="].iter().any(|s| name.contains(s))
}

/// Apply Gazebo's one-pass topic sanitization; invalid results become empty.
/// ASCII spaces become underscores, `@`/`~` and each `:=`/`//` pair are removed.
/// Remaining UTF-8 is preserved and the output byte length must fit 65535.
pub fn as_valid_topic(topic: &str) -> String {
    let mut result = String::with_capacity(topic.len());
    let mut input = topic.chars().peekable();
    while let Some(c) = input.next() {
        match c {
            ' ' => result.push('_'),
            '@' | '~' => {}
            ':' if input.peek() == Some(&'=') => {
                input.next();
            }
            '/' if input.peek() == Some(&'/') => {
                input.next();
            }
            c => result.push(c),
        }
    }
    if result.is_empty() || !valid(&result) {
        result.clear();
    }
    result
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

#[cfg(test)]
#[path = "../tests/support/names.rs"]
mod tests;
