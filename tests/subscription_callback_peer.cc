// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Bounded native scheduling/lifetime characterization, not an original test.
#include <gz/transport/Node.hh>
#include <gz/msgs/int32.pb.h>
#include <cassert>
#include <chrono>
#include <condition_variable>
#include <cstdlib>
#include <filesystem>
#include <functional>
#include <iostream>
#include <mutex>
#include <optional>
#include <set>
#include <thread>
#include <vector>
using namespace std::chrono_literals;
using Callback = std::function<void(const gz::msgs::Int32 &,
    const gz::transport::MessageInfo &)>;
static std::size_t threads()
{
  return std::distance(std::filesystem::directory_iterator("/proc/self/task"),
      std::filesystem::directory_iterator{});
}
static void connected(const gz::transport::Node::Publisher &publisher)
{
  const auto deadline = std::chrono::steady_clock::now() + 1s;
  while (!publisher.HasConnections())
  {
    assert(std::chrono::steady_clock::now() < deadline);
    std::this_thread::sleep_for(1ms);
  }
}
static gz::msgs::Int32 five()
{
  gz::msgs::Int32 value;
  value.set_data(5);
  return value;
}
static void many()
{
  gz::transport::Node node;
  auto publisher = node.Advertise<gz::msgs::Int32>("/foo");
  assert(publisher);
  const auto caller = std::this_thread::get_id();
  const std::string partition = std::string("/") + std::getenv("GZ_PARTITION");
  std::mutex mutex;
  std::condition_variable ready;
  unsigned calls = 0;
  std::set<std::thread::id> ids;
  const auto before = threads();
  std::vector<gz::transport::Node::Subscriber> subscriptions;
  for (unsigned i = 0; i < 32; ++i)
  {
    Callback callback = [&](const auto &value, const auto &info)
    {
      assert(value.data() == 5 && info.Topic() == "/foo");
      assert(info.Partition() == partition && info.Type() == "gz.msgs.Int32");
      assert(info.IntraProcess() && std::this_thread::get_id() != caller);
      std::lock_guard<std::mutex> lock(mutex);
      ids.insert(std::this_thread::get_id());
      ++calls;
      ready.notify_all();
    };
    auto subscription = node.CreateSubscriber("/foo", callback);
    assert(subscription);
    subscriptions.emplace_back(std::move(subscription));
  }
  const auto after = threads();
  connected(publisher);
  assert(publisher.Publish(five()));
  std::unique_lock<std::mutex> lock(mutex);
  assert(ready.wait_for(lock, 2s, [&]{return calls == 32;}));
  assert(ids.size() == 1);
  std::cout << "many callbacks=" << calls << " callback_threads=" << ids.size()
    << " caller_thread=0 threads_before=" << before << " threads_after="
    << after << std::endl;
}
static void retire()
{
  gz::transport::Node first, second;
  auto blocked = first.Advertise<gz::msgs::Int32>("/blocked");
  auto pending = first.Advertise<gz::msgs::Int32>("/pending");
  auto other = second.Advertise<gz::msgs::Int32>("/other");
  assert(blocked && pending && other);
  std::mutex mutex;
  std::condition_variable ready;
  bool entered = false, release = false, finished = false;
  unsigned pendingCalls = 0, otherCalls = 0;
  Callback slow = [&](const auto &value, const auto &)
  {
    assert(value.data() == 5);
    std::unique_lock<std::mutex> lock(mutex);
    entered = true;
    ready.notify_all();
    assert(ready.wait_for(lock, 2s, [&]{return release;}));
    finished = true;
    ready.notify_all();
  };
  Callback next = [&](const auto &value, const auto &)
  {
    assert(value.data() == 5);
    std::lock_guard<std::mutex> lock(mutex);
    ++pendingCalls;
    ready.notify_all();
  };
  Callback cross = [&](const auto &value, const auto &)
  {
    assert(value.data() == 5);
    std::lock_guard<std::mutex> lock(mutex);
    ++otherCalls;
    ready.notify_all();
  };
  auto a = first.CreateSubscriber("/blocked", slow);
  auto b = first.CreateSubscriber("/pending", next);
  auto c = second.CreateSubscriber("/other", cross);
  assert(a && b && c);
  connected(blocked);
  assert(blocked.Publish(five()));
  {
    std::unique_lock<std::mutex> lock(mutex);
    assert(ready.wait_for(lock, 1s, [&]{return entered;}));
  }
  assert(pending.Publish(five()) && other.Publish(five()));
  std::this_thread::sleep_for(100ms);
  {
    std::lock_guard<std::mutex> lock(mutex);
    assert(pendingCalls == 0 && otherCalls == 0);
  }
  assert(b.Unsubscribe() && !b);
  {
    std::lock_guard<std::mutex> lock(mutex);
    release = true;
    ready.notify_all();
  }
  {
    std::unique_lock<std::mutex> lock(mutex);
    assert(ready.wait_for(lock, 1s, [&]{return finished && otherCalls == 1;}));
  }
  std::this_thread::sleep_for(100ms);
  std::lock_guard<std::mutex> lock(mutex);
  assert(pendingCalls == 0 && otherCalls == 1);
  std::cout << "retire queued_removed=1 blocked_other_node=1 other_after_release=1"
    << std::endl;
}
static void self_and_raw()
{
  gz::transport::Node node;
  auto publisher = node.Advertise<gz::msgs::Int32>("/self");
  assert(publisher);
  std::mutex mutex;
  std::condition_variable ready;
  bool completed = false;
  std::optional<gz::transport::Node::Subscriber> self;
  Callback callback = [&](const auto &value, const auto &)
  {
    assert(value.data() == 5);
    assert(self->Unsubscribe());
    std::lock_guard<std::mutex> lock(mutex);
    completed = true;
    ready.notify_all();
  };
  self.emplace(node.CreateSubscriber("/self", callback));
  assert(*self);
  connected(publisher);
  assert(publisher.Publish(five()));
  {
    std::unique_lock<std::mutex> lock(mutex);
    assert(ready.wait_for(lock, 1s, [&]{return completed;}));
  }
  assert(!*self);
  auto rawPublisher = node.Advertise<gz::msgs::Int32>("/raw");
  assert(rawPublisher);
  const auto caller = std::this_thread::get_id();
  unsigned rawCalls = 0;
  Callback raw = [&](const auto &value, const auto &info)
  {
    assert(value.data() == 5 && info.IntraProcess());
    assert(std::this_thread::get_id() == caller);
    ++rawCalls;
  };
  auto rawSub = node.CreateSubscriber("/raw", raw);
  assert(rawSub);
  connected(rawPublisher);
  assert(rawPublisher.PublishRaw(five().SerializeAsString(), "gz.msgs.Int32"));
  assert(rawCalls == 1);
  std::cout << "self_unsubscribe=1 raw_inline=1 raw_callback_before_return=1"
    << std::endl;
}
int main()
{
  assert(std::getenv("GZ_PARTITION"));
  many();
  retire();
  self_and_raw();
}
