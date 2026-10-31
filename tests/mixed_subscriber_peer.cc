// Copyright (C) 2014, 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
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
// Publication side of pinned twoProcPubSub.PubSubTwoProcsMixedSubscribers.
#include <gz/msgs/vector3d.pb.h>
#include <gz/transport/Node.hh>
#include <gz/transport/WaitHelpers.hh>
#include <cassert>
#include <chrono>
#include <iostream>
#include <thread>
#ifdef NDEBUG
#error "This reference helper requires active assertions"
#endif

int main()
{
  gz::transport::Node node;
  auto pub = node.Advertise<gz::msgs::Vector3d>("/foo");
  assert(pub);
  assert(!pub.HasConnections());
  std::cout << "READY" << std::endl;
  gz::msgs::Vector3d message;
  message.set_x(1.0);
  message.set_y(2.0);
  message.set_z(3.0);
  assert(gz::transport::waitUntil([&] { return pub.HasConnections(); }));
  int published = 0;
  for (int i = 0; i < 10; ++i)
  {
    assert(pub.Publish(message));
    ++published;
    std::this_thread::sleep_for(std::chrono::milliseconds(500));
  }
  // Supplemental observation after the original schedule: remote logical
  // interest should retire too, rather than only suppressing local callbacks.
  std::cout << "CONNECTIONS " << pub.HasConnections() << std::endl;
  std::cout << "PUBLISHED " << published << std::endl;
}
