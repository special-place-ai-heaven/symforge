#!/usr/bin/env bash
# Shallow-clone phase0 battery repos for STEL golden replay and MCP batteries.
# Idempotent: skips any corpus whose marker file already exists.
set -euo pipefail

ROOT="${1:-tests/fixtures/phase0-corpus}"

clone_if_missing() {
  local dir="$1"
  local url="$2"
  local marker="$3"
  if [[ -f "${dir}/${marker}" ]]; then
    echo "phase0 corpus ready: ${dir} (${marker})"
    return 0
  fi
  echo "cloning ${url} -> ${dir}"
  git clone --depth 1 "${url}" "${dir}"
}

mkdir -p "${ROOT}"

clone_if_missing "${ROOT}/cfg-if-rust" https://github.com/rust-lang/cfg-if.git src/lib.rs
clone_if_missing "${ROOT}/records-python" https://github.com/kennethreitz/records.git records.py
clone_if_missing "${ROOT}/is-plain-obj-ts" https://github.com/sindresorhus/is-plain-obj.git index.js
