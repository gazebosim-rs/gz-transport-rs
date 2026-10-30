// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Pure controlled-clock TopicStatistics::FillMessage; no Nodes or sockets.
#include <gz/transport/TopicStatistics.hh>
#include <gz/msgs/statistic.pb.h>
#include <cassert>
#include <chrono>
#include <cstdint>
#include <cstring>
#include <dlfcn.h>
#include <iomanip>
#include <iostream>
#include <string>
#include <time.h>

static std::uint64_t bits(double value)
{
  std::uint64_t raw;
  std::memcpy(&raw, &value, sizeof raw);
  return raw;
}

static void stat(const gz::msgs::Statistic &value)
{
  std::cout << value.type() << ' ' << value.name() << ' '
    << std::hex << std::setfill('0') << std::setw(16) << bits(value.value())
    << std::dec << '\n';
}

static void output(const char *name, const gz::msgs::Metric &metric)
{
  std::cout << "CASE " << name << ' ' << metric.has_header() << ' '
    << metric.unit() << ' ' << metric.statistics_size() << ' '
    << metric.statistics_groups_size() << '\n';
  for (const auto &value : metric.statistics())
  {
    std::cout << "STAT ";
    stat(value);
  }
  for (const auto &group : metric.statistics_groups())
  {
    std::cout << "GROUP " << group.name() << ' ' << group.statistics_size()
      << '\n';
    for (const auto &value : group.statistics())
    {
      std::cout << "STAT ";
      stat(value);
    }
  }
  std::cout << "WIRE ";
  for (const unsigned char byte : metric.SerializeAsString())
    std::cout << std::hex << std::setfill('0') << std::setw(2) << unsigned(byte);
  std::cout << std::dec << '\n';
}

int main()
{
  auto setClock = reinterpret_cast<int(*)(std::uint64_t)>(
    dlsym(RTLD_DEFAULT, "gz_test_monotonic_ms"));
  assert(setClock);
  auto at = [&](std::uint64_t value)
  {
    assert(setClock(value) == 0);
    const auto observed = std::chrono::duration_cast<std::chrono::milliseconds>(
      std::chrono::steady_clock::now().time_since_epoch()).count();
    assert(observed == static_cast<std::int64_t>(value));
    timespec real;
    assert(clock_gettime(CLOCK_REALTIME, &real) == 0 && real.tv_sec > 1000000000);
  };
  gz::transport::TopicStatistics stats;
  gz::msgs::Metric empty;
  stats.FillMessage(empty);
  output("controlled_empty", empty);
  at(20);
  stats.Update("foo", 10, 0);
  at(30);
  stats.Update("foo", 15, 1);
  assert(stats.ReceptionStatistics().Avg() == 10);
  assert(stats.AgeStatistics().Avg() == 15);
  at(38);
  stats.Update("foo", 18, 3);
  assert(stats.PublicationStatistics().Avg() == 4);
  assert(stats.PublicationStatistics().StdDev() == 1);
  assert(stats.ReceptionStatistics().Avg() == 9);
  assert(stats.ReceptionStatistics().StdDev() == 1);
  assert(stats.AgeStatistics().Avg() == 17.5);
  assert(stats.AgeStatistics().StdDev() == 2.5);
  gz::msgs::Metric populated;
  stats.FillMessage(populated);
  output("controlled_populated", populated);
  gz::msgs::Metric prefix;
  prefix.mutable_header()->mutable_stamp()->set_sec(17);
  prefix.mutable_header()->mutable_stamp()->set_nsec(18);
  prefix.set_unit("seconds");
  auto *keep = prefix.add_statistics();
  keep->set_name("keep");
  keep->set_type(gz::msgs::Statistic::AVERAGE);
  keep->set_value(42);
  auto *group = prefix.add_statistics_groups();
  group->set_name("keep_group");
  auto *prior = group->add_statistics();
  prior->set_name("pre");
  prior->set_type(gz::msgs::Statistic::MAXIMUM);
  prior->set_value(7);
  stats.FillMessage(prefix);
  output("controlled_prefix", prefix);
  stats.FillMessage(populated);
  output("controlled_repeated", populated);
}
