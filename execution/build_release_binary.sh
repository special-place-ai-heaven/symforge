#!/bin/bash
set -euo pipefail
builder="$1"
target="$2"
# Every release target builds natively on a host of its own OS (Windows on
# GitHub-hosted windows-latest), so plain cargo is the only builder.
if [ "$builder" != "cargo" ]; then
  echo "unsupported release builder: ${builder}" >&2
  exit 1
fi
cargo build --release --target "$target"
