// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Differential peer for pinned Gazebo Transport; no replacement logic is linked.
// Native assertion controls retain the original Node_TEST.cc notice.
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
#include <gz/transport/Node.hh>
#include <gz/transport/MessageInfo.hh>
#include <gz/transport/NodeShared.hh>
#include <gz/transport/TopicUtils.hh>
#include <gz/transport/TopicStorage.hh>
#include <gz/msgs/stringmsg.pb.h>
#include <gz/msgs/int32.pb.h>
#include <gz/msgs/vector3d.pb.h>
#include <gz/msgs/bytes.pb.h>
#include <gz/msgs/empty.pb.h>
#include <gz/msgs/discovery.pb.h>
#include <atomic>
#include <chrono>
#include <optional>
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

struct MetadataState {
  std::mutex mutex;
  std::condition_variable cv;
  bool received=false, valid=false, intra=false;
};
static auto metadata_callback(const std::shared_ptr<MetadataState> &state,
                              std::string topic,std::string partition) {
  return std::function<void(const gz::msgs::Int32&,const gz::transport::MessageInfo&)>(
    [state,topic,partition](const auto &message,const auto &info) {
      std::lock_guard<std::mutex> guard(state->mutex);
      state->valid=message.data()==5 && info.Topic()==topic && info.Partition()==partition && info.Type()=="gz.msgs.Int32";
      state->intra=info.IntraProcess();state->received=true;state->cv.notify_all();
    });
}
static std::optional<bool> wait_metadata(const std::shared_ptr<MetadataState> &state) {
  std::unique_lock<std::mutex> lock(state->mutex);
  if (!state->cv.wait_for(lock,std::chrono::seconds(5),[&]{return state->received;}) || !state->valid) return std::nullopt;
  return state->intra;
}
int main(int argc, char **argv) {
  if (argc != 4) return 2;
  const std::string mode=argv[1], topic=argv[2], value=argv[3];
  if (mode == "message-info-decompose") {
    gz::transport::MessageInfo info;
    if (!info.SetTopicAndPartition(topic)) return 107;
    std::cout << info.Topic() << '\n' << info.Partition() << '\n';
    return info.Partition()==value ? 0 : 108;
  }
  if (mode == "decode-discovery") {
    const std::string input(std::istreambuf_iterator<char>(std::cin), {});
    gz::msgs::Discovery message;
    if (!message.ParseFromString(input)) return 30;
    message.DiscardUnknownFields();
    if (!message.SerializeToOstream(&std::cout)) return 31;
    return 0;
  }
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
  if (mode == "remap-options") {
    gz::transport::NodeOptions options;
    const bool added=options.AddTopicRemap(topic,value);
    const bool duplicate=options.AddTopicRemap(topic,value);
    std::string output="sentinel";
    const bool found=options.TopicRemap(topic,output);
    std::cout << added << ' ' << duplicate << ' ' << found << '\n' << output;
    return 0;
  }
  if (mode == "remap-service-lifecycle") {
    gz::transport::NodeOptions options;
    if (!options.AddTopicRemap(topic,value) || !options.AddTopicRemap(value,"/third")) return 35;
    gz::transport::Node mapped(options);
    std::function<bool(const gz::msgs::StringMsg&,gz::msgs::StringMsg&)> cb=[](const auto&,auto&){return true;};
    if (!mapped.Advertise(topic,cb) || mapped.AdvertisedServices()!=std::vector<std::string>{value}) return 36;
    std::string request,response;
    using Resolution=gz::transport::Node::ServiceTypeResolution;
    if (mapped.ResolveServiceTypes(topic,request,response)!=Resolution::kNoProviders ||
        mapped.ResolveServiceTypes(value,request,response)!=Resolution::kResolved ||
        request!="gz.msgs.StringMsg" || response!="gz.msgs.StringMsg") return 37;
    if (!mapped.UnadvertiseSrv(topic) || mapped.AdvertisedServices()!=std::vector<std::string>{value}) return 38;
    if (!mapped.UnadvertiseSrv(value) || !mapped.AdvertisedServices().empty()) return 39;
    std::cout << "READY" << std::endl;
    return 0;
  }
  if (mode == "local-inventory") {
    gz::transport::NodeOptions options;
    if (!options.SetNameSpace("ns") || !options.AddTopicRemap(topic,value)) return 60;
    gz::transport::Node local(options);
    if (!local.AdvertisedTopics().empty() || !local.SubscribedTopics().empty()) return 61;
    auto publication=local.Advertise<gz::msgs::StringMsg>(topic);
    std::string qualified;
    if (!publication || !gz::transport::TopicUtils::FullyQualifiedName(options.Partition(),options.NameSpace(),value,qualified)) return 62;
    const auto actual=qualified.substr(qualified.find_last_of('@')+1);
    if (local.AdvertisedTopics()!=std::vector<std::string>{actual}) return 63;
    std::function<void(const gz::msgs::StringMsg&)> cb=[](const auto&){};
    if (!local.Subscribe(topic,cb) || !local.Subscribe(topic,cb) || local.SubscribedTopics()!=std::vector<std::string>{actual}) return 64;
    if (!local.Unsubscribe(actual) || !local.SubscribedTopics().empty()) return 65;
    publication=gz::transport::Node::Publisher();
    if (!local.AdvertisedTopics().empty()) return 66;
    std::cout << "READY" << std::endl;
    return 0;
  }
  gz::transport::Node node;
  if (mode == "scope-observer") {
    struct State { std::atomic<unsigned> received{0}; std::atomic<bool> valid{true}; };
    auto state=std::make_shared<State>();
    std::function<void(const gz::msgs::StringMsg&)> callback=[state,value](const auto &message) {
      if (message.data()!=value) state->valid.store(false);
      state->received.fetch_add(1);
    };
    if (!node.Subscribe(topic,callback)) return 114;
    std::cout << "READY" << std::endl;
    std::string command;
    while (std::getline(std::cin,command)) {
      if (command=="QUIT") return 0;
      if (command!="STATE") return 115;
      std::vector<gz::transport::MessagePublisher> publishers,subscribers;
      if (!node.TopicInfo(topic,publishers,subscribers)) return 116;
      const int scope=publishers.empty()?-1:static_cast<int>(publishers.front().Options().Scope());
      std::cout << "STATE " << state->received.load() << ' ' << state->valid.load() << ' '
                << publishers.size() << ' ' << scope << std::endl;
    }
    return 117;
  }
  if (mode == "message-info-two-nodes" || mode == "message-info-subscribe") {
    auto state=std::make_shared<MetadataState>();
    std::string qualified;
    gz::transport::MessageInfo expected;
    if (!gz::transport::TopicUtils::FullyQualifiedName(node.Options().Partition(),node.Options().NameSpace(),topic,qualified) || !expected.SetTopicAndPartition(qualified)) return 100;
    if (mode == "message-info-two-nodes") {
      gz::transport::Node other;
      auto publisher=node.Advertise<gz::msgs::Int32>(topic);
      if (!publisher || !other.Subscribe(topic,metadata_callback(state,expected.Topic(),expected.Partition())) || !publisher.HasConnections()) return 101;
      gz::msgs::Int32 message;message.set_data(5);
      if (!publisher.Publish(message)) return 102;
      auto intra=wait_metadata(state);if (!intra) return 103;
      std::cout << "READY" << std::endl << (*intra?"true":"false") << std::endl;
    } else {
      if (!node.Subscribe(topic,metadata_callback(state,expected.Topic(),expected.Partition()))) return 104;
      std::cout << "READY" << std::endl;
      auto intra=wait_metadata(state);if (!intra || *intra) return 105;
    }
  } else if (mode == "message-info-publisher") {
    auto publisher=node.Advertise<gz::msgs::Int32>(topic);if(!publisher)return 106;
    std::atomic<bool> running{true};std::atomic<int> current{5};
    std::thread publishing([&]{while(running.load()){gz::msgs::Int32 message;message.set_data(current.load());publisher.Publish(message);std::this_thread::sleep_for(std::chrono::milliseconds(20));}});
    std::cout << "READY" << std::endl;
    std::string command;
    while(std::getline(std::cin,command) && command!="QUIT") {if(command=="6")current.store(6);}
    running.store(false);publishing.join();
  } else if (mode == "unsubscribe-reference") {
    gz::transport::NodeOptions options;
    if (!options.AddTopicRemap(topic,value)) return 90;
    gz::transport::Node first(options),second(options);
    std::function<void(const gz::msgs::Int32&)> callback=[](const auto&){};
    if (!first.Subscribe(topic,callback) || !first.Subscribe(topic,callback) || !second.Subscribe(topic,callback)) return 91;
    const std::vector<std::string> expected{value};
    if (first.SubscribedTopics()!=expected || second.SubscribedTopics()!=expected) return 92;
    if (first.Unsubscribe("invalid topic") || !first.Unsubscribe("/absent")) return 93;
    if (first.SubscribedTopics()!=expected || !first.Unsubscribe(topic) || first.SubscribedTopics()!=expected) return 94;
    if (!first.Unsubscribe(value) || !first.SubscribedTopics().empty() || second.SubscribedTopics()!=expected) return 95;
    if (!first.Unsubscribe(value) || !first.Subscribe(topic,callback) || first.SubscribedTopics()!=expected) return 96;
    if (!first.Unsubscribe(value) || !second.Unsubscribe(value) || !first.SubscribedTopics().empty() || !second.SubscribedTopics().empty()) return 97;
    std::cout << "READY" << std::endl;
  } else if (mode == "publisher-membership") {
    auto publisher=node.Advertise(topic,value);
    if (!publisher || publisher.HasConnections()) return 40;
    std::function<void(const gz::msgs::StringMsg&)> cb=[](const auto&){};
    if (!node.Subscribe(topic,cb) || !publisher.HasConnections()) return 41;
    if (!node.Unsubscribe(topic) || publisher.HasConnections()) return 42;
    auto shared=gz::transport::NodeShared::Instance();
    std::string qualified;
    if (!gz::transport::TopicUtils::FullyQualifiedName(node.Options().Partition(),node.Options().NameSpace(),topic,qualified)) return 43;
    gz::transport::MessagePublisher remote(qualified,"tcp://127.0.0.1:45555","wrong","remote","subscriber",value,gz::transport::AdvertiseMessageOptions());
    shared->OnNewRegistration(remote);
    if (publisher.HasConnections()) return 44;
    remote.SetCtrl(shared->pUuid);
    shared->OnNewRegistration(remote);
    if (!publisher.HasConnections()) return 45;
    auto duplicate=remote; duplicate.SetMsgTypeName("changed");
    shared->OnNewRegistration(duplicate);
    if (!publisher.HasConnections()) return 46;
    auto wrong=remote; wrong.SetCtrl("wrong"); shared->OnEndRegistration(wrong);
    if (!publisher.HasConnections()) return 47;
    auto gone=remote; gone.SetTopic(""); gone.SetNUuid(""); shared->OnNewDisconnection(gone);
    if (!publisher.HasConnections()) return 48;
    auto end=remote; end.SetCtrl(""); shared->OnEndRegistration(end);
    if (publisher.HasConnections()) return 49;
    remote.SetPUuid(""); remote.SetNUuid(""); remote.SetMsgTypeName("google.protobuf.Message");
    shared->OnNewRegistration(remote);
    if (!publisher.HasConnections()) return 50;
    gz::msgs::Discovery projection;
    projection.set_type(gz::msgs::Discovery::END_CONNECTION);
    projection.mutable_sub()->set_topic(qualified);
    gz::transport::MessagePublisher projected;
    projected.SetFromDiscovery(projection); shared->OnEndRegistration(projected);
    if (publisher.HasConnections()) return 51;
    remote.SetTopic(""); shared->OnNewRegistration(remote);
    if (!shared->remoteSubscribers.HasTopic("")) return 52;
    projection.clear_sub(); projected=gz::transport::MessagePublisher();
    projected.SetFromDiscovery(projection); shared->OnEndRegistration(projected);
    if (shared->remoteSubscribers.HasTopic("")) return 53;
    remote.SetTopic(qualified); remote.SetPUuid("remote"); remote.SetNUuid("subscriber");
    shared->OnNewRegistration(remote);
    if (!publisher.HasConnections()) return 54;
    remote.FillDiscovery(projection); projection.mutable_pub()->mutable_srv_pub();
    projected.SetFromDiscovery(projection); shared->OnEndRegistration(projected);
    if (publisher.HasConnections()) return 55;
    std::cout << "READY" << std::endl;
  } else if (mode == "subscriber-membership") {
    struct State { std::mutex mutex; std::condition_variable cv; bool received=false; };
    auto state=std::make_shared<State>();
    std::function<void(const gz::msgs::StringMsg&)> cb=[state,value](const auto &message){
      if (message.data()!=value) return;
      std::lock_guard<std::mutex> lock(state->mutex); state->received=true; state->cv.notify_all();
    };
    if (!node.Subscribe(topic,cb)) return 56;
    std::cout << "READY" << std::endl;
    std::unique_lock<std::mutex> lock(state->mutex);
    if (!state->cv.wait_for(lock,std::chrono::seconds(10),[&]{return state->received;})) return 57;
    lock.unlock(); std::cout << "RECEIVED" << std::endl;
    std::string command;
    if (!std::getline(std::cin,command) || command!="DROP" || !node.Unsubscribe(topic)) return 58;
    std::cout << "DROPPED" << std::endl;
    if (!std::getline(std::cin,command) || command!="QUIT") return 59;
  } else if (mode == "inspect-service") {
    std::cout << "READY" << std::endl;
    std::string request, response;
    if (node.ResolveServiceTypes(topic, request, response, 5000) !=
        gz::transport::Node::ServiceTypeResolution::kResolved ||
        request != value || response != value) return 32;
    std::vector<gz::transport::ServicePublisher> providers;
    if (!node.ServiceInfo(topic, providers) || providers.empty()) return 33;
    for (const auto &provider : providers)
      if (provider.ReqTypeName() != value || provider.RepTypeName() != value) return 34;
  } else if (mode == "service-lifecycle") {
    if (!node.UnadvertiseSrv(topic) || !node.AdvertisedServices().empty()) return 26;
    std::function<bool(const gz::msgs::StringMsg&,gz::msgs::StringMsg&)> callback=[](const auto &,auto &){return true;};
    if (!node.Advertise(topic,callback)) return 27;
    const auto topics=node.AdvertisedServices();
    if (topics.size()!=1 || topics.front()!=value) return 28;
    if (node.UnadvertiseSrv("invalid service") || node.AdvertisedServices()!=topics) return 29;
    if (!node.UnadvertiseSrv(topic) || !node.AdvertisedServices().empty() || !node.UnadvertiseSrv(topic)) return 30;
    std::cout << "READY" << std::endl;
  } else if (mode == "config") {
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
  } else if (mode == "subscribe-recreate") {
    struct State { std::mutex mutex; std::condition_variable cv; bool before=false, after=false; };
    auto state=std::make_shared<State>();
    std::function<void(const gz::msgs::StringMsg&)> callback = [state,value](const auto &message) {
      std::lock_guard<std::mutex> lock(state->mutex);
      if (message.data()==value+"-before") state->before=true;
      if (state->before && message.data()==value+"-after") state->after=true;
      state->cv.notify_all();
    };
    if (!node.Subscribe(topic,callback)) return 23;
    std::cout << "READY" << std::endl;
    std::unique_lock<std::mutex> lock(state->mutex);
    if (!state->cv.wait_for(lock,std::chrono::seconds(8),[&]{return state->before;})) return 24;
    std::cout << "FIRST" << std::endl;
    if (!state->cv.wait_for(lock,std::chrono::seconds(8),[&]{return state->after;})) return 25;
    std::cout << "SECOND" << std::endl;
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
  } else if (mode == "list-subscriber") {
    std::function<void(const gz::msgs::StringMsg&)> callback = [](const auto &) {};
    if (!node.Subscribe(topic, callback)) return 88;
    std::cout << "READY" << std::endl;
    std::string command;
    if (!std::getline(std::cin, command) || command != "QUIT") return 89;
  } else if (mode == "list-vector3d-subscriber") {
    std::function<void(const gz::msgs::Vector3d&)> callback = [](const auto &) {};
    if (!node.Subscribe(topic, callback)) return 112;
    std::cout << "READY" << std::endl;
    std::string command;
    if (!std::getline(std::cin, command) || command != "QUIT") return 113;
  } else if (mode == "list-vector3d-publisher") {
    auto publisher=node.Advertise<gz::msgs::Vector3d>(topic);
    if (!publisher) return 110;
    std::cout << "READY" << std::endl;
    std::string command;
    if (!std::getline(std::cin, command) || command != "QUIT") return 111;
  } else if (mode == "list-two-publishers") {
    auto publisher=node.Advertise<gz::msgs::Vector3d>(topic);
    auto large=node.Advertise<gz::msgs::Bytes>("/large_msg");
    if (!publisher || !large) return 115;
    std::cout << "READY" << std::endl;
    std::string command;
    if (!std::getline(std::cin, command) || command != "QUIT") return 116;
  } else if (mode == "service-info-record-count") {
    if (value != "2") return 85;
    const auto deadline=std::chrono::steady_clock::now()+std::chrono::seconds(10);
    std::vector<gz::transport::ServicePublisher> providers;
    for (;;) {
      if (node.ServiceInfo(topic,providers) && providers.size()==2) break;
      if (std::chrono::steady_clock::now()>=deadline) return 86;
      std::this_thread::sleep_for(std::chrono::milliseconds(100));
    }
    for (const auto &provider:providers)
      if (provider.ReqTypeName()!="gz.msgs.Int32" || provider.RepTypeName()!="gz.msgs.Int32") return 87;
    std::cout << "READY" << std::endl;
  } else if (mode == "service-info-reference") {
    if (value == "ordinary") {
      std::function<bool(const gz::msgs::Int32&,gz::msgs::Int32&)> callback=[](const auto &request,auto &response){response=request;return true;};
      if (!node.Advertise(topic,callback)) return 80;
    } else if (value == "inputless") {
      std::function<bool(gz::msgs::Int32&)> callback=[](auto &response){response.set_data(5);return true;};
      if (!node.Advertise(topic,callback)) return 81;
    } else if (value == "outputless") {
      std::function<void(const gz::msgs::Int32&)> callback=[](const auto &){};
      if (!node.Advertise(topic,callback)) return 82;
    } else return 83;
    std::cout << "READY" << std::endl;
    std::string command;
    if (!std::getline(std::cin,command) || command!="QUIT") return 84;
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
