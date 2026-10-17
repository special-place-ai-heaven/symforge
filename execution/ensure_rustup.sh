#!/bin/bash
set -euo pipefail
if [ -x "${HOME}/.cargo/bin/rustup" ]; then
  export PATH="${HOME}/.cargo/bin:${PATH}"
fi
if ! command -v rustup >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs | sh -s -- -y --default-toolchain none
  export PATH="${HOME}/.cargo/bin:${PATH}"
fi
if [ -n "${GITHUB_PATH:-}" ]; then
  echo "${HOME}/.cargo/bin" >> "${GITHUB_PATH}"
fi
rustup toolchain install
if [ "$#" -gt 0 ]; then
  rustup "$@"
fi
