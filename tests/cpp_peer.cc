// SPDX-License-Identifier: Apache-2.0
// Differential peer for pinned Gazebo Transport; no replacement logic is linked.
#include <gz/transport/Node.hh>
#include <gz/msgs/stringmsg.pb.h>
#include <chrono>
#include <condition_variable>
#include <iostream>
#include <mutex>
#include <thread>

int main(int argc, char **argv) {
  if (argc != 4) return 2;
  const std::string mode=argv[1], topic=argv[2], value=argv[3];
  gz::transport::Node node;
  if (mode == "publish") {
    auto publisher=node.Advertise<gz::msgs::StringMsg>(topic);
    if (!publisher) return 3;
    std::cout << "READY" << std::endl;
    gz::msgs::StringMsg message; message.set_data(value);
    for (int i=0;i<150;++i) {
      publisher.Publish(message);
      std::this_thread::sleep_for(std::chrono::milliseconds(20));
    }
  } else if (mode == "subscribe") {
    std::mutex mutex; std::condition_variable cv; bool received=false;
    std::function<void(const gz::msgs::StringMsg&)> callback = [&](const auto &message) {
      std::lock_guard<std::mutex> lock(mutex);
      if (message.data() == value) { received=true; cv.notify_all(); }
    };
    if (!node.Subscribe(topic,callback)) return 4;
    std::cout << "READY" << std::endl;
    std::unique_lock<std::mutex> lock(mutex);
    if (!cv.wait_for(lock,std::chrono::seconds(8),[&]{return received;})) return 5;
  } else if (mode == "serve") {
    std::function<bool(const gz::msgs::StringMsg&,gz::msgs::StringMsg&)> callback = [&](const auto &request,auto &response) {
      response.set_data(request.data()+value); return true;
    };
    if (!node.Advertise(topic,callback)) return 6;
    std::cout << "READY" << std::endl;
    std::this_thread::sleep_for(std::chrono::seconds(8));
  } else if (mode == "request") {
    gz::msgs::StringMsg request,response; request.set_data(value);
    bool result=false;
    std::cout << "READY" << std::endl;
    if (!node.Request(topic,request,7000,response,result) || !result || response.data()!=value+"-rust") return 7;
  } else return 8;
}
