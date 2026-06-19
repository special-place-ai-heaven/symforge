#!/usr/bin/env bash
# Free several GB on GitHub-hosted ubuntu-latest before heavy Rust builds.
# Runners are ephemeral — this is the only "disk cleanup" lever (no SSH).
set -euo pipefail

echo "Disk before cleanup:"
df -h /

sudo rm -rf /usr/share/dotnet
sudo rm -rf /usr/local/lib/android
sudo rm -rf /opt/ghc
sudo rm -rf /opt/hostedtoolcache/CodeQL
sudo docker image prune --all --force 2>/dev/null || true

echo "Disk after cleanup:"
df -h /
