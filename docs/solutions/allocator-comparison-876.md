# Allocator comparison for issue #876

Measured 2026-11-15. Decision: the `symforge` binary uses jemalloc on Linux
glibc and keeps the system allocator on every other target. The `malloc_trim`
after a project unload stays.

## Method

`examples/allocator_876/` is a standalone package (its own workspace, never
part of symforge's dependency graph or a shipped binary). Its features pick the
global allocator: none (system), `mimalloc`, `jemalloc`. One run copies this
repository's tracked files minus `tests/fixtures/` (1,617 indexed files), loads
an index, publishes 60 single-file edits through `update_file_from_disk` (the
watcher's seam) holding the last 4 published generations, releases them, then
drops the index. It reads resident memory (Linux `VmRSS`, Windows working set)
at each step, again after 20 idle seconds ("settled"), and optionally after
`malloc_trim(0)`.

```text
cargo build --release --manifest-path examples/allocator_876/Cargo.toml [--features mimalloc|jemalloc]
ALLOC876_CHANGES=60 ALLOC876_SETTLE_SECS=20 [ALLOC876_TRIM=1] <target>/release/allocator-876
```

Release profile identical to the root manifest (fat LTO, one codegen unit),
same `[patch]` set. Configurations were interleaved, five runs each; tables
show medians. Linux is Ubuntu 24.04 under WSL2 (glibc 2.39), Windows is
Windows 11 MSVC. Memory in MiB, times in ms. The Windows checkout has CRLF line
endings, so its corpus is slightly larger; compare within a platform only.

Versions actually built: jemalloc 5.3.1 (`tikv-jemallocator` 0.7.0,
`tikv-jemalloc-sys` 0.7.1, feature `background_threads`). mimalloc 3.6.1, the
newest upstream release, through `mimalloc-safe` 0.1.70 with feature `v3`
(`libmimalloc-sys2` 0.1.66). The common `mimalloc` crate 0.1.52 bundles 3.3.2
(and 2.3.2), so it was not used.

## Linux glibc

| Configuration | loaded | loaded, settled | 4 pinned | released, settled | released, trimmed | unloaded, settled | unloaded, trimmed | peak | load | edit mean |
|---|---|---|---|---|---|---|---|---|---|---|
| system + trim (today) | 673 | 673 | 1157 | 1157 | 451 | 450 | 230 | 1154 | 1603 | 38 |
| jemalloc | 746 | 482 | 1011 | 494 | - | 288 | - | 1012 | 1670 | 35 |
| jemalloc + trim | 737 | 478 | 1004 | 492 | 303 | 99 | 99 | 1007 | 1596 | 34 |
| jemalloc, decay 1 s | 720 | 466 | 964 | 492 | - | 285 | - | 993 | 1698 | 35 |
| mimalloc 3.6.1 | 1207 | 1207 | 1655 | 1655 | - | 1655 | - | 1655 | 1664 | 33 |

"Trim" calls `malloc_trim(0)` after the release and again after the unload.
Without a trim the system allocator held 1150 MiB after the unload (earlier
round below). The daemon today trims only after an unload, so with the system
allocator a loaded project that keeps publishing sits at the "released,
settled" figure: 1157 MiB, against 494 MiB with jemalloc.

Earlier round (20 edits, 5 s settle, three runs each), for the configurations
not repeated above:

| Configuration | loaded | 4 pinned | released | unloaded | unloaded, trimmed | load | edit mean |
|---|---|---|---|---|---|---|---|
| system, no trim | 672 | 1150 | 1150 | 1149 | - | 1797 | 45 |
| system + trim | 663 | 1142 | 1142 | 439 | 230 | 1517 | 46 |
| `GLIBC_TUNABLES` arena_max=2, trim_threshold=128 KiB | 330 | 807 | 758 | 722 | - | 2932 | 54 |
| same tunables + trim | 321 | 797 | 744 | 324 | 87 | 3109 | 56 |
| mimalloc, `MIMALLOC_PURGE_DELAY=100` | 1159 | 1616 | 1616 | 1616 | - | 1616 | 31 |

A second round with `MIMALLOC_PURGE_DELAY=0` (30 s settle) still held 980 to
1000 MiB after the release and 680 to 830 MiB after the unload.

## Windows MSVC

jemalloc does not build for MSVC, so only two allocators were compared.

| Configuration | loaded | 4 pinned | released, settled | unloaded, settled | peak | load | edit mean |
|---|---|---|---|---|---|---|---|
| system (today) | 295 | 787 | 427 | 101 | 819 | 2267 | 86 |
| mimalloc 3.6.1 | 539 | 964 | 964 | 964 | 968 | 2107 | 38 |

## Decision

- jemalloc on Linux glibc: after settling, 28 percent lower resident memory
  once loaded, 13 percent lower with generations pinned, 57 percent lower once
  they are released, and peak 12 percent lower. Load time within 5 percent
  (the same binary measured 1670 and 1596 ms in two interleaved series), edits
  no slower. Adopted.
- glibc tunables: the lowest memory of all, but load 60 to 90 percent slower
  and edits about 20 percent slower. Rejected by the 5 percent rule.
- mimalloc: much faster edits on Windows, but it held everything it ever
  mapped on both platforms in this workload, even with an immediate purge
  delay. Rejected.
- Windows, macOS and musl keep the system allocator: Windows already returns
  memory (101 MiB after the unload), and macOS and musl were not measured.
- The trim stays: under jemalloc the bundled C libraries still allocate
  through glibc, and with the trim the unloaded process settled at 99 MiB against 288 MiB
  without it.

`health` prints which allocator serves the process in its Memory section. The
jemalloc line appears only when a probe allocation through the global
allocator moved jemalloc's own counter; otherwise it says the allocator was not
identified.

## Operator tuning

jemalloc reads `_RJEM_MALLOC_CONF` at start-up, for example
`_RJEM_MALLOC_CONF=dirty_decay_ms:1000,muzzy_decay_ms:1000` to return freed
pages sooner, or `background_thread:false` to stop background purging. There
is no run-time switch back to glibc malloc: the global allocator is fixed when
the binary is built.

## Not measured

The affected server itself, macOS, musl, and a load larger than this
repository.
