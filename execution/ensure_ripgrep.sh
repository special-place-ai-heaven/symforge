#!/bin/bash
set -euo pipefail
if command -v rg >/dev/null 2>&1; then
  exit 0
fi
if sudo -n true >/dev/null 2>&1; then
  sudo -n apt-get update
  sudo -n apt-get install -y ripgrep
  exit 0
fi
dest="${HOME}/.local/bin"
mkdir -p "$dest"
tmp="$(mktemp -d)"
url="https://github.com/BurntSushi/ripgrep/releases/download/14.1.1/ripgrep-14.1.1-x86_64-unknown-linux-musl.tar.gz"
  curl -fsSL -o "$tmp/rg.tgz" "$url"
tar -xz -C "$tmp" -f "$tmp/rg.tgz"
install -m 755 "$tmp"/ripgrep-*/rg "$dest/rg"
if [ -n "${GITHUB_PATH:-}" ]; then
  echo "$dest" >> "${GITHUB_PATH}"
fi
