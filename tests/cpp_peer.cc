// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Differential peer for pinned Gazebo Transport; no replacement logic is linked.
// Subscriber-only assertion ports: Copyright (C) 2014 Open Source Robotics Foundation.
#include <gz/transport/Node.hh>
#include <gz/transport/NodeShared.hh>
#include <gz/transport/TopicUtils.hh>
#include <gz/msgs/stringmsg.pb.h>
#include <gz/msgs/empty.pb.h>
#include <chrono>
#include <condition_variable>
#include <iostream>
#include <mutex>
#include <memory>
#include <google/protobuf/unknown_field_set.h>
#include <thread>
#include <algorithm>
#include <iterator>

int main(int argc, char **argv) {
  if (argc != 4) return 2;
  const std::string mode=argv[1], topic=argv[2], value=argv[3];
  if (mode == "valid-topic" || mode == "valid-topic-bytes") {
    const auto input=mode == "valid-topic-bytes" ?
      std::string(std::istreambuf_iterator<char>(std::cin), {}) : topic;
    auto result=gz::transport::TopicUtils::AsValidTopic(input);
    std::cout << result;
    return mode == "valid-topic-bytes" || result == value ? 0 : 15;
  }
  gz::transport::Node node;
  if (mode == "config") {
    gz::transport::NodeOptions options;
    auto shared=gz::transport::NodeShared::Instance();
    std::cout << "CONFIG " << options.Partition() << ' ' << shared->msgDiscPort << ' ' << shared->srvDiscPort << ' ' << shared->SndHwm() << ' ' << shared->RcvHwm() << std::endl;
  } else if (mode == "inspect-subscriber") {
    auto observed = [&](bool expected) {
      std::vector<std::string> topics;
      node.TopicList(topics);
      std::vector<gz::transport::MessagePublisher> publishers, subscribers;
      if (!node.TopicInfo(topic, publishers, subscribers)) return false;
      if (!expected) return std::count(topics.begin(), topics.end(), topic) == 0 &&
        publishers.empty() && subscribers.empty();
      return std::count(topics.begin(), topics.end(), topic) == 1 &&
        publishers.empty() && subscribers.size() == 1 &&
        subscribers.front().MsgTypeName() == value &&
        subscribers.front().Ctrl().empty();
    };
    auto wait = [&](bool expected) {
      const auto seconds=expected ? (topic == "/unsub_test" ? 4 : 5) : 8;
      const auto deadline=std::chrono::steady_clock::now()+std::chrono::seconds(seconds);
      do {
        if (observed(expected)) return true;
        std::this_thread::sleep_for(std::chrono::milliseconds(10));
      } while (std::chrono::steady_clock::now() < deadline);
      return false;
    };
    std::vector<std::string> initialTopics;
    node.TopicList(initialTopics); // Initialize discovery before the Rust subscription.
    std::cout << "READY" << std::endl;
    if (!wait(true)) return 12;
    std::cout << "FOUND" << std::endl;
    std::string command;
    if (!std::getline(std::cin, command) || command != "KEEP" || !observed(true)) return 13;
    std::cout << "KEPT" << std::endl;
    if (!std::getline(std::cin, command) || command != "GONE" || !wait(false)) return 14;
  } else if (mode == "publish" || mode == "publish-unknown") {
    auto publisher=node.Advertise<gz::msgs::StringMsg>(topic);
    if (!publisher) return 3;
    std::cout << "READY" << std::endl;
    gz::msgs::StringMsg message; message.set_data(value);
    if (mode == "publish-unknown") message.GetReflection()->MutableUnknownFields(&message)->AddVarint(127,1);
    for (int i=0;i<150;++i) {
      publisher.Publish(message);
      std::this_thread::sleep_for(std::chrono::milliseconds(20));
    }
  } else if (mode == "subscribe" || mode == "subscribe-unknown") {
    struct State { std::mutex mutex; std::condition_variable cv; bool received=false; };
    auto state=std::make_shared<State>();
    std::function<void(const gz::msgs::StringMsg&)> callback = [state,value,mode](const auto &message) {
      std::lock_guard<std::mutex> lock(state->mutex);
      const auto &unknown=message.GetReflection()->GetUnknownFields(message);
      const bool fields=mode!="subscribe-unknown" || (unknown.field_count()==1 && unknown.field(0).number()==127 && unknown.field(0).varint()==1);
      if (message.data() == value && fields) { state->received=true; state->cv.notify_all(); }
    };
    if (!node.Subscribe(topic,callback)) return 4;
    std::cout << "READY" << std::endl;
    std::unique_lock<std::mutex> lock(state->mutex);
    if (!state->cv.wait_for(lock,std::chrono::seconds(8),[&]{return state->received;})) return 5;
  } else if (mode == "serve" || mode == "serve-host") {
    std::function<bool(const gz::msgs::StringMsg&,gz::msgs::StringMsg&)> callback = [value](const auto &request,auto &response) {
      response.set_data(request.data()+value); return true;
    };
    gz::transport::AdvertiseServiceOptions options;
    if (mode == "serve-host") options.SetScope(gz::transport::Scope_t::HOST);
    if (!node.Advertise(topic,callback,options)) return 6;
    std::cout << "READY" << std::endl;
    std::this_thread::sleep_for(std::chrono::seconds(8));
  } else if (mode == "serve-oneway") {
    struct State { std::mutex mutex; std::condition_variable cv; bool received=false; };
    auto state=std::make_shared<State>();
    std::function<void(const gz::msgs::StringMsg&)> callback = [state,value](const auto &request) {
      std::lock_guard<std::mutex> lock(state->mutex);
      if (request.data()==value) { state->received=true; state->cv.notify_all(); }
    };
    if (!node.Advertise(topic,callback)) return 9;
    std::cout << "READY" << std::endl;
    std::unique_lock<std::mutex> lock(state->mutex);
    if (!state->cv.wait_for(lock,std::chrono::seconds(8),[&]{return state->received;})) return 10;
  } else if (mode == "request-oneway") {
    gz::msgs::StringMsg request; request.set_data(value);
    std::cout << "READY" << std::endl;
    if (!node.Request(topic,request)) return 11;
    // Native one-way Request returns before discovery. Keep the process alive
    // until its bounded test window ends so pending discovery can deliver it.
    std::this_thread::sleep_for(std::chrono::seconds(3));
  } else if (mode == "request") {
    gz::msgs::StringMsg request,response; request.set_data(value);
    bool result=false;
    std::cout << "READY" << std::endl;
    if (!node.Request(topic,request,7000,response,result) || !result || response.data()!=value+"-rust") return 7;
  } else return 8;
}
