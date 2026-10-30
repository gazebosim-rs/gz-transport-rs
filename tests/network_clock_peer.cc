// Copyright (C) 2018, 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// All clock values and serialized messages below come from the actual SDK.
#include <gz/msgs/clock.pb.h>
#include <gz/transport/Clock.hh>
#include <gz/transport/Node.hh>

#include <chrono>
#include <cstdint>
#include <functional>
#include <iomanip>
#include <iostream>
#include <memory>
#include <mutex>
#include <sstream>
#include <string>
#include <thread>

static_assert(sizeof(int) == 4, "qualified SetTime seconds temporary");

struct Observed
{
  std::mutex mutex;
  std::string bytes;
  std::uint64_t count = 0;
};

std::string Hex(const std::string &_bytes)
{
  if (_bytes.empty())
    return "-";
  std::ostringstream output;
  for (unsigned char byte : _bytes)
    output << std::hex << std::setw(2) << std::setfill('0') << int(byte);
  return output.str();
}

bool Unhex(const std::string &_hex, std::string &_bytes)
{
  if (_hex == "-")
    return true;
  if (_hex.size() % 2)
    return false;
  for (std::size_t i = 0; i < _hex.size(); i += 2)
  {
    const auto nibble = [](char c) -> int
    {
      if (c >= '0' && c <= '9') return c - '0';
      if (c >= 'a' && c <= 'f') return c - 'a' + 10;
      return -1;
    };
    int high = nibble(_hex[i]);
    int low = nibble(_hex[i + 1]);
    if (high < 0 || low < 0)
      return false;
    _bytes.push_back(static_cast<char>(high * 16 + low));
  }
  return true;
}

int main(int argc, char **argv)
{
  if (argc != 3)
    return 2;
  using Base = gz::transport::NetworkClock::TimeBase;
  const std::string baseName = argv[1];
  Base base;
  if (baseName == "sim") base = Base::SIM;
  else if (baseName == "real") base = Base::REAL;
  else if (baseName == "system") base = Base::SYS;
  else return 2;

  auto observed = std::make_shared<Observed>();
  gz::transport::NetworkClock clock(argv[2], base);
  gz::transport::Node node;
  std::function<void(const gz::msgs::Clock &)> callback =
      [observed](const gz::msgs::Clock &_message)
      {
        std::lock_guard<std::mutex> lock(observed->mutex);
        observed->bytes = _message.SerializeAsString();
        ++observed->count;
      };
  if (!node.Subscribe(argv[2], callback))
    return 3;
  auto publisher = node.Advertise<gz::msgs::Clock>(argv[2]);
  if (!publisher)
    return 4;
  std::cout << "READY" << std::endl;

  std::string command;
  while (std::getline(std::cin, command))
  {
    std::istringstream input(command);
    std::string operation;
    input >> operation;
    if (operation == "QUIT")
      return 0;
    if (operation == "SET")
    {
      std::int64_t value;
      if (!(input >> value))
        return 5;
      clock.SetTime(std::chrono::nanoseconds(value));
    }
    else if (operation == "MSG")
    {
      std::string hex, bytes;
      input >> hex;
      gz::msgs::Clock message;
      if (!Unhex(hex, bytes) || !message.ParseFromString(bytes) ||
          !publisher.Publish(message))
        return 6;
    }
    else if (operation == "STAMP")
    {
      std::int64_t seconds;
      std::int32_t nanoseconds;
      if (!(input >> seconds >> nanoseconds))
        return 8;
      // Fixtures use only inputs whose native signed arithmetic is defined.
      // Overflow rejection is a Rust-only safety control, not a native oracle.
      gz::msgs::Clock message;
      auto stamp = base == Base::SIM ? message.mutable_sim() :
          (base == Base::REAL ? message.mutable_real() : message.mutable_system());
      stamp->set_sec(seconds);
      stamp->set_nsec(nanoseconds);
      if (!publisher.Publish(message))
        return 9;
    }
    else if (operation != "GET")
      return 7;

    if (operation != "GET")
      std::this_thread::sleep_for(std::chrono::milliseconds(100));
    std::lock_guard<std::mutex> lock(observed->mutex);
    std::cout << "STATE " << clock.IsReady() << ' ' << clock.Time().count()
              << ' ' << observed->count << ' ' << Hex(observed->bytes)
              << std::endl;
  }
}
