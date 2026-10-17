#!/bin/bash
set -euo pipefail
if command -v musl-gcc >/dev/null 2>&1; then
  exit 0
fi
if sudo -n true >/dev/null 2>&1; then
  sudo -n apt-get update
  sudo -n apt-get install -y musl-tools
  exit 0
fi
prefix="${HOME}/.local/musl-native"
if [ ! -x "${prefix}/bin/musl-gcc" ]; then
  tmp="$(mktemp -d)"
  curl -fsSL https://musl.cc/x86_64-linux-musl-native.tgz | tar -xz -C "$tmp"
  src="$(find "$tmp" -mindepth 1 -maxdepth 1 -type d | head -n 1)"
  rm -rf "$prefix"
  mkdir -p "${HOME}/.local"
  mv "$src" "$prefix"
  if [ ! -x "${prefix}/bin/musl-gcc" ] && [ -x "${prefix}/bin/x86_64-linux-musl-gcc" ]; then
    ln -s x86_64-linux-musl-gcc "${prefix}/bin/musl-gcc"
  fi
fi
if [ -n "${GITHUB_PATH:-}" ]; then
  echo "${prefix}/bin" >> "${GITHUB_PATH}"
fi

