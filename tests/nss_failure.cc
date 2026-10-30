// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// This interposer is loaded only into explicit study children, never native NSS
// qualification or the invoking shell. Malformed records test defensive checks.
#include <arpa/inet.h>
#include <dlfcn.h>
#include <netdb.h>
#include <cerrno>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>

extern "C" int gethostbyname_r(const char *name, hostent *record, char *buffer,
    size_t length, hostent **result, int *hostError) {
  const char *mode = std::getenv("GZ_NSS_STUDY_MODE");
  if (!mode) {
    const auto real = reinterpret_cast<decltype(&gethostbyname_r)>(
        dlsym(RTLD_NEXT, "gethostbyname_r"));
    return real(name, record, buffer, length, result, hostError);
  }
  std::fprintf(stderr, "CALL %zu\n", length);
  *result = nullptr;
  *hostError = 0;
  if (!std::strcmp(mode, "range") ||
      (!std::strcmp(mode, "retry") && length < 4096)) {
    *hostError = NETDB_INTERNAL;
    return ERANGE;
  }
  if (!std::strcmp(mode, "range-other-error")) { *hostError = TRY_AGAIN; return ERANGE; }
  if (!std::strcmp(mode, "again")) { *hostError = TRY_AGAIN; return EAGAIN; }
  if (!std::strcmp(mode, "memory")) { *hostError = NETDB_INTERNAL; return ENOMEM; }
  if (!std::strcmp(mode, "absent")) { *hostError = HOST_NOT_FOUND; return 0; }

  record->h_addrtype = AF_INET;
  record->h_length = 4;
  record->h_name = nullptr;
  record->h_aliases = nullptr;
  const size_t offset = !std::strcmp(mode, "unaligned") ? 1 : 0;
  record->h_addr_list = reinterpret_cast<char **>(buffer + offset);
  char *first = buffer + 128;
  std::memcpy(buffer + offset, &first, sizeof(first));
  char *last = nullptr;
  std::memcpy(buffer + offset + sizeof(first), &last, sizeof(last));
  const unsigned char bytes[] = {203, 0, 113, 9};
  std::memcpy(first, bytes, sizeof(bytes));
  *result = record;
  if (!std::strcmp(mode, "family")) record->h_addrtype = AF_INET6;
  if (!std::strcmp(mode, "length")) record->h_length = 16;
  if (!std::strcmp(mode, "null-list")) record->h_addr_list = nullptr;
  if (!std::strcmp(mode, "null-first")) std::memcpy(buffer + offset, &last, sizeof(last));
  if (!std::strcmp(mode, "foreign-result")) *result = reinterpret_cast<hostent *>(buffer);
  if (!std::strcmp(mode, "foreign-list")) record->h_addr_list = reinterpret_cast<char **>(1);
  if (!std::strcmp(mode, "list-tail")) record->h_addr_list = reinterpret_cast<char **>(buffer + length - 1);
  if (!std::strcmp(mode, "foreign-address")) {
    first = reinterpret_cast<char *>(1);
    std::memcpy(buffer + offset, &first, sizeof(first));
  }
  if (!std::strcmp(mode, "address-tail")) {
    first = buffer + length - 3;
    std::memcpy(buffer + offset, &first, sizeof(first));
  }
  if (!std::strcmp(mode, "later-malformed")) {
    last = reinterpret_cast<char *>(1);
    std::memcpy(buffer + offset + sizeof(first), &last, sizeof(last));
  }
  if (!std::strcmp(mode, "alternating")) {
    static unsigned count = 0;
    first[3] = static_cast<char>(++count);
  }
  return 0;
}
