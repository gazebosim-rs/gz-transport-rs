#!/usr/bin/env bash
# Copyright (C) 2026 Open Source Robotics Foundation
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail
cd "$(dirname "$0")/.."
export PKG_CONFIG_PATH="${GZ_REFERENCE_PREFIX:-/home/nkoenig/local}/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
mkdir -p artifacts
c++ -std=c++17 -Wall -Wextra -Werror tests/cpp_peer.cc -o artifacts/cpp-peer $(pkg-config --cflags --libs gz-transport) -Wl,-rpath,"${GZ_REFERENCE_PREFIX:-/home/nkoenig/local}/lib"
