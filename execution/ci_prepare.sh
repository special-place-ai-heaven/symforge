#!/usr/bin/env bash
# Print what this runner provides, install missing build tools when the image
# allows it, and fail with a clear message when a tool the job needs is absent.
# Linux jobs run on GitHub-hosted images and on ephemeral self-hosted runners
# whose contents are not ours, so nothing may be assumed preinstalled.
#
# Usage: ci_prepare.sh TOOL...   (prints the build-parallelism bound on stdout)
set -euo pipefail

{
  echo "runner: ${RUNNER_NAME:-unknown} (${RUNNER_ENVIRONMENT:-unknown}, ${RUNNER_OS:-unknown} ${RUNNER_ARCH:-})"
  if [ -r /etc/os-release ]; then . /etc/os-release && echo "os: ${PRETTY_NAME:-unknown}"; fi
  echo "user: $(id -un 2>/dev/null || echo unknown), sudo: $(sudo -n true >/dev/null 2>&1 && echo yes || echo no)"
  for tool in git curl tar gzip zstd sha256sum python3 node jq gh rg cc c++ make cmake pkg-config sudo; do
    printf '  %-10s %s\n' "$tool" "$(command -v "$tool" || echo MISSING)"
  done
  df -h "${GITHUB_WORKSPACE:-.}" || true
} >&2

package_for() {
  case "$1" in
    cc | c++ | make) echo build-essential ;;
    sha256sum) echo coreutils ;;
    *) echo "$1" ;;
  esac
}

missing=()
for tool in "$@"; do
  command -v "$tool" >/dev/null 2>&1 || missing+=("$tool")
done
if [ "${#missing[@]}" -gt 0 ]; then
  as_root=()
  if [ "$(id -u)" -ne 0 ]; then as_root=(sudo -n); fi
  if command -v apt-get >/dev/null 2>&1 && "${as_root[@]}" true >/dev/null 2>&1; then
    packages=()
    for tool in "${missing[@]}"; do packages+=("$(package_for "$tool")"); done
    echo "installing: ${packages[*]}" >&2
    "${as_root[@]}" apt-get -o Acquire::Retries=3 update -qq >&2
    "${as_root[@]}" apt-get -o Acquire::Retries=3 install -y -qq --no-install-recommends "${packages[@]}" >&2
  fi
  still=()
  for tool in "${missing[@]}"; do command -v "$tool" >/dev/null 2>&1 || still+=("$tool"); done
  if [ "${#still[@]}" -gt 0 ]; then
    echo "::error::runner ${RUNNER_NAME:-unknown} lacks ${still[*]} and cannot install them (no apt-get or no passwordless sudo). Add them to the runner image, or set the repository variable CI_RUNNERS=hosted." >&2
    exit 1
  fi
fi

# Build parallelism: one job per CPU, bounded by memory at 2 GiB per job so
# parallel links cannot exhaust an 8 GiB runner. Container limits (cgroup v2)
# win over what the host reports.
cpus="$(nproc 2>/dev/null || echo 2)"
if [ -r /sys/fs/cgroup/cpu.max ]; then
  read -r quota period < /sys/fs/cgroup/cpu.max || true
  if [ "${quota:-max}" != max ] && [ "${period:-0}" -gt 0 ]; then
    limit=$(( (quota + period - 1) / period ))
    [ "$limit" -lt "$cpus" ] && cpus="$limit"
  fi
fi
mem_kib="$(awk '/^MemTotal:/ { print $2 }' /proc/meminfo 2>/dev/null || echo 0)"
if [ -r /sys/fs/cgroup/memory.max ]; then
  cg="$(cat /sys/fs/cgroup/memory.max)"
  if [ "$cg" != max ] && [ $(( cg / 1024 )) -lt "$mem_kib" ]; then mem_kib=$(( cg / 1024 )); fi
fi
by_memory=$(( mem_kib / (2 * 1024 * 1024) ))
jobs="$cpus"
[ "$by_memory" -lt "$jobs" ] && jobs="$by_memory"
[ "$jobs" -lt 1 ] && jobs=1
echo "cpus=$cpus memory_kib=$mem_kib build_jobs=$jobs" >&2
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  echo "- runner \`${RUNNER_NAME:-unknown}\` (${RUNNER_ENVIRONMENT:-unknown}): ${cpus} CPUs, $(( mem_kib / 1024 )) MiB, build jobs ${jobs}" >> "$GITHUB_STEP_SUMMARY"
fi
echo "$jobs"
