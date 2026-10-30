// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Child-only controlled inputs to the installed library, never a Node fixture.
#include <arpa/inet.h>
#include <dlfcn.h>
#include <ifaddrs.h>
#include <net/if.h>
#include <netdb.h>
#include <sys/ioctl.h>
#include <unistd.h>
#include <cerrno>
#include <cstdio>
#include <cstdlib>
#include <cstdarg>
#include <cstring>
#include <iostream>
#include <string>
#include <vector>

namespace {
bool controlled = false;
void initialize();
std::string host;
std::vector<in_addr> resolved;
std::vector<char *> resolvedPointers;
struct Entry {
  std::string name;
  int index;
  ifaddrs record{};
  sockaddr_storage address{};
};
std::vector<Entry> entries;
unsigned hostCalls = 0, resolveCalls = 0, inventoryCalls = 0;
unsigned socketCalls = 0, indexCalls = 0, freeCalls = 0;
}

extern "C" int gethostname(char *output, size_t size) noexcept {
  initialize();
  if (!controlled) {
    const auto real = reinterpret_cast<int (*)(char *, size_t)>(
        dlsym(RTLD_NEXT, "gethostname"));
    return real(output, size);
  }
  ++hostCalls;
  if (host == "!fail") { errno = EIO; return -1; }
  if (host.size() >= size) { errno = ENAMETOOLONG; return -1; }
  std::memcpy(output, host.c_str(), host.size() + 1);
  return 0;
}

extern "C" hostent *gethostbyname(const char *name) {
  initialize();
  if (!controlled) {
    const auto real = reinterpret_cast<hostent *(*)(const char *)>(
        dlsym(RTLD_NEXT, "gethostbyname"));
    return real(name);
  }
  ++resolveCalls;
  if (name != host || resolved.empty()) return nullptr;
  static hostent result{};
  result.h_name = host.data();
  result.h_addrtype = AF_INET;
  result.h_length = sizeof(in_addr);
  result.h_addr_list = resolvedPointers.data();
  return &result;
}

extern "C" int getifaddrs(ifaddrs **output) noexcept {
  initialize();
  if (!controlled) {
    const auto real = reinterpret_cast<int (*)(ifaddrs **)>(
        dlsym(RTLD_NEXT, "getifaddrs"));
    return real(output);
  }
  ++inventoryCalls;
  if (std::getenv("GZ_INTERFACE_INVENTORY_ERROR")) {
    *output = nullptr;
    errno = EIO;
    return -1;
  }
  *output = entries.empty() ? nullptr : &entries.front().record;
  return 0;
}

extern "C" void freeifaddrs(ifaddrs *input) noexcept {
  initialize();
  if (!controlled) {
    const auto real = reinterpret_cast<void (*)(ifaddrs *)>(
        dlsym(RTLD_NEXT, "freeifaddrs"));
    real(input);
    return;
  }
  if (input != (entries.empty() ? nullptr : &entries.front().record))
    std::abort();
  ++freeCalls;
}

extern "C" int socket(int domain, int type, int protocol) noexcept {
  initialize();
  if (!controlled) {
    const auto real = reinterpret_cast<int (*)(int, int, int)>(
        dlsym(RTLD_NEXT, "socket"));
    return real(domain, type, protocol);
  }
  // NetUtils only uses this descriptor for our intercepted interface ioctl.
  // No actual socket, network lookup or process-global interposition occurs.
  if (domain != AF_UNIX || type != SOCK_DGRAM || protocol != 0) std::abort();
  ++socketCalls;
  return 12345;
}

extern "C" int ioctl(int descriptor, unsigned long request, ...) noexcept {
  initialize();
  va_list args;
  va_start(args, request);
  auto *record = va_arg(args, ifreq *);
  va_end(args);
  if (!controlled || descriptor != 12345 || request != SIOCGIFINDEX)
    std::abort();
  ++indexCalls;
  for (const auto &entry : entries) {
    if (entry.name == record->ifr_name) {
      if (entry.index < 0) { errno = ENODEV; return -1; }
      record->ifr_ifindex = entry.index;
      return 0;
    }
  }
  std::abort();
}

namespace {
void initialize() {
  static bool initialized = false;
  if (initialized) return;
  initialized = true;
  const char *hostname = std::getenv("GZ_INTERFACE_HOST");
  if (!hostname) return;
  host = hostname;
  std::string addresses = std::getenv("GZ_INTERFACE_RESOLVED") ? std::getenv("GZ_INTERFACE_RESOLVED") : "";
  while (!addresses.empty()) {
    const auto split = addresses.find(',');
    in_addr value{};
    if (inet_pton(AF_INET, addresses.substr(0, split).c_str(), &value) != 1) std::abort();
    resolved.push_back(value);
    if (split == std::string::npos) break;
    addresses.erase(0, split + 1);
  }
  for (auto &address : resolved) resolvedPointers.push_back(reinterpret_cast<char *>(&address));
  resolvedPointers.push_back(nullptr);
  std::vector<std::string> specifications;
  std::string rows = std::getenv("GZ_INTERFACE_ROWS") ? std::getenv("GZ_INTERFACE_ROWS") : "";
  while (!rows.empty()) {
    const auto split = rows.find('|');
    specifications.push_back(rows.substr(0, split));
    if (split == std::string::npos) break;
    rows.erase(0, split + 1);
  }
  entries.resize(specifications.size());
  for (size_t i = 0; i < specifications.size(); ++i) {
    const auto &specification = specifications[i];
    const auto first = specification.find(',');
    const auto second = specification.find(',', first + 1);
    if (first == std::string::npos || second == std::string::npos) std::abort();
    auto &entry = entries[i];
    entry.name = "fixture" + std::to_string(i);
    if (std::getenv("GZ_INTERFACE_AMBIG_NAME")) entry.name.push_back(static_cast<char>(255));
    entry.index = std::stoi(specification.substr(0, first));
    entry.record.ifa_flags = std::stoul(specification.substr(first + 1, second));
    entry.record.ifa_name = entry.name.data();
    entry.record.ifa_next = i + 1 < entries.size() ? &entries[i + 1].record : nullptr;
    auto address = specification.substr(second + 1);
    if (address == "null") continue;
    entry.record.ifa_addr = reinterpret_cast<sockaddr *>(&entry.address);
    if (address == "unknown") entry.record.ifa_addr->sa_family = AF_UNIX;
    else if (address.find(':') != std::string::npos) {
      auto *ip = reinterpret_cast<sockaddr_in6 *>(&entry.address);
      ip->sin6_family = AF_INET6;
      if (inet_pton(AF_INET6, address.c_str(), &ip->sin6_addr) != 1) std::abort();
    } else {
      auto *ip = reinterpret_cast<sockaddr_in *>(&entry.address);
      ip->sin_family = AF_INET;
      if (inet_pton(AF_INET, address.c_str(), &ip->sin_addr) != 1) std::abort();
    }
  }
  controlled = true;
}
}

extern "C" unsigned if_nametoindex(const char *name) noexcept {
  initialize();
  if (!controlled) {
    const auto real = reinterpret_cast<unsigned (*)(const char *)>(dlsym(RTLD_NEXT, "if_nametoindex"));
    return real(name);
  }
  ++indexCalls;
  for (const auto &entry : entries) {
    if (entry.name == name) {
      if (entry.index < 0) { errno = ENODEV; return 0; }
      return entry.index;
    }
  }
  std::abort();
}

extern "C" int gethostbyname_r(const char *name, hostent *record, char *buffer,
    size_t length, hostent **result, int *hostError) {
  initialize();
  if (!controlled) {
    const auto real = reinterpret_cast<decltype(&gethostbyname_r)>(dlsym(RTLD_NEXT, "gethostbyname_r"));
    return real(name, record, buffer, length, result, hostError);
  }
  ++resolveCalls;
  *result = nullptr;
  *hostError = 0;
  if (name != host || resolved.empty()) { *hostError = HOST_NOT_FOUND; return 0; }
  const size_t pointers = (resolved.size() + 1) * sizeof(char *);
  if (length < pointers + resolved.size() * 4) { *hostError = NETDB_INTERNAL; return ERANGE; }
  record->h_name = nullptr;
  record->h_aliases = nullptr;
  record->h_addrtype = AF_INET;
  record->h_length = 4;
  record->h_addr_list = reinterpret_cast<char **>(buffer);
  for (size_t i = 0; i < resolved.size(); ++i) {
    char *address = buffer + pointers + i * 4;
    std::memcpy(address, &resolved[i], 4);
    std::memcpy(buffer + i * sizeof(char *), &address, sizeof(address));
  }
  char *end = nullptr;
  std::memcpy(buffer + resolved.size() * sizeof(char *), &end, sizeof(end));
  *result = record;
  return 0;
}

__attribute__((destructor)) void printCounts() {
  if (std::getenv("GZ_INTERFACE_HOST"))
    std::fprintf(stderr, "COUNTS %u %u %u %u %u %u\n", hostCalls, resolveCalls,
                 inventoryCalls, socketCalls, indexCalls, freeCalls);
}
