// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Test-only child interposition. No host clock or NSS state is changed.
#include <atomic>
#include <cerrno>
#include <cstdlib>
#include <cstdint>
#include <cstdio>
#include <dlfcn.h>
#include <time.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/syscall.h>

namespace {
std::atomic<unsigned> monoCalls{0}, realCalls{0}, timeCalls{0};
std::int64_t realNs = 0, monoNs[3] = {0, 0, 0};
bool enabled = false, failReal = false, failMono = false;
const char *ownerFile = nullptr;
bool wideReal = false;
std::int64_t realSeconds = 0;
std::int64_t nextRealNs = 0;
bool stepReal = false;
long failMonoAt = -1;
std::int64_t Read(const char *name) {
  const char *text = std::getenv(name);
  return text ? std::strtoll(text, nullptr, 10) : 0;
}
__attribute__((constructor)) void Configure() {
  enabled = std::getenv("GZ_WALL_VALUES") != nullptr;
  if (!enabled) return;
  realNs = Read("GZ_WALL_REAL_NS");
  ownerFile = std::getenv("GZ_WALL_OWNER_FILE");
  wideReal = std::getenv("GZ_WALL_REAL_SEC") != nullptr;
  realSeconds = Read("GZ_WALL_REAL_SEC");
  monoNs[0] = Read("GZ_WALL_MONO_0");
  monoNs[1] = Read("GZ_WALL_MONO_1");
  monoNs[2] = Read("GZ_WALL_MONO_2");
  stepReal = std::getenv("GZ_WALL_NEXT_REAL_NS") != nullptr;
  nextRealNs = Read("GZ_WALL_NEXT_REAL_NS");
  failReal = std::getenv("GZ_WALL_FAIL_REAL") != nullptr;
  failMono = std::getenv("GZ_WALL_FAIL_MONO") != nullptr;
  if (std::getenv("GZ_WALL_FAIL_MONO_AT")) failMonoAt = Read("GZ_WALL_FAIL_MONO_AT");
}
bool Active() {
  if (!enabled) return false;
  if (!ownerFile) return true;
  int fd = open(ownerFile, O_RDONLY | O_CLOEXEC);
  if (fd < 0) return false;
  char text[32] = {};
  const auto size = read(fd, text, sizeof(text) - 1);
  close(fd);
  return size > 0 && std::strtol(text, nullptr, 10) == syscall(SYS_gettid);
}
void Stamp(std::int64_t ns, timespec *out) {
  auto seconds = ns / 1000000000;
  auto nanos = ns % 1000000000;
  if (nanos < 0) { --seconds; nanos += 1000000000; }
  out->tv_sec = seconds;
  out->tv_nsec = nanos;
}
}
extern "C" unsigned gz_wall_count(unsigned kind) noexcept {
  return kind == 0 ? timeCalls.load() : (kind == 1 ? monoCalls.load() : realCalls.load());
}
extern "C" void gz_wall_realtime(std::int64_t ns) noexcept { realNs = ns; }
extern "C" time_t time(time_t *out) noexcept {
  using Function = time_t (*)(time_t *);
  if (!Active()) {
    static auto real = reinterpret_cast<Function>(dlsym(RTLD_NEXT, "time"));
    return real(out);
  }
  ++timeCalls;
  timespec stamp;
  Stamp(realNs, &stamp);
  if (out) *out = stamp.tv_sec;
  return stamp.tv_sec;
}
extern "C" int clock_gettime(clockid_t kind, timespec *out) noexcept {
  using Function = int (*)(clockid_t, timespec *);
  if (Active() && (kind == CLOCK_REALTIME || kind == CLOCK_MONOTONIC)) {
    const auto index = kind == CLOCK_REALTIME ? realCalls.fetch_add(1) : monoCalls.fetch_add(1);
    if ((kind == CLOCK_REALTIME && failReal) ||
        (kind == CLOCK_MONOTONIC && (failMono || long(index) == failMonoAt))) {
      errno = EIO;
      return -1;
    }
    if (kind == CLOCK_REALTIME) {
      if (wideReal) { out->tv_sec = realSeconds; out->tv_nsec = 0; }
      else Stamp(realNs, out);
    }
    else { Stamp(monoNs[index < 3 ? index : 2], out);
      if (stepReal && index == 1) realNs = nextRealNs; }
    return 0;
  }
  static auto real = reinterpret_cast<Function>(dlsym(RTLD_NEXT, "clock_gettime"));
  return real(kind, out);
}

__attribute__((destructor)) static void Report() {
  if (std::getenv("GZ_WALL_REPORT"))
    std::fprintf(stderr, "COUNTS %u %u %u\n", timeCalls.load(), monoCalls.load(), realCalls.load());
}
