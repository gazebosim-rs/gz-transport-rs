// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
#include <gz/transport/NetUtils.hh>
#include <iostream>

int main() {
  std::cout << "HOST " << gz::transport::determineHost() << '\n';
  std::cout << "INTERFACES";
  for (const auto &address : gz::transport::determineInterfaces())
    std::cout << ' ' << address;
  std::cout << '\n';
}
