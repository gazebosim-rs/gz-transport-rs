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
 *
*/
// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Service option assertions from AdvertiseOptions_TEST.cc: srv* cases.
use gz_transport::{Scope, ServiceOptions};
#[test]
fn original_service_options_default_constructor() {
    let options = ServiceOptions::default();
    assert_eq!(options.scope(), Scope::All);
}
#[test]
fn original_service_options_copy_constructor() {
    let mut first = ServiceOptions::default();
    first.set_scope(Scope::Host);
    let second = first;
    assert_eq!(first, second);
}
#[test]
fn original_service_options_assignment() {
    let mut first = ServiceOptions::default();
    let mut second = ServiceOptions::default();
    assert_eq!(second.scope(), Scope::All);
    first.set_scope(Scope::Process);
    second = first;
    assert_eq!(first, second);
}
#[test]
fn original_service_options_equality() {
    let mut first = ServiceOptions::default();
    let mut second = ServiceOptions::default();
    first.set_scope(Scope::Process);
    assert!(!(first == second));
    assert!(first != second);
    second.set_scope(Scope::Process);
    assert!(first == second);
    assert!(!(first != second));
}
#[test]
fn original_service_options_stream_insertion() {
    assert_eq!(
        ServiceOptions::default().to_string(),
        "Advertise options:\n\tScope: All\n"
    );
}
#[test]
fn original_service_options_accessors() {
    let mut options = ServiceOptions::default();
    assert_eq!(options.scope(), Scope::All);
    options.set_scope(Scope::Host);
    assert_eq!(options.scope(), Scope::Host);
}
#[test]
fn service_scope_format_literals_and_writer_error_are_preserved() {
    use std::fmt::Write;
    struct Reject;
    impl std::fmt::Write for Reject {
        fn write_str(&mut self, _: &str) -> std::fmt::Result {
            Err(std::fmt::Error)
        }
    }
    for (scope, expected) in [
        (Scope::All, "Advertise options:\n\tScope: All\n"),
        (Scope::Host, "Advertise options:\n\tScope: Host\n"),
        (Scope::Process, "Advertise options:\n\tScope: Process\n"),
    ] {
        let mut options = ServiceOptions::default();
        options.set_scope(scope);
        assert_eq!(options.to_string(), expected);
        assert!(write!(Reject, "{options}").is_err());
    }
}
