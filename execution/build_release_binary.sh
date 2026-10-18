#!/bin/bash
set -euo pipefail
builder="$1"
target="$2"
here="$(cd "$(dirname "$0")" && pwd)"
if [ "$builder" = "xwin" ]; then
  bash "${here}/ensure_windows_msvc.sh"
  cargo xwin build --release --target "$target"
else
  cargo build --release --target "$target"
fi
