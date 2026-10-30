// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Native-only control probe; data records come from an actual admitted service.
#include <gz/transport/Node.hh>
#include <gz/transport/Uuid.hh>
#include <gz/msgs/int32.pb.h>
#include "Discovery.hh"
#include <atomic>
#include <functional>
#include <iostream>
#include <string>
#include <vector>
int main(int argc,char **argv) {
  using namespace gz;
  if(argc!=4)return 1;
  const std::string mode=argv[1],topic=argv[2];
  const auto scope=mode=="all"?transport::Scope_t::ALL:mode=="host"?transport::Scope_t::HOST:transport::Scope_t::PROCESS;
  if(mode!="all" && mode!="host" && mode!="process")return 2;
  transport::Node owner,caller;
  transport::AdvertiseServiceOptions options;options.SetScope(scope);
  std::atomic<int> calls{0};
  std::function<bool(const msgs::Int32&,msgs::Int32&)> callback=[&](const auto &request,auto &reply){++calls;reply.set_data(request.data());return true;};
  if(!owner.Advertise(topic,callback,options))return 3;
  std::vector<transport::ServicePublisher> providers;
  if(!owner.ServiceInfo(topic,providers) || providers.size()!=1)return 4;
  const auto publisher=providers.front();
  const auto port=std::stoi(argv[3]);
  transport::Discovery<transport::ServicePublisher> control(publisher.PUuid(),"239.255.0.7",port);
  transport::Discovery<transport::ServicePublisher> requester(transport::Uuid().ToString(),"239.255.0.7",port);
  // A first empty heartbeat establishes a known interval boundary. Neither the
  // native defaults nor their original tests are changed by this control setup.
  control.SetHeartbeatInterval(2000);requester.SetHeartbeatInterval(2000);
  std::cout<<publisher.Topic()<<'\n'<<publisher.Addr()<<'\n'<<publisher.PUuid()<<'\n'<<publisher.NUuid()<<'\n'<<static_cast<int>(publisher.Options().Scope())<<'\n'<<publisher.SocketId()<<'\n'<<publisher.ReqTypeName()<<'\n'<<publisher.RepTypeName()<<'\n';
  control.Start();requester.Start();std::cout<<"READY"<<std::endl;
  std::string command;
  while(std::getline(std::cin,command)) {
    if(command=="ADVERTISE") {if(!control.Advertise(publisher))return 5;std::cout<<"ADVERTISED"<<std::endl;}
    else if(command=="REQUEST") {if(!requester.Discover(publisher.Topic()))return 6;std::cout<<"REQUESTED"<<std::endl;}
    else if(command=="UNADVERTISE") {if(!control.Unadvertise(publisher.Topic(),publisher.NUuid()))return 7;std::cout<<"UNADVERTISED"<<std::endl;}
    else if(command=="CALL") {
      msgs::Int32 request,reply;request.set_data(5);bool result=false;
      const bool returned=caller.Request(topic,request,300,reply,result);
      if(!returned || !result || reply.data()!=5 || calls!=1)return 8;
      std::cout<<"CALLED "<<returned<<' '<<result<<' '<<reply.data()<<' '<<calls<<std::endl;
    } else if(command=="QUIT")return 0;
    else return 9;
  }
  return 10;
}
