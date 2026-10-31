/*
 * Copyright (C) 2014, 2025 Open Source Robotics Foundation
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
// Receiver body of pinned twoProcPubSub.PubSubLargeMessageAboveShmThreshold.
#include <gz/msgs/bytes.pb.h>
#include <gz/transport/Node.hh>
#include <cassert>
#include <chrono>
#include <functional>
#include <iostream>
#include <mutex>
#include <string>
#include <thread>
#ifdef NDEBUG
#error "This reference helper requires active assertions"
#endif

int main()
{
  std::mutex mutex;
  bool executed = false;
  bool invalid = false;
  std::string received;
  gz::transport::Node node;
  std::function<void(const gz::msgs::Bytes &)> callback = [&](const gz::msgs::Bytes &message)
  {
    std::lock_guard<std::mutex> lock(mutex);
    received = message.data();
    executed = true;
    // Supplemental persistent state prevents a later valid packet from hiding
    // an earlier corrupted payload; the original checks the final payload.
    invalid |= received != std::string(256u * 1024u, 'S');
  };
  assert(node.Subscribe("/large_msg", callback));
  std::cout << "READY" << std::endl;
  std::this_thread::sleep_for(std::chrono::milliseconds(2500));
  {
    std::lock_guard<std::mutex> lock(mutex);
    assert(executed);
    const std::size_t expectedSize = 256u * 1024u;
    assert(received.size() == expectedSize);
    assert(received == std::string(expectedSize, 'S'));
    assert(!invalid);
  }
  std::cout << "PASSED" << std::endl;
}
