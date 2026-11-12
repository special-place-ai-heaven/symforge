#!/bin/bash
# Supply cmake without root when the runner image lacks it (the self-hosted
# pool has no cmake and no sudo). aws-lc-sys builds with it. Pinned official
# Kitware release; the SHA-256 is the one Kitware publishes in
# cmake-3.31.12-SHA-256.txt. 3.31 is the series the hosted ubuntu images ship;
# CMake 4 drops compatibility with old cmake_minimum_required policies.
set -euo pipefail
if command -v cmake >/dev/null 2>&1; then
  exit 0
fi
version=3.31.12
sha256=0dc2e9a6860f06bf10bd8fadc03e35d9eeb4df46e33763a7e480e987758f385c
url="https://github.com/Kitware/CMake/releases/download/v${version}/cmake-${version}-linux-x86_64.tar.gz"
dir="${RUNNER_TEMP:?}/cmake"
mkdir -p "$dir"
curl --proto '=https' --tlsv1.2 -fsSL --retry 5 --retry-delay 2 --retry-connrefused -o "$dir/cmake.tar.gz" "$url"
printf '%s  %s\n' "$sha256" "$dir/cmake.tar.gz" | sha256sum --check --strict
tar -xzf "$dir/cmake.tar.gz" -C "$dir" --strip-components=1
rm "$dir/cmake.tar.gz"
"$dir/bin/cmake" --version
echo "$dir/bin" >> "${GITHUB_PATH:?}"
