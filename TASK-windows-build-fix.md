# Task: SymForge does not compile on Windows

`symforge 11.1.5` cannot be built for `x86_64-pc-windows-msvc` on any toolchain. Four
errors, all from two lines. Fix them.

Nobody noticed because the MCP server running on this machine is a **prebuilt 11.1.4 from
npm** (`node_modules/symforge-windows-x64`). Every query on this box goes through that
binary, so the environment behaves as though SymForge builds here. It does not.

## The failure, reproduced

`src/index_lifecycle/physical_root.rs:98-99`:

```rust
#[cfg(windows)]
fn observe_physical_root_anchor(path: &Path) -> Option<PhysicalRootAnchor> {
    use std::os::windows::fs::MetadataExt;
    let metadata = std::fs::metadata(path).ok()?;
    Some(PhysicalRootAnchor {
        dev: metadata.volume_serial_number(),   // <- both lines
        ino: metadata.file_index(),             // <- are wrong twice
    })
}
```

Two independent problems stacked on each line:

1. **`volume_serial_number()` and `file_index()` are behind the unstable
   `windows_by_handle` feature.** Not stable on any channel. `E0658`, twice.
2. **Both return `Option<u64>` and are assigned to `u64` fields.** `E0308`, twice.

Verified on three toolchains:

| toolchain | result |
|---|---|
| stable 1.97.1 | 4 errors |
| stable 1.96.0 (this repo's own `rust-toolchain.toml` pin) | 4 errors |
| nightly | **5** — the same four, plus API drift on `fetch_update` → `try_update` |

Reproduce: `cargo check --no-default-features --features embed`

**Adding `?` to both lines fixes only the type error.** The unstable-feature error survives,
so that is not the fix.

## The fix

**The `windows` crate 0.62 is already a dependency** (`Cargo.toml:159`, under
`[target.'cfg(windows)'.dependencies]`). No new dependency is needed.

`GetFileInformationByHandle` returns a `BY_HANDLE_FILE_INFORMATION` carrying
`dwVolumeSerialNumber`, `nFileIndexHigh` and `nFileIndexLow` — the same identity the
unstable std methods expose, on stable. Open the path with `FILE_FLAG_BACKUP_SEMANTICS`,
which is required to open a **directory** handle; `observe` is called on repository roots,
so it is always a directory.

Compose the two index halves as `(high as u64) << 32 | low as u64`.

## What this identity is for — do not silently weaken it

The doc comment states it: *"Observed physical object at one path. Detects same-path
replacement (ABA) without rekeying the path convergence map."*

If a directory is swapped for a different one at the same path, the anchor changes and
SymForge notices. **Returning `None` on Windows would compile and would remove that
detection**, and every caller already handles `Option`, so nothing would fail loudly. That
is the tempting wrong fix.

If you conclude `None` is genuinely correct on Windows, say so explicitly in the report
with your reasoning, and change the doc comment so it stops claiming a guarantee the
platform no longer provides. Do not leave the comment asserting a property the code
abandoned.

## Callers, for context

`observe()` already returns `Option<Self>` and every caller handles it:

```
src/index_lifecycle/activation.rs:427, 946, 978
src/live_index/store.rs:1357, 1519
```

## Tests

**Every test must be capable of failing.** After writing them, remove each guard in turn,
run the suite, and paste the **real `cargo test` output** from each mutated run — a genuine
flip shows `N-1 passed; 1 failed` with the failing test named, and `passed + failed`
identical in both worlds. A table of the word FLIPPED is not evidence.

Required:

- Two directories at different paths yield different anchors.
- The same directory observed twice yields the same anchor.
- **A directory replaced by a different directory at the same path yields a different
  anchor.** This is the ABA property the type exists for, and it is the one that fails
  silently if the implementation degrades to `None`.
- A path that does not exist yields `None` rather than an error or a panic.
- A file rather than a directory is handled — decide and state whether it is `Some` or
  `None`, and test what you decided.

## Acceptance

```
cargo check --no-default-features --features embed     # the build that fails today
cargo test                                             # the existing suite, unchanged
```

Paste both real outputs. **If any currently-passing test goes red, stop and report rather
than fixing it.**

State the toolchain you built with. If it only builds on nightly, that is a failure, not a
result — the whole point is a stable Windows build.

## Why this is urgent

Fathom links SymForge as a path dependency for its code intelligence, which is the
capability the entire product rests on. Its first end-to-end run against a real repository
could not start, because the only build that can index anything is
`cargo build --features symforge`, and that does not compile.

`crates/fathom-repo/Cargo.toml` already carries a comment saying SymForge does not build on
Windows. The comment is accurate and understates it: even nightly fails.

## Out of scope

The `fetch_update` → `try_update` drift that appears only on nightly. This repository pins
stable 1.96.0; fix the four stable errors and leave nightly alone.
