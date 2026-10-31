// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Pure conversion plus captured actual constructor storage; no network sends.
#include <gz/transport/Helpers.hh>
#include "Discovery.hh"
#include <arpa/inet.h>
#include <algorithm>
#include <iomanip>
#include <fstream>
#include <iostream>
#include <string>
#include <vector>

int main(int argc, char **argv)
{
  if (argc == 3 && std::string(argv[1]) == "batch")
  {
    std::ifstream input(argv[2]);
    if (!input)
      return 3;
    std::string line;
    while (std::getline(input, line))
    {
      if (line.size() % 2)
        return 4;
      std::string token;
      for (size_t i = 0; i < line.size(); i += 2)
        token.push_back(static_cast<char>(std::stoul(line.substr(i, 2), nullptr, 16)));
      const in_addr address{inet_addr(token.c_str())};
      char normalized[INET_ADDRSTRLEN];
      if (!inet_ntop(AF_INET, &address, normalized, sizeof(normalized)))
        return 5;
      std::cout << normalized << '\n';
    }
    return 0;
  }
  if (argc == 2 && std::string(argv[1]) == "constructor")
  {
    // LD_PRELOAD errors can be nonfatal: fail closed before creating a socket.
    const char *ready = std::getenv("GZ_LEGACY_CAPTURE_READY");
    if (!ready || std::string(ready) != "sendto-intercepted")
      return 6;
    gz::transport::Discovery<gz::transport::MessagePublisher> messages("legacy-m", "239.255.0.7", 0);
    gz::transport::Discovery<gz::transport::ServicePublisher> services("legacy-s", "239.255.0.7", 0);
    for (const auto &address : messages.RelayAddresses())
      std::cout << "message " << address << '\n';
    for (const auto &address : services.RelayAddresses())
      std::cout << "service " << address << '\n';
    return 0;
  }
  std::string input;
  const bool present = gz::transport::env("GZ_RELAY", input);
  std::cout << "ENV " << present << ' ' << input.size() << '\n';
  std::vector<in_addr_t> addresses;
  if (present && !input.empty())
  {
    for (const auto &part : gz::transport::split(input, ':'))
    {
      const auto address = inet_addr(part.c_str());
      const bool duplicate =
          std::find(addresses.begin(), addresses.end(), address) != addresses.end();
      const in_addr native{address};
      char normalized[INET_ADDRSTRLEN];
      if (!inet_ntop(AF_INET, &native, normalized, sizeof(normalized)))
        return 1;
      std::cout << "TOKEN ";
      for (const unsigned char byte : part)
        std::cout << std::hex << std::setw(2) << std::setfill('0')
                  << static_cast<unsigned int>(byte);
      std::cout << std::dec << ' ' << normalized << ' ' << duplicate << ' '
                << (address == INADDR_NONE) << '\n';
      if (!duplicate)
        addresses.push_back(address);
    }
  }
  std::cout << "FINAL";
  for (const auto address : addresses)
  {
    const in_addr native{address};
    char normalized[INET_ADDRSTRLEN];
    if (!inet_ntop(AF_INET, &native, normalized, sizeof(normalized)))
      return 2;
    std::cout << ' ' << normalized;
  }
  std::cout << '\n';
}
