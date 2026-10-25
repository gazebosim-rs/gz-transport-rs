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
use super::{decompose_fully_qualified_topic, qualified_name, valid};
use crate::{Config, Error};

// TopicUtils_TEST.cc:TopicUtilsTest.testTopics. Native topics cannot be empty.
#[test]
fn upstream_valid_topic_preserves_original_predicates() {
    let max = "a".repeat(65_535);
    for topic in [
        "abc", "/abc", "abc/de", "a", "abc/", "/abc/", "/abc/d", "/abc/d/e", &max,
    ] {
        assert!(!topic.is_empty() && valid(topic), "valid topic: {topic:?}");
    }
    let over_max = "a".repeat(65_536);
    assert_eq!(qualified_name("", "", ""), Err(Error::InvalidName));
    for topic in [
        " ",
        "~a",
        "topic1 ",
        "abc//def",
        "ab~cd",
        "/",
        "~/",
        "~",
        "@partition",
        "topic:=",
        &over_max,
    ] {
        assert!(!valid(topic), "invalid topic: {topic:?}");
    }
}

// TopicUtils_TEST.cc:TopicUtilsTest.testNamespaces. Empty is a valid namespace.
#[test]
fn upstream_valid_namespace_preserves_original_predicates() {
    let max = "a".repeat(65_535);
    for namespace in [
        "/abcde",
        "abcde",
        "abcde/",
        "/abcde/",
        "/abcde/fg",
        "/abcde/fg/",
        "",
        &max,
    ] {
        assert!(valid(namespace), "valid namespace: {namespace:?}");
    }
    let over_max = "a".repeat(65_536);
    for namespace in [
        "/",
        " ",
        "ns ",
        "abc//def",
        "ab~cd",
        "~/abcde",
        "~abcde",
        "@namespace",
        "namespace:=",
        &over_max,
    ] {
        assert!(!valid(namespace), "invalid namespace: {namespace:?}");
    }
}

// TopicUtils_TEST.cc:TopicUtilsTest.testPartitions. The same component grammar
// is applied by Config::set_partition, including the valid empty partition.
#[test]
fn upstream_valid_partition_preserves_original_predicates() {
    let max = "a".repeat(65_535);
    for partition in [
        "/abcde",
        "abcde",
        "abcde/",
        "/abcde/",
        "/abcde/fg",
        "/abcde/fg/",
        "",
        &max,
    ] {
        let mut config = Config::default();
        assert_eq!(config.set_partition(partition), Ok(()), "{partition:?}");
        assert_eq!(config.partition, partition);
    }
    let over_max = "a".repeat(65_536);
    for partition in [
        "/",
        " ",
        "ns ",
        "abc//def",
        "ab~cd",
        "~/abcde",
        "~abcde",
        "@namespace",
        "namespace:=",
        &over_max,
    ] {
        let mut config = Config::default();
        config.set_partition("retained").unwrap();
        assert_eq!(
            config.set_partition(partition),
            Err(Error::InvalidName),
            "{partition:?}"
        );
        assert_eq!(config.partition, "retained");
    }
}

// TopicUtils_TEST.cc:TopicUtilsTest.decomposeFullyQualifiedTopic.
#[test]
fn upstream_decompose_fully_qualified_topic_preserves_original_assertions() {
    for (name, partition, topic) in [
        ("@/foo@/fiz/buz", "/foo", "/fiz/buz"),
        ("@/foo@/bar", "/foo", "/bar"),
        ("@@/bar", "", "/bar"),
    ] {
        assert_eq!(
            decompose_fully_qualified_topic(name),
            Ok((partition, topic)),
            "{name:?}"
        );
    }
    for name in ["@/bar", "@@@/bar", "@/bar@", "@@", "@/foo@/"] {
        assert_eq!(
            decompose_fully_qualified_topic(name),
            Err(Error::InvalidName),
            "{name:?}"
        );
    }
}
