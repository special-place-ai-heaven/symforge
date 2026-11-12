#!/bin/bash
# Make rustup and the rust-toolchain.toml toolchain available.
# Usage: ensure_rustup.sh [--minimal] [rustup args...]
#   --minimal  install only the pinned channel's minimal profile (rustc,
#              rust-std, cargo), no components; for jobs that build nothing.
# When rustup is absent it is bootstrapped from a pinned rustup-init release
# whose SHA-256 is the one static.rust-lang.org publishes for that version.
set -euo pipefail
minimal=""
if [ "${1:-}" = "--minimal" ]; then
  minimal=yes
  shift
fi
if [ -x "${HOME}/.cargo/bin/rustup" ]; then
  export PATH="${HOME}/.cargo/bin:${PATH}"
fi
if ! command -v rustup >/dev/null 2>&1; then
  if [ "$(uname -s)-$(uname -m)" != "Linux-x86_64" ]; then
    echo "rustup is missing and the pinned bootstrap covers Linux x86_64 only" >&2
    exit 1
  fi
  rustup_version=1.29.3
  rustup_sha256=b1c8cb14e30889e7e2999e6b69e62d9e9b9ffc0f4df8180564d306d9680e6c55
  tmp="$(mktemp -d)"
  curl --proto '=https' --tlsv1.2 -fsSL --retry 5 --retry-delay 2 --retry-connrefused \
    -o "$tmp/rustup-init" "https://static.rust-lang.org/rustup/archive/${rustup_version}/x86_64-unknown-linux-gnu/rustup-init"
  printf '%s  %s\n' "$rustup_sha256" "$tmp/rustup-init" | sha256sum --check --strict
  chmod +x "$tmp/rustup-init"
  "$tmp/rustup-init" -y --default-toolchain none
  rm -rf "$tmp"
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
if [ -n "$minimal" ]; then
  channel="$(sed -n 's/^channel = "\(.*\)"$/\1/p' rust-toolchain.toml)"
  if [ -z "$channel" ]; then
    echo "no channel in rust-toolchain.toml" >&2
    exit 1
  fi
  rustup toolchain install "$channel" --profile minimal --no-self-update
else
  rustup toolchain install
fi
if [ "$#" -gt 0 ]; then
  rustup "$@"
fi
