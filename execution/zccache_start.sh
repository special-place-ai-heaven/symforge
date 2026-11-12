#!/usr/bin/env bash
# Install and start the forge-ci/zccache 1.15.6 compiler cache (tag commit
# 15ec9537af0d219e07b72ce2b3f97d5f68d72b5e); the release archive is checked
# against its published SHA-256. ZCCACHE_MODE=COPY: hits are independent
# copies, never an inode shared with the cache.
#
# The cache is an optimization, never a correctness dependency: when the
# download, the checksum, or the daemon start fails after bounded retries, the
# job continues WITHOUT it (RUSTC_WRAPPER is set empty, which the build tool
# treats as "no wrapper") and the job summary says so.
set -euo pipefail

version=1.15.6
url="https://github.com/forge-ci/zccache/releases/download/${version}/zccache-v${version}-x86_64-unknown-linux-musl.tar.gz"
sha256=885a109c758808480ebda6b92ac0c55ae55ff2638f0ef850423bb185f28e59f7
dir="$RUNNER_TEMP/zccache-bin"
export ZCCACHE_CACHE_DIR="$RUNNER_TEMP/zccache"
export ZCCACHE_MODE=COPY

without_cache() {
  echo "::warning::zccache unavailable ($1); this job compiles without the compiler cache"
  echo "- zccache unavailable ($1): compiled without the compiler cache" >> "$GITHUB_STEP_SUMMARY"
  echo "RUSTC_WRAPPER=" >> "$GITHUB_ENV"
  exit 0
}

mkdir -p "$dir"
curl --proto '=https' --tlsv1.2 -fsSL --retry 5 --retry-delay 2 --retry-connrefused \
  -o "$dir/zccache.tar.gz" "$url" || without_cache "download failed"
printf '%s  %s\n' "$sha256" "$dir/zccache.tar.gz" | sha256sum --check --strict \
  || without_cache "checksum mismatch"
tar -xzf "$dir/zccache.tar.gz" -C "$dir" --strip-components=1 || without_cache "unpack failed"
[ "$("$dir/zccache" --version)" = "zccache ${version}" ] || without_cache "unexpected version"

started=""
for attempt in 1 2 3; do
  if "$dir/zccache" start && "$dir/zccache" status >/dev/null; then
    started=yes
    break
  fi
  echo "zccache start attempt ${attempt} failed; retrying" >&2
  "$dir/zccache" stop >/dev/null 2>&1 || true
  sleep $(( attempt * 3 ))
done
[ -n "$started" ] || without_cache "daemon did not accept connections after 3 attempts"

echo "$dir" >> "$GITHUB_PATH"
{
  echo "ZCCACHE_CACHE_DIR=$ZCCACHE_CACHE_DIR"
  echo "ZCCACHE_MODE=$ZCCACHE_MODE"
  echo "RUSTC_WRAPPER=$dir/zccache"
} >> "$GITHUB_ENV"
