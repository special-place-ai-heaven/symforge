#!/bin/bash
set -euo pipefail

prefix="${HOME}/.local/llvm-18"
if ! command -v clang >/dev/null 2>&1; then
  if [ -x "${prefix}/bin/clang" ]; then
    export PATH="${prefix}/bin:${PATH}"
  elif sudo -n true >/dev/null 2>&1; then
    sudo -n apt-get update
    sudo -n apt-get install -y clang lld llvm
  else
    tmp="$(mktemp -d)"
    curl -fsSL -o "$tmp/llvm.tar.xz" \
      "https://github.com/llvm/llvm-project/releases/download/llvmorg-18.1.8/clang+llvm-18.1.8-x86_64-linux-gnu-ubuntu-18.04.tar.xz"
    tar -xJ -C "$tmp" -f "$tmp/llvm.tar.xz"
    src="$(find "$tmp" -mindepth 1 -maxdepth 1 -type d | head -n 1)"
    rm -rf "$prefix"
    mkdir -p "${HOME}/.local"
    mv "$src" "$prefix"
    export PATH="${prefix}/bin:${PATH}"
  fi
fi
if [ -n "${GITHUB_PATH:-}" ] && [ -d "${prefix}/bin" ]; then
  echo "${prefix}/bin" >> "${GITHUB_PATH}"
fi

# aws-lc-sys under cargo-xwin invokes clang-cl (MSVC-flavored clang).
# Host packages often ship only `clang`; put a user-local clang-cl on PATH.
user_bin="${HOME}/.local/bin"
mkdir -p "${user_bin}"
clang_bin="$(command -v clang)"
if [ -z "${clang_bin}" ]; then
  echo "clang is required for Windows xwin builds" >&2
  exit 1
fi
ln -sfn "${clang_bin}" "${user_bin}/clang-cl"
export PATH="${user_bin}:${PATH}"
if [ -n "${GITHUB_PATH:-}" ]; then
  echo "${user_bin}" >> "${GITHUB_PATH}"
fi
if ! command -v clang-cl >/dev/null 2>&1; then
  echo "clang-cl is not on PATH after ensure_windows_msvc" >&2
  exit 1
fi

if ! command -v cargo-xwin >/dev/null 2>&1; then
  cargo install --locked cargo-xwin --version 0.23.1
fi
if [ -n "${GITHUB_PATH:-}" ] && [ -d "${HOME}/.cargo/bin" ]; then
  echo "${HOME}/.cargo/bin" >> "${GITHUB_PATH}"
fi
