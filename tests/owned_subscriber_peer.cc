// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Stored native handle validity is distinct from live registration state.
#include <gz/msgs/int32.pb.h>
#include <gz/transport/Node.hh>
#include <cassert>
#include <functional>
#include <iostream>
int main()
{
  std::function<void(const gz::msgs::Int32 &)> cb =
      [](const gz::msgs::Int32 &) {};
  gz::transport::Node node;
  auto sub = node.CreateSubscriber("/foo", cb);
  assert(sub);
  const bool first = sub.Unsubscribe();
  const bool validAfter = static_cast<bool>(sub);
  const bool repeated = sub.Unsubscribe();
  assert(first && !validAfter && !repeated);
  std::cout << "own first=" << first << " valid_after=" << validAfter
            << " repeated=" << repeated << '\n';
  auto held = node.CreateSubscriber("/foo", cb);
  assert(held);
  const bool nodeResult = node.Unsubscribe("/foo");
  const bool heldValid = static_cast<bool>(held);
  const bool heldResult = held.Unsubscribe();
  std::cout << "node unsubscribe=" << nodeResult << " held_valid=" << heldValid
            << " held_unsubscribe=" << heldResult << '\n';
  gz::transport::Node::Subscriber orphan;
  {
    gz::transport::Node owner;
    orphan = owner.CreateSubscriber("/orphan", cb);
    assert(orphan);
  }
  const bool orphanValid = static_cast<bool>(orphan);
  const bool orphanResult = orphan.Unsubscribe();
  std::cout << "owner_exit held_valid=" << orphanValid
            << " held_unsubscribe=" << orphanResult << '\n';
}
