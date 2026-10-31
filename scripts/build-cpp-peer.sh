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
c++ -std=c++17 -Wall -Wextra -Werror -pthread tests/raw_subscription_peer.cc -o artifacts/raw-subscription-peer $(pkg-config --cflags --libs gz-transport) -Wl,-rpath,"${GZ_REFERENCE_PREFIX:-/home/nkoenig/local}/lib"
c++ -std=c++17 -Wall -Wextra -Werror -I "${GZ_REFERENCE_SOURCE:-$(pwd)/../gz-transport}/src" tests/discovery_relay_peer.cc -o artifacts/discovery-relay-peer $(pkg-config --cflags --libs gz-transport) -Wl,-rpath,"${GZ_REFERENCE_PREFIX:-/home/nkoenig/local}/lib"
# First-A qualification creates no Nodes. Interposition is child-only.
c++ -std=c++17 -Wall -Wextra -Werror tests/interface_selection_peer.cc -o artifacts/interface-selection-peer $(pkg-config --cflags --libs gz-transport) -Wl,-rpath,"${GZ_REFERENCE_PREFIX:-/home/nkoenig/local}/lib"
c++ -std=c++17 -Wall -Wextra -Werror -shared -fPIC tests/interface_inputs.cc -o artifacts/interface-inputs.so -ldl
c++ -std=c++17 -Wall -Wextra -Werror -shared -fPIC tests/nss_failure.cc -o artifacts/nss-failure.so -ldl
cc -std=c11 -Wall -Wextra -Werror -O2 tests/nss_peer.c -o artifacts/nss-peer

# NetworkClock values and bidirectional Clock messages use the actual SDK.
c++ -std=c++17 -Wall -Wextra -Werror -pthread tests/network_clock_peer.cc -o artifacts/network-clock-peer $(pkg-config --cflags --libs gz-transport) -Wl,-rpath,"${GZ_REFERENCE_PREFIX:-/home/nkoenig/local}/lib"

# WallClock observation is pure and its interposition is scoped to child reads.
c++ -std=c++17 -Wall -Wextra -Werror -shared -fPIC tests/wall_clock_control.cc -o artifacts/wall-clock-control.so -ldl
c++ -std=c++17 -Wall -Wextra -Werror tests/wall_clock_peer.cc -o artifacts/wall-clock-peer $(pkg-config --cflags --libs gz-transport) -ldl -Wl,-rpath,"${GZ_REFERENCE_PREFIX:-/home/nkoenig/local}/lib"

# Exact mixed-subscriber publication schedule; child-only domains come from tests.
c++ -std=c++17 -Wall -Wextra -Werror -pthread tests/mixed_subscriber_peer.cc -o artifacts/mixed-subscriber-peer $(pkg-config --cflags --libs gz-transport) -Wl,-rpath,"${GZ_REFERENCE_PREFIX:-/home/nkoenig/local}/lib"

# Legacy relay inputs include invalid-to-broadcast; all sends are child-intercepted.
c++ -std=c++17 -Wall -Wextra -Werror -I "${GZ_REFERENCE_SOURCE:-$(pwd)/../gz-transport}/src" tests/legacy_relay_peer.cc -o artifacts/legacy-relay-peer $(pkg-config --cflags --libs gz-transport) -Wl,-rpath,"${GZ_REFERENCE_PREFIX:-/home/nkoenig/local}/lib"
c++ -std=c++17 -Wall -Wextra -Werror -shared -fPIC tests/legacy_relay_sends.cc -o artifacts/legacy-relay-sends.so
