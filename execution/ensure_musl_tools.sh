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
  version="1.2.6"
  sha256="d585fd3b613c66151fc3249e8ed44f77020cb5e6c1e635a616d3f9f82460512a"
  tmp="$(mktemp -d)"
  curl -fsSL --retry 5 --retry-delay 2 --retry-connrefused -o "${tmp}/musl.tar.gz" "https://musl.libc.org/releases/musl-${version}.tar.gz"
  echo "${sha256}  ${tmp}/musl.tar.gz" | sha256sum -c -
  tar -xzf "${tmp}/musl.tar.gz" -C "$tmp"
  rm -rf "$prefix"
  (cd "${tmp}/musl-${version}" && ./configure --prefix="$prefix" && make -j"$(nproc)" && make install)
  rm -rf "$tmp"
fi
if [ -n "${GITHUB_PATH:-}" ]; then
  echo "${prefix}/bin" >> "${GITHUB_PATH}"
fi
