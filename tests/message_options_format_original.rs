/*
 * Copyright (C) 2015 Open Source Robotics Foundation
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
// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// AdvertiseOptionsTest.msgStreamInsertion ordinary stream-output assertions.
use gz_transport::{AdvertiseOptions, Scope};

#[test]
fn original_message_options_stream_insertion() {
    let mut options = AdvertiseOptions::default();
    assert_eq!(
        format!("{options}"),
        "Advertise options:\n\tScope: All\n\tThrottled? No\n"
    );
    options.set_msgs_per_sec(10);
    assert_eq!(
        format!("{options}"),
        "Advertise options:\n\tScope: All\n\tThrottled? Yes\n\tRate: 10 msgs/sec\n"
    );
}

#[test]
fn message_options_format_matches_native_scope_and_rate_bytes() {
    let fixture: gz_msgs::serde_json::Value =
        gz_msgs::serde_json::from_str(include_str!("fixtures/message-options-format.json"))
            .unwrap();
    assert_eq!(
        fixture["native_revision"],
        "41913b9dfc3dbce33ac5464ddcb427cc82450f26"
    );
    let rows = fixture["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 24);
    for row in rows {
        assert_eq!(row["status"], 0);
        assert_eq!(row["stderr_hex"], "");
        let mut options = AdvertiseOptions::default();
        options.set_scope(match row["scope"].as_str().unwrap() {
            "process" => Scope::Process,
            "host" => Scope::Host,
            "all" => Scope::All,
            other => panic!("unexpected native scope: {other}"),
        });
        options.set_msgs_per_sec(row["rate"].as_u64().unwrap());
        let hex = row["stdout_hex"].as_str().unwrap();
        let expected: Vec<u8> = hex
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect();
        assert_eq!(format!("{options}").as_bytes(), expected, "{row}");
    }
}

#[test]
fn message_options_display_preserves_rust_writer_errors() {
    struct Limited(usize);
    impl std::fmt::Write for Limited {
        fn write_str(&mut self, value: &str) -> std::fmt::Result {
            if value.len() > self.0 {
                return Err(std::fmt::Error);
            }
            self.0 -= value.len();
            Ok(())
        }
    }
    use std::fmt::Write;
    let mut options = AdvertiseOptions::default();
    assert!(write!(Limited(0), "{options}").is_err());
    let prefix = "Advertise options:\n\tScope: All\n".len();
    assert!(write!(Limited(prefix), "{options}").is_err());
    options.set_msgs_per_sec(10);
    assert!(write!(Limited(prefix), "{options}").is_err());
    assert!(write!(Limited(100), "{options}").is_ok());
}
