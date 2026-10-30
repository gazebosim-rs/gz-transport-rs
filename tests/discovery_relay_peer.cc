// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Native protobuf mutation and two-channel discovery controls.
#include <gz/msgs/discovery.pb.h>
#include <gz/transport/TopicUtils.hh>
#include "Discovery.hh"
#include <iostream>
#include <iterator>
#include <string>

int main(int argc, char **argv)
{
  if (argc == 5 && std::string(argv[1]) == "network")
  {
    using namespace gz::transport;
    Discovery<MessagePublisher> messages("native-message-relay", argv[2], std::stoi(argv[3]));
    Discovery<ServicePublisher> services("native-service-relay", argv[2], std::stoi(argv[4]));
    messages.SetHeartbeatInterval(60000);
    services.SetHeartbeatInterval(60000);
    messages.Start();
    services.Start();
    std::string topic;
    std::string service;
    if (!TopicUtils::FullyQualifiedName("relay", "", "/topic", topic) ||
        !TopicUtils::FullyQualifiedName("relay", "", "/service", service))
      return 8;
    std::cout << "READY" << std::endl;
    std::string command;
    while (std::getline(std::cin, command))
    {
      if (command == "RELAYS")
      {
        for (const auto &address : messages.RelayAddresses())
          std::cout << "message " << address << '\n';
        for (const auto &address : services.RelayAddresses())
          std::cout << "service " << address << '\n';
        std::cout << "END" << std::endl;
      }
      else if (command == "DISCOVER")
      {
        if (!messages.Discover(topic) || !services.Discover(service))
          return 5;
        std::cout << "DISCOVERED" << std::endl;
      }
      else if (command == "QUIT")
        return 0;
      else
        return 6;
    }
    return 7;
  }
  if (argc != 2)
    return 1;
  const std::string mode = argv[1];
  const std::string input(std::istreambuf_iterator<char>(std::cin), {});
  gz::msgs::Discovery message;
  if (!message.ParseFromString(input))
    return 2;
  if (mode == "unicast")
    message.mutable_flags()->set_relay(true);
  else if (mode == "multicast")
  {
    message.mutable_flags()->set_relay(false);
    message.mutable_flags()->set_no_relay(true);
  }
  else if (mode != "parse")
    return 3;
  return message.SerializeToOstream(&std::cout) ? 0 : 4;
}
