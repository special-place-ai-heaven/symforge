#!/bin/bash
set -euo pipefail

# Fresh self-hosted runners have no apt. Match ensure_ripgrep/ensure_musl_tools:
# use a host tool if present, else install a user-local official LLVM tarball.
#
# Prefer the ubuntu-22.04 LLVM 17 build: the older ubuntu-18.04 LLVM 18
# tarball needs libtinfo.so.5 and fails on modern runners.
prefix="${HOME}/.local/llvm-17-ubuntu-22.04"
llvm_url="https://github.com/llvm/llvm-project/releases/download/llvmorg-17.0.6/clang+llvm-17.0.6-x86_64-linux-gnu-ubuntu-22.04.tar.xz"

# Drop the broken LLVM 18 cache from earlier release attempts.
legacy_prefix="${HOME}/.local/llvm-18"
if [ -d "${legacy_prefix}" ]; then
  rm -rf "${legacy_prefix}"
fi

clang_ok() {
  command -v clang >/dev/null 2>&1 && clang --version >/dev/null 2>&1
}

install_llvm_tarball() {
  tmp="$(mktemp -d)"
  curl -fsSL -o "$tmp/llvm.tar.xz" "$llvm_url"
  tar -xJ -C "$tmp" -f "$tmp/llvm.tar.xz"
  src="$(find "$tmp" -mindepth 1 -maxdepth 1 -type d | head -n 1)"
  if [ -z "$src" ] || [ ! -x "${src}/bin/clang" ]; then
    echo "LLVM tarball did not contain a runnable clang" >&2
    exit 1
  fi
  rm -rf "$prefix"
  mkdir -p "${HOME}/.local"
  mv "$src" "$prefix"
}

if [ -x "${prefix}/bin/clang" ]; then
  export PATH="${prefix}/bin:${PATH}"
fi

if ! clang_ok; then
  if sudo -n true >/dev/null 2>&1; then
    sudo -n apt-get update
    sudo -n apt-get install -y clang lld llvm || true
  fi
fi

if ! clang_ok; then
  install_llvm_tarball
  export PATH="${prefix}/bin:${PATH}"
fi

if ! clang_ok; then
  echo "clang is required for Windows xwin builds and could not be installed" >&2
  exit 1
fi

if [ -n "${GITHUB_PATH:-}" ] && [ -d "${prefix}/bin" ] && [ -x "${prefix}/bin/clang" ]; then
  echo "${prefix}/bin" >> "${GITHUB_PATH}"
fi

# aws-lc-sys under cargo-xwin invokes clang-cl (MSVC-flavored clang).
user_bin="${HOME}/.local/bin"
mkdir -p "${user_bin}"
clang_bin="$(command -v clang)"
ln -sfn "${clang_bin}" "${user_bin}/clang-cl"
# Some LLVM packages ship clang-cl already; keep our shim first on PATH.
export PATH="${user_bin}:${PATH}"
if [ -n "${GITHUB_PATH:-}" ]; then
  echo "${user_bin}" >> "${GITHUB_PATH}"
fi
if ! command -v clang-cl >/dev/null 2>&1; then
  echo "clang-cl is not on PATH after ensure_windows_msvc" >&2
  exit 1
fi
if ! clang-cl --version >/dev/null 2>&1; then
  echo "clang-cl is not runnable after ensure_windows_msvc" >&2
  exit 1
fi

if ! command -v cargo-xwin >/dev/null 2>&1; then
  cargo install --locked cargo-xwin --version 0.23.1
fi
if [ -n "${GITHUB_PATH:-}" ] && [ -d "${HOME}/.cargo/bin" ]; then
  echo "${HOME}/.cargo/bin" >> "${GITHUB_PATH}"
fi
