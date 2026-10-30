// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Safe characterization of queue cancellation versus a selected handler.
#include <gz/msgs/int32.pb.h>
#include <gz/transport/Node.hh>
#include <gz/transport/WaitHelpers.hh>
#include <cassert>
#include <chrono>
#include <condition_variable>
#include <functional>
#include <iostream>
#include <memory>
#include <mutex>
#include <string>
#include <thread>

struct State
{
  std::mutex mutex;
  std::condition_variable ready;
  bool entered = false;
  bool release = false;
  bool finished = false;
  bool returned = false;
  bool result = false;
};
int main()
{
  using namespace std::chrono_literals;
  for (const bool raw : {false, true})
  {
    gz::transport::Node node;
    const std::string topic = raw ? "/raw-selected" : "/typed-selected";
    auto publisher = node.Advertise<gz::msgs::Int32>(topic);
    assert(publisher);
    auto state = std::make_shared<State>();
    auto block = [state]()
    {
      std::unique_lock<std::mutex> lock(state->mutex);
      state->entered = true;
      state->ready.notify_all();
      state->ready.wait(lock, [&] { return state->release; });
      state->finished = true;
      state->ready.notify_all();
    };
    if (raw)
    {
      std::function<void(const char *, size_t, const gz::transport::MessageInfo &)>
          callback = [block](const char *, size_t, const auto &) { block(); };
      assert(node.SubscribeRaw(topic, callback));
    }
    else
    {
      std::function<void(const gz::msgs::Int32 &)> callback =
          [block](const auto &) { block(); };
      assert(node.Subscribe(topic, callback));
    }
    assert(gz::transport::waitUntil([&] { return publisher.HasConnections(); }));
    gz::msgs::Int32 message;
    assert(publisher.Publish(message));
    {
      std::unique_lock<std::mutex> lock(state->mutex);
      assert(state->ready.wait_for(lock, 5s, [&] { return state->entered; }));
    }
    std::thread unsubscribe([&]
    {
      const bool result = node.Unsubscribe(topic);
      std::lock_guard<std::mutex> lock(state->mutex);
      state->result = result;
      state->returned = true;
      state->ready.notify_all();
    });
    bool beforeRelease;
    {
      std::unique_lock<std::mutex> lock(state->mutex);
      beforeRelease = state->ready.wait_for(lock, 1s,
          [&] { return state->returned; });
      assert(!state->finished);
      state->release = true;
      state->ready.notify_all();
    }
    unsubscribe.join();
    {
      std::unique_lock<std::mutex> lock(state->mutex);
      assert(state->ready.wait_for(lock, 5s, [&] { return state->finished; }));
      std::cout << (raw ? "raw" : "typed")
                << " unsubscribe_before_release=" << beforeRelease
                << " result=" << state->result
                << " completed=" << state->finished << std::endl;
    }
  }
}
