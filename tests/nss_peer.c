// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
#define _DEFAULT_SOURCE
#include <arpa/inet.h>
#include <gnu/libc-version.h>
#include <netdb.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>

_Static_assert(NETDB_INTERNAL == -1, "Linux GNU host error ABI");

static void addresses(const struct hostent *record) {
  if (!record) return;
  if (record->h_addrtype != AF_INET || record->h_length != 4 || !record->h_addr_list) abort();
  for (char **address = record->h_addr_list; *address; ++address) {
    char text[INET_ADDRSTRLEN];
    if (!inet_ntop(AF_INET, *address, text, sizeof(text))) abort();
    printf(" %s", text);
  }
}

int main(int argc, char **argv) {
  if (argc != 2) return 2;
  printf("GLIBC %s\n", gnu_get_libc_version());
  errno = 0;
  struct hostent *legacy = gethostbyname(argv[1]);
  const int legacy_error = h_errno;
  const int legacy_errno = errno;
  printf("LEGACY %d %d %d", legacy != NULL, legacy_error, legacy_errno);
  addresses(legacy);
  puts("");
  struct hostent record = {0};
  struct hostent *result = NULL;
  int host_error = 0;
  char buffer[65536];
  const int code = gethostbyname_r(argv[1], &record, buffer, sizeof(buffer), &result, &host_error);
  printf("REENTRANT %d %d %d", code, result != NULL, host_error);
  addresses(code == 0 ? result : NULL);
  puts("");
}
