// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Test-only monotonic clock; never loaded by a Node or network process.
#include <atomic>
#include <cstdint>
#include <limits>
#include <sys/syscall.h>
#include <time.h>
#include <unistd.h>

static std::atomic<bool> enabled{false};
static std::atomic<std::uint64_t> milliseconds{0};

extern "C" int gz_test_monotonic_ms(std::uint64_t value) noexcept
{
  if (value > static_cast<std::uint64_t>(
      std::numeric_limits<std::int64_t>::max() / 1000000))
    return -1;
  milliseconds.store(value);
  enabled.store(true);
  return 0;
}

extern "C" int clock_gettime(clockid_t id, timespec *value) noexcept
{
  if (id == CLOCK_MONOTONIC && enabled.load())
  {
    const auto ms = milliseconds.load();
    value->tv_sec = ms / 1000;
    value->tv_nsec = (ms % 1000) * 1000000;
    return 0;
  }
  // Preserve unrelated clocks and uncontrolled startup through the kernel.
  return static_cast<int>(syscall(SYS_clock_gettime, id, value));
}
