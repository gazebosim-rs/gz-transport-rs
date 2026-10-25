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
// New Rust adaptation: Copyright (C) 2026 Open Source Robotics Foundation.
// SPDX-License-Identifier: Apache-2.0
use gz_msgs::{int32::Int32, vector3d::Vector3d};
use gz_transport::{Config, Error, Node};
#[test]
fn original_type_mismatch() {
    let right = Int32 {
        data: 1,
        ..Default::default()
    };
    let wrong = Vector3d {
        x: 1.,
        y: 2.,
        z: 3.,
        ..Default::default()
    };
    let node = Node::new(Config {
        partition: format!("typed-prototype-{}", uuid::Uuid::new_v4()),
        ..Default::default()
    })
    .unwrap();
    let publisher = node.advertise_typed::<Int32>("/foo").unwrap();
    assert_eq!(publisher.publish_typed(&wrong), Err(Error::InvalidFrame));
    assert_eq!(publisher.publish_typed(&right), Ok(()));
}
