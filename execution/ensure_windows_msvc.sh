#!/bin/bash
set -euo pipefail

# Prefer a working host clang. The old llvmorg ubuntu-18.04 tarball under
# ~/.local/llvm-18 needs libtinfo.so.5 and breaks modern runners.
prefix="${HOME}/.local/llvm-18"
if [ -x "${prefix}/bin/clang" ] && ! "${prefix}/bin/clang" --version >/dev/null 2>&1; then
  rm -rf "${prefix}"
fi

clang_ok() {
  command -v clang >/dev/null 2>&1 && clang --version >/dev/null 2>&1
}

if ! clang_ok; then
  if sudo -n true >/dev/null 2>&1; then
    sudo -n apt-get update
    sudo -n apt-get install -y clang lld llvm
  else
    echo "clang is required for Windows xwin builds and apt is unavailable" >&2
    exit 1
  fi
fi

if ! clang_ok; then
  echo "clang installed but not runnable" >&2
  exit 1
fi

# aws-lc-sys under cargo-xwin invokes clang-cl (MSVC-flavored clang).
user_bin="${HOME}/.local/bin"
mkdir -p "${user_bin}"
clang_bin="$(command -v clang)"
ln -sfn "${clang_bin}" "${user_bin}/clang-cl"
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
