// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Native Statistics implementation: OSRF 2020, Apache-2.0, pinned SDK.
#include <gz/transport/TopicStatistics.hh>
#include <cstdint>
#include <cstring>
#include <iomanip>
#include <iostream>
#include <sstream>
#include <string>

static std::uint64_t bits(double value)
{
  std::uint64_t result;
  std::memcpy(&result, &value, sizeof result);
  return result;
}

int main()
{
  gz::transport::Statistics stats;
  std::string line, name;
  std::uint64_t step = 0;
  while (std::getline(std::cin, line))
  {
    std::istringstream input(line);
    std::string command, value;
    if (!(input >> command >> value))
      return 2;
    if (command == "RESET")
    {
      stats = gz::transport::Statistics();
      name = value;
      step = 0;
      value = "-";
    }
    else if (command == "ADD")
    {
      std::uint64_t raw;
      std::istringstream encoded(value);
      encoded >> std::hex >> raw;
      if (!encoded || name.empty())
        return 2;
      double sample;
      std::memcpy(&sample, &raw, sizeof sample);
      stats.Update(sample);
      ++step;
    }
    else
      return 2;
    std::cout << "S " << name << ' ' << step << ' ' << value << ' '
      << stats.Count() << ' ' << std::hex << std::setfill('0')
      << std::setw(16) << bits(stats.Avg()) << ' '
      << std::setw(16) << bits(stats.StdDev()) << ' '
      << std::setw(16) << bits(stats.Min()) << ' '
      << std::setw(16) << bits(stats.Max()) << std::dec << '\n';
  }
}
