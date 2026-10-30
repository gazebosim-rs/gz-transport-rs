#!/usr/bin/env bash
# Copyright (C) 2026 Open Source Robotics Foundation
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail
cd "$(dirname "$0")/.."
export PKG_CONFIG_PATH="${GZ_REFERENCE_PREFIX:-/home/nkoenig/local}/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
mkdir -p artifacts
c++ -std=c++17 -Wall -Wextra -Werror tests/cpp_peer.cc -o artifacts/cpp-peer $(pkg-config --cflags --libs gz-transport) -lzmq -Wl,-rpath,"${GZ_REFERENCE_PREFIX:-/home/nkoenig/local}/lib"
# Scope controls instantiate the pinned native Discovery template; use the same
# reference source/configuration as the installed SDK and the baseline verifier.
c++ -std=c++17 -Wall -Wextra -Werror -I "${GZ_REFERENCE_SOURCE:-$(pwd)/../gz-transport}/src" tests/service_scope_peer.cc -o artifacts/service-scope-peer $(pkg-config --cflags --libs gz-transport) -Wl,-rpath,"${GZ_REFERENCE_PREFIX:-/home/nkoenig/local}/lib"
c++ -std=c++17 -Wall -Wextra -Werror tests/sample_statistics_peer.cc -o artifacts/sample-statistics-peer $(pkg-config --cflags --libs gz-transport) -Wl,-rpath,"${GZ_REFERENCE_PREFIX:-/home/nkoenig/local}/lib"
c++ -std=c++17 -Wall -Wextra -Werror tests/topic_statistics_peer.cc -o artifacts/topic-statistics-peer $(pkg-config --cflags --libs gz-transport) -Wl,-rpath,"${GZ_REFERENCE_PREFIX:-/home/nkoenig/local}/lib"
# Clock interposition is scoped to the pure metric helper child by its Rust test.
c++ -std=c++17 -Wall -Wextra -Werror -shared -fPIC tests/topic_metric_clock.cc -o artifacts/topic-metric-clock.so
c++ -std=c++17 -Wall -Wextra -Werror tests/topic_metric_peer.cc -o artifacts/topic-metric-peer $(pkg-config --cflags --libs gz-transport) -ldl -Wl,-rpath,"${GZ_REFERENCE_PREFIX:-/home/nkoenig/local}/lib"
c++ -std=c++17 -Wall -Wextra -Werror -pthread tests/subscription_callback_peer.cc -o artifacts/subscription-callback-peer $(pkg-config --cflags --libs gz-transport) -Wl,-rpath,"${GZ_REFERENCE_PREFIX:-/home/nkoenig/local}/lib"
c++ -std=c++17 -Wall -Wextra -Werror -pthread tests/owned_subscriber_peer.cc -o artifacts/owned-subscriber-peer $(pkg-config --cflags --libs gz-transport) -Wl,-rpath,"${GZ_REFERENCE_PREFIX:-/home/nkoenig/local}/lib"
