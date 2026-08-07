# SymForge → AAP: what changed for embedders, 8.18.0 → 10.0.0

as_of 2026-08-07. Audience: the AAP agent/maintainer. Counterpart to
`aap-embedder-reverse-asks.md` (as_of 2026-08-04, written against symforge main
@ `67d494a` = **8.18.0**).

Symforge main is now **10.0.0** — two major bumps. Everything below is verifiable
against `E:/project/symforge-policy` (branch `main`); every claim carries a
`file:line` or a commit.

**Read §1 first — it is the only part that will fail your build.**

---

## 1. Breaking changes — action required

Exactly two breaking commits landed in this range:

```
16b8f5d  fix(embed)!: mark SkipReason non_exhaustive before more variants land (#536)
ec76c79  fix!: report only work that was actually verified (...) (#538)
```

### 1.1 `SkipReason` and `MetadataOnlyReason` are now `#[non_exhaustive]`

`src/domain/index.rs:1459` and `:878`. The comment on both is explicit:
*"reachable from the semver-public embed facade."*

**What breaks:** any exhaustive `match` on either enum. Rust will reject it.

**What to do:** add a `_ => …` arm. Decide deliberately what the fallback means —
a new skip reason is a file symforge declined to index, so treating an unknown
variant as "indexed fine" would be wrong.

```rust
match skip_reason {
    SkipReason::DependencyLockfile => …,
    SkipReason::Unreadable => …,
    // …
    _ => /* treat as: not indexed, reason unknown to this build */,
}
```

The variant `MetadataOnlyReason::PlatformPathCollision` was **deleted** in the same
range (`5a37a4e`). Nothing ever minted it, and the enum is externally-tagged serde,
so no persisted snapshot anywhere contains that string — but if you name it in
source, that source no longer compiles.

`SkipReason::Unreadable` was **added**. It now absorbs three former cases
(`Unreadable | UnstableDuringRead | AbortedCircuitBreaker`), so if you were
distinguishing those, you no longer can from this enum.

### 1.2 `#538` — no embed API signature changed

Despite the `!`, this one changed *behaviour*, not shape: daemon identity
disclosure, the trust envelope deriving currency from measured freshness instead
of a hardcoded string, reconciliation no longer discarding re-parse results, and
project routing. None of it touches an embed signature. **No action.**

---

## 2. New surface since your doc

All four of your board items shipped. Mapped to the ask numbers in
`aap-embedder-reverse-asks.md`:

| Commit | Ships | Your ask |
|---|---|---|
| `c413282` (#501) | snapshot-restore fast path on the facade | #22 |
| `0b49518` (#503) | portable snapshot import with identity rebind | #23 |
| `0355579` (#505) | per-file update/remove through the admission seam | #24 / ask 2 |
| `506fe30` (#507) | `engine_info`, runtime guarantees, indexing-pool cap | #25 |

### 2.1 Per-file incremental update — **this is the one that matters** (`embed.rs:43`)

```rust
pub use crate::live_index::single_file::{ReindexResult, remove_file, update_file_from_disk};
```

`update_file_from_disk` re-indexes ONE file through the **same admission seam** the
watcher and bulk load use: scope exclusions, metadata-first scout, and
secret-content demotion.

**Your hand-rolled reindex does not.** See §4 — this is the most important item in
this document.

### 2.2 `engine_info()` — readiness evidence, one call, no I/O (`embed.rs:67-79`)

```rust
pub struct EngineInfo {
    pub version: &'static str,              // symforge crate version
    pub snapshot_format_version: u32,       // on-disk snapshot format
    pub secret_policy_version: u32,         // detector policy at admission + raw reads
    pub grammars: &'static [&'static str],  // every supported grammar, stable lowercase
}
```

All compile-time constants. Use it as the compatibility probe you asked for in #22:
compare `snapshot_format_version` and `secret_policy_version` before trusting a
baked artifact, and log `version` in any receipt you send back.

`grammars` is now **25** entries.

### 2.3 Snapshot restore + portable import (`embed.rs:131`)

```rust
pub use crate::live_index::persist::{
    IndexSnapshot, PortableSnapshotProvenance, import_portable_snapshot, load_snapshot,
    load_snapshot_for_root, project_local_state_placement, snapshot_compatible,
    snapshot_to_live_index,
};
pub use crate::domain::StatePlacement;
```

`import_portable_snapshot` is the explicit opt-in rebind from ask 3. It rebinds
**path-derived identity only**; every content-derived check stays enforced, and a
second in-process attempt is refused (restore-once). Bake the artifact host-side
with `live_index::persist::export_artifact` — still a deep path, not on the flat
facade.

`snapshot_compatible` is the cheap pre-check. All mismatch classes still fail soft:
warn → quarantine → cold fallback, never a panic.

### 2.4 Durable STEL ledger — **not in your doc, entirely new** (`embed.rs:152-160`)

```rust
pub use crate::stel_core::ledger_store::{
    LedgerStoreStatus, LedgerSubsystemState, LedgerSummary, StelLedgerStore, StoredLedgerRecord,
};
pub use crate::stel_core::calibration::{
    CalibrationVerdict, StelCalibrationSummary, format_calibration_section, summarize_calibration,
};
pub use crate::stel_core::types::{
    AdmissionDecision, IntentBucket, RouteConfidence, StelLedgerEvent,
};
```

An embedder can now open the durable per-project economics ledger, record events,
read them back, and summarize calibration — **without the server or transport
stack**. `StelLedgerStore::open(project_root, session_id)` → `.record(&event)`
(degrades silently, never panics) → `.recent(limit)` / `.summary()`.

This is worth a look for AAP: it is the mechanism for measuring whether code
intelligence is actually paying for itself per room, using symforge's own
accounting rather than one you build.

### 2.5 Runtime guarantee, now stated in the source (`embed.rs:49-52`)

> constructing or loading an index under the embed feature spawns **no watchers,
> timers, or async runtimes** — the file watcher is server-only and caller-started,
> never by `LiveIndex::load`.

The one caveat from your doc still stands: the lazy rayon parse pool spawns
persistent compute workers on first parse. The pool-size knob shipped with #507.

---

## 3. In flight — a break that would have hit you

**Status: being fixed as this is written. Do not act yet; this is a heads-up.**

A cold-start defect was fixed on `fix/cold-start-ready-before-rooted`: an index
could report `Ready` before it had bound a repository root, publishing knowledge
with no source identity. **That lane does not exist under embed** — no watcher, no
detached load — so AAP was never affected by the bug.

The **fix**, however, made `index_state()` return `Loading` whenever
`load_source == EmptyBootstrap`. And `LiveIndex::empty()` + `add_file` — the
construction your adapter uses — produces exactly that. Under the unrepaired fix,
an AAP-built in-memory index would report `Loading` **permanently**, with
`from_source_files` sitting behind `pub(crate)` so there was no public alternative.

This was caught in review before merge, specifically because a reviewer read
`embed.rs` and its `_assert_named` contract test. A public path to a Ready
in-memory index is being added before that branch lands.

**Why you are being told about a bug that never reached you:** it is the clearest
possible evidence for §4. The adapter reaches past the facade into store
internals, so a change to symforge's internal state machine changed AAP's
observable behaviour. On the facade methods, the facade would have absorbed it.

---

## 4. Do this now: delete the hand-rolled reindex

Your own doc's ask 2 says to adopt the facade methods and delete the hand-rolled
`process_file` + `IndexedFile::from_parse_result` + store-poking path, with the
payoff framed as *symforge can then narrow visibility and refactor freely*.

Those methods shipped in `#505`. Your acceptance criterion — "one release after
#24 ships, the adapter has no store-mutation code of its own" — is now due.

**There is a second reason your doc does not record, and it is the stronger one.**

The hand-rolled path bypasses the admission gate. Every lane that reopens a file
from disk routes through `admit_disk_read` (`src/protocol/read_gate.rs:92`), which
owns the read and returns the buffer only on a permit verdict. Secret-content
demotion happens there. A path that parses a file and pokes the result into the
store never crosses that seam.

**Concretely: an agent that edits a credential into a file inside a room gets it
re-entered into the index unscanned.** Symforge's own admission gate would have
withheld it.

That is why `#505` was built. It appears in the PR body and nowhere else — the
changelog line reads as a convenience feature, and the README never mentioned it.
If you read one sentence in this document, read this one.

**Migration:** replace the parse-and-poke path with `update_file_from_disk` for
changes and `remove_file` for deletions. `ReindexResult` tells you what actually
happened — `Reindexed`, `HashSkip`, `Skipped`, `NotFound`, `ReadError`. Do not
discard it; `Skipped` and `ReadError` mean the file is **not** in the index, and
treating them as success is the exact class of defect symforge spent two days
removing from its own code (see the reporting invariant in `CLAUDE.md`).

---

## 5. Still blocked on you

`#23`'s trust design still needs your threat model. Four questions from
`aap-embedder-reverse-asks.md` §3 remain unanswered:

1. Who builds the baked snapshot, and is that pipeline the same trust domain as
   the room image build?
2. Is the workspace layer immutable from the in-room agent's perspective at
   restore time, or could it have replaced `index.bin.zst` before a restart?
3. Is restore-once-at-boot sufficient, or do you need re-import mid-lifetime?
4. What is the exact host builder platform (host Windows/Linux → guest Linux)?

`import_portable_snapshot` shipped with a conservative default. Answering these
lets its provenance parameter name a real guarantee instead of a blanket opt-in.

---

## 6. Also worth knowing

- **The published token-economics claim was withdrawn.** The README claimed
  70–95% savings citing a file that does not exist; the one committed benchmark
  measured the *opposite* for the full surface (4,032 vs 2,262 tokens, 1.78x) and
  its own verdict was "do not promote as a token-efficient default." All
  unsupported figures are removed. If AAP has been repeating those numbers
  anywhere, stop. A fresh benchmark against 10.0.0 has not been run.
- **Binding repo policy, new today:** *a component may not report success for an
  operation whose completion it did not observe.* Recorded in `CLAUDE.md`. You can
  hold symforge to it, and it is the standard `ReindexResult` handling above is
  measured against.
- **Documentation now exists.** The embed surface had one README bullet before
  today. There is now an `Embedding-the-Engine` wiki page and a real README
  section.
- **Receipts still land.** Your Tier-2 receipt went report → merged fix in one
  session because it named tool, input, observed output, expected output. Keep
  sending them in that shape, especially under musl where symforge has CI but no
  live soak.

---

## 7. How to verify any of this

```bash
git -C E:/project/symforge-policy log 67d494a..origin/main --oneline -- src/embed.rs
git -C E:/project/symforge-policy show origin/main:src/embed.rs
```

`src/embed.rs` is the contract. Its `contract` module pins every exported type and
every function's full signature via fn-pointer bindings, so drift fails symforge's
build before it reaches you. Anything not exported there — including
`export_artifact` — rides outside that guarantee.
