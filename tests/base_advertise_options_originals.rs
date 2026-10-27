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
// Fixed base-option scope values use the combined Rust options at default rate.
use gz_transport::{AdvertiseOptions, Scope};

#[test]
fn original_base_options_default_constructor() {
    let options = AdvertiseOptions::default();
    assert_eq!(options.scope(), Scope::All);
}

#[test]
fn original_base_options_copy_constructor() {
    let mut first = AdvertiseOptions::default();
    first.set_scope(Scope::Host);
    let second = first;
    assert_eq!(second.scope(), first.scope());
}

#[test]
fn original_base_options_assignment() {
    let mut first = AdvertiseOptions::default();
    let mut second = AdvertiseOptions::default();
    // Read the original destination before assignment; its default is supplementary.
    assert_eq!(second.scope(), Scope::All);
    first.set_scope(Scope::Process);
    second = first;
    assert_eq!(second.scope(), first.scope());
}

#[test]
fn original_base_options_equality() {
    let mut first = AdvertiseOptions::default();
    let mut second = AdvertiseOptions::default();
    first.set_scope(Scope::Process);
    assert!(!(first == second));
    assert!(first != second);
    second.set_scope(Scope::Process);
    assert!(first == second);
    assert!(!(first != second));
}

#[test]
fn original_base_options_accessors() {
    let mut options = AdvertiseOptions::default();
    assert_eq!(options.scope(), Scope::All);
    options.set_scope(Scope::Host);
    assert_eq!(options.scope(), Scope::Host);
}
