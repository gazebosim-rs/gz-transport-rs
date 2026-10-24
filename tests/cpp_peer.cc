// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Differential peer for pinned Gazebo Transport; no replacement logic is linked.
#include <gz/transport/Node.hh>
#include <gz/transport/NodeShared.hh>
#include <gz/msgs/stringmsg.pb.h>
#include <gz/msgs/empty.pb.h>
#include <chrono>
#include <condition_variable>
#include <iostream>
#include <mutex>
#include <memory>
#include <google/protobuf/unknown_field_set.h>
#include <thread>

int main(int argc, char **argv) {
  if (argc != 4) return 2;
  const std::string mode=argv[1], topic=argv[2], value=argv[3];
  gz::transport::Node node;
  if (mode == "config") {
    gz::transport::NodeOptions options;
    auto shared=gz::transport::NodeShared::Instance();
    std::cout << "CONFIG " << options.Partition() << ' ' << shared->msgDiscPort << ' ' << shared->srvDiscPort << ' ' << shared->SndHwm() << ' ' << shared->RcvHwm() << std::endl;
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
  } else if (mode == "serve") {
    std::function<bool(const gz::msgs::StringMsg&,gz::msgs::StringMsg&)> callback = [value](const auto &request,auto &response) {
      response.set_data(request.data()+value); return true;
    };
    if (!node.Advertise(topic,callback)) return 6;
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
