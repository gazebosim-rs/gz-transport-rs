// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Pure observations of the pinned OSRF 2020 TopicStatistics implementation.
#include <gz/transport/TopicStatistics.hh>
#include <cstdint>
#include <cstring>
#include <iomanip>
#include <iostream>
#include <map>
#include <memory>
#include <sstream>
#include <string>

static std::uint64_t bits(double value)
{
  std::uint64_t result;
  std::memcpy(&result, &value, sizeof result);
  return result;
}
static void sample(const gz::transport::Statistics &value)
{
  std::cout << ' ' << value.Count() << ' ' << std::hex << std::setfill('0')
    << std::setw(16) << bits(value.Avg()) << ' '
    << std::setw(16) << bits(value.StdDev()) << ' '
    << std::setw(16) << bits(value.Min()) << ' '
    << std::setw(16) << bits(value.Max()) << std::dec;
}
int main()
{
  using gz::transport::TopicStatistics;
  auto stats = std::make_unique<TopicStatistics>();
  std::map<std::string, std::unique_ptr<TopicStatistics>> saved;
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
      stats = std::make_unique<TopicStatistics>();
      name = value;
      step = 0;
    }
    else if (command == "SAVE")
      saved[value] = std::make_unique<TopicStatistics>(*stats);
    else if (command == "LOAD")
    {
      if (saved.count(value) == 0)
        return 2;
      stats = std::make_unique<TopicStatistics>(*saved.at(value));
      name = value;
      step = stats->PublicationStatistics().Count();
    }
    else if (command == "ADD")
    {
      // The explicit Rust reception time is intentionally not injected into the
      // SDK's hidden clock. Actual native reception/age getters are still output.
      std::string sender;
      std::uint64_t stamp, sequence;
      if (!(input >> sender >> stamp >> sequence))
        return 2;
      std::string bytes;
      if (sender != "-")
      {
        if (sender.size() % 2 != 0)
          return 2;
        for (std::size_t i = 0; i < sender.size(); i += 2)
          bytes.push_back(static_cast<char>(std::stoul(sender.substr(i, 2), nullptr, 16)));
      }
      stats->Update(bytes, stamp, sequence);
      ++step;
    }
    else
      return 2;
    std::cout << "T " << name << ' ' << step << ' ' << stats->DroppedMsgCount();
    sample(stats->PublicationStatistics());
    sample(stats->ReceptionStatistics());
    sample(stats->AgeStatistics());
    std::cout << '\n';
  }
}
