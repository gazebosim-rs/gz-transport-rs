// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Differential peer for pinned Gazebo Transport; no replacement logic is linked.
// Subscriber-only assertion ports: Copyright (C) 2014 Open Source Robotics Foundation.
#include <gz/transport/Node.hh>
#include <gz/transport/NodeShared.hh>
#include <gz/transport/TopicUtils.hh>
#include <gz/transport/TopicStorage.hh>
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
#include <map>

template<typename Publisher>
std::size_t Count(const gz::transport::TopicStorage<Publisher> &storage,
                  const std::string &topic) {
  std::map<std::string,std::vector<Publisher>> entries;
  if (!storage.Publishers(topic,entries)) return 0;
  std::size_t count=0;
  for (const auto &entry : entries) count+=entry.second.size();
  return count;
}

int main(int argc, char **argv) {
  if (argc != 4) return 2;
  const std::string mode=argv[1], topic=argv[2], value=argv[3];
  if (mode == "peer-storage") {
    using namespace gz::transport;
    TopicStorage<MessagePublisher> messages;
    TopicStorage<ServicePublisher> services;
    MessagePublisher publication(topic,"tcp://127.0.0.1:45000","","remote-process","remote-node",value,AdvertiseMessageOptions());
    ServicePublisher service(topic,"tcp://127.0.0.1:45001","service","remote-process","remote-node",value,value,AdvertiseServiceOptions());
    if (!messages.AddPublisher(publication) || !services.AddPublisher(service)) return 16;
    std::cout << Count(messages,topic)+Count(services,topic) << ' ';
    auto other=publication; other.SetAddr("tcp://127.0.0.1:45002");
    if (!messages.AddPublisher(other)) return 17;
    std::cout << Count(messages,topic)+Count(services,topic) << ' ';
    auto duplicate=publication; duplicate.SetMsgTypeName("changed-descriptor");
    if (messages.AddPublisher(duplicate)) return 18;
    std::map<std::string,std::vector<MessagePublisher>> entries;
    if (!messages.Publishers(topic,entries)) return 19;
    for (const auto &entry : entries) for (const auto &p : entry.second)
      if (p.MsgTypeName()!=value) return 20;
    std::cout << Count(messages,topic)+Count(services,topic) << ' ';
    if (!messages.DelPublisherByNode(topic,"remote-process","remote-node")) return 21;
    std::cout << Count(messages,topic)+Count(services,topic) << ' ';
    if (!services.DelPublisherByNode(topic,"remote-process","remote-node")) return 22;
    std::cout << Count(messages,topic)+Count(services,topic) << std::endl;
    return 0;
  }
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
  } else if (mode == "dual") {
    struct State { std::mutex mutex; std::condition_variable cv; bool requested=false; };
    auto state=std::make_shared<State>();
    std::function<bool(const gz::msgs::StringMsg&,gz::msgs::StringMsg&)> callback = [state,value](const auto &request,auto &response) {
      response.set_data(request.data()+value);
      std::lock_guard<std::mutex> lock(state->mutex);
      state->requested=true; state->cv.notify_all(); return true;
    };
    auto publisher=node.Advertise<gz::msgs::StringMsg>(topic);
    if (!publisher || !node.Advertise(topic,callback)) return 23;
    std::cout << "READY" << std::endl;
    const auto deadline=std::chrono::steady_clock::now()+std::chrono::seconds(8);
    gz::msgs::StringMsg message; message.set_data(value);
    std::unique_lock<std::mutex> lock(state->mutex);
    while (!state->requested) {
      lock.unlock(); publisher.Publish(message); lock.lock();
      state->cv.wait_for(lock,std::chrono::milliseconds(10),[&]{return state->requested;});
      if (std::chrono::steady_clock::now()>=deadline) return 24;
    }
    lock.unlock();
    // The Rust side signals only after the actual RPC reply arrives.
    std::string command;
    if (!std::getline(std::cin,command) || command!="QUIT") return 25;
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
