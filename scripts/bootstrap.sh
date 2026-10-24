#!/usr/bin/env bash
# Copyright (C) 2026 Open Source Robotics Foundation
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail
cd "$(dirname "$0")/.."
rustup toolchain install 1.94.1 --profile minimal --component clippy,rustfmt,llvm-tools-preview
git config core.hooksPath .githooks
