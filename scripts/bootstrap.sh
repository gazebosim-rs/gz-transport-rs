#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
rustup toolchain install 1.94.1 --profile minimal --component clippy,rustfmt,llvm-tools-preview
git config core.hooksPath .githooks
