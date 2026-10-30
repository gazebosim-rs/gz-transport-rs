// Copyright (C) 2018, 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Pure actual SDK WallClock: no Node or NetworkClock object is constructed.
#include <gz/transport/Clock.hh>
#include <chrono>
#include <cstdint>
#include <dlfcn.h>
#include <iostream>
int main(int argc, char **argv) {
  using Count = unsigned (*)(unsigned);
  using Step = void (*)(std::int64_t);
  auto count = reinterpret_cast<Count>(dlsym(RTLD_DEFAULT, "gz_wall_count"));
  auto step = reinterpret_cast<Step>(dlsym(RTLD_DEFAULT, "gz_wall_realtime"));
  if (!count || !step || argc != 2) return 2;
  using namespace std::chrono;
  auto realBefore = duration_cast<nanoseconds>(system_clock::now().time_since_epoch()).count();
  const auto *clock = gz::transport::WallClock::Instance();
  if (!clock) return 3;
  const auto ready = clock->IsReady();
  const auto first = clock->Time().count();
  step(std::stoll(argv[1]));
  const auto last = clock->Time().count();
  auto realAfter = duration_cast<nanoseconds>(system_clock::now().time_since_epoch()).count();
  std::cout << ready << ' ' << first << ' ' << last << ' ' << realBefore << ' ' << realAfter << ' '
            << count(0) << ' ' << count(1) << ' ' << count(2) << ' '
            << (clock == gz::transport::WallClock::Instance()) << '\n';
}
