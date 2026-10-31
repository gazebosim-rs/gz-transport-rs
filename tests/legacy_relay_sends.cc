// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Child-only capture: never call a real network-send function, for ANY address.
#include <arpa/inet.h>
#include <cerrno>
#include <cstdio>
#include <cstdlib>
#include <sys/socket.h>

__attribute__((constructor)) static void ready()
{
  if (setenv("GZ_LEGACY_CAPTURE_READY", "sendto-intercepted", 1) != 0)
    std::abort();
}

extern "C" ssize_t sendto(int, const void *, size_t length, int,
    const sockaddr *destination, socklen_t size)
{
  if (size != sizeof(sockaddr_in) || destination->sa_family != AF_INET)
    std::abort();
  const auto *address = reinterpret_cast<const sockaddr_in *>(destination);
  char text[INET_ADDRSTRLEN];
  if (!inet_ntop(AF_INET, &address->sin_addr, text, sizeof(text)))
    std::abort();
  std::fprintf(stderr, "CAPTURE %s %u %zu\n", text,
      ntohs(address->sin_port), length);
  if (std::getenv("GZ_LEGACY_FAIL_BROADCAST") &&
      address->sin_addr.s_addr == INADDR_BROADCAST)
  {
    errno = EACCES;
    return -1;
  }
  return static_cast<ssize_t>(length);
}
