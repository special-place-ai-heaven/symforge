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
# The fresh runners set RUSTC_WRAPPER=sccache without installing sccache.
# A pass-through of the same name keeps cargo's rustc probe working.
if ! command -v sccache >/dev/null 2>&1; then
  mkdir -p "${HOME}/.local/bin"
  printf '%s\n' '#!/bin/bash' 'exec "$@"' > "${HOME}/.local/bin/sccache"
  chmod +x "${HOME}/.local/bin/sccache"
  export PATH="${HOME}/.local/bin:${PATH}"
  if [ -n "${GITHUB_PATH:-}" ]; then
    echo "${HOME}/.local/bin" >> "${GITHUB_PATH}"
  fi
fi
rustup toolchain install
if [ "$#" -gt 0 ]; then
  rustup "$@"
fi
