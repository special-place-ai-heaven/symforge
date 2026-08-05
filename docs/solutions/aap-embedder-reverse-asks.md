# SymForge → AAP: reverse asks (what AAP can do to be a better embedder)

as_of 2026-08-04. Audience: the AAP agent/maintainer. Counterpart to AAP's
`docs/solutions/symforge-embed-upstream-asks.md` (received and verified; filed
as symforge task-board #22–#25). Written against symforge main @ `67d494a`
(post rmcp-3.1 migration, post Tier-2 honesty fix), embed facade as it exists
in `src/embed.rs`. Everything below is verifiable by pointing your agent at
`E:\project\symforge` — the facade, its contract test, and the identity code
are all named with paths.

## Context: what symforge already guarantees you

- `symforge::embed` is a **flat, semver-public facade** (`src/embed.rs`) whose
  header declares: breaking any name or signature there is a MAJOR bump. A
  compile-time `contract` test module pins every exported type and every
  function's FULL signature via fn-pointer bindings — drift fails symforge's
  own build before it can reach you.
- Snapshot restore already fails soft on every mismatch class (format version,
  secret-policy version, project identity, manifest/content digest): warn →
  quarantine → cold fallback, never a panic. Your compatibility acceptance
  criterion is met engine-side today; #22 only adds the facade export + probe.

## The asks, ranked

### 1. Flip the adapter to `symforge::embed::*` imports (you already queued it — this is the "why now")

The deep module paths you use (`live_index::store::…`, `parsing::…`) resolve
via back-compat re-exports, but only a SUBSET of deep paths is pinned by the
contract test — the flat facade items are ALL pinned. Until you flip, part of
your surface rides outside the guarantee. **Acceptance:** the adapter compiles
with `use symforge::embed::…` only; no path contains `::store::` or a second
`::` below `embed`.

### 2. Adopt #24 (`update_file`/`remove_file`) promptly when it lands, then DELETE the hand-rolled reindex

This is the ask with symforge-side payoff: your
`process_file` + `IndexedFile::from_parse_result` + store-poking path is the
only external consumer forcing several store internals to stay public. Once
you're on the facade methods, symforge can narrow visibility
(`pub` → `pub(crate)`) and refactor the store freely without risking you.
**Acceptance:** one release after #24 ships, the adapter has no store-mutation
code of its own.

### 3. Specify the trust model for portable snapshots (blocking input to #23 design)

The foreign-state refusal your rooms warm-start trips over is deliberate: a
snapshot is TRUSTED INDEX STATE — content, symbol tables, and the admission
manifest (what was withheld as sensitive). An attacker-supplied `index.bin`
could resurrect content the admission gate refused, or lie about what a file
says. #23 will be an explicit opt-in rebind, and its design needs YOUR threat
model, not a blanket bypass. Answer these in a short addendum to your brief:

- Who builds the baked snapshot, and is that pipeline the same trust domain
  as the room image build?
- Is the workspace layer immutable/read-only from the agent's perspective at
  restore time, or could an in-room agent have replaced `index.bin.zst`
  before a restart?
- Is restore-once-at-boot sufficient, or do you need re-import mid-lifetime?
- Cross-platform is real for you (host Windows/Linux → guest Linux): confirm
  the exact host builder platform so the identity-rebind lane covers it.

**Acceptance:** #23's opt-in parameter can name a concrete provenance
guarantee ("baked by image builder, layer immutable at runtime") instead of
`PortableTrust::JustTrustMe`.

### 4. Keep the "surface consumed" section of your brief current

Your brief's quoted-imports section is what symforge's contract coverage is
audited against. When the adapter's imports change, update that section in the
same commit. Cheap for you, and it keeps the "we don't break this casually"
promise anchored to reality instead of to a stale snapshot of your code.
**Acceptance:** the brief's import block and
`crates/aap-code-intel/src/adapter.rs` never disagree for more than one AAP
release.

### 5. Keep sending receipts, in the testpilot format

The Tier-2 receipt ("advice → refusal loop; search denies existence") went
from report to merged fix (#497) in one session BECAUSE it named the exact
tool calls, the observed output, and the expected behavior. Symptom-first,
tool-call-level receipts for anything the engine does wrong inside a room —
especially under the musl build, where symforge has CI but no live soak — are
the highest-value thing AAP can send. **Acceptance:** a receipt names tool,
input, observed output, expected output, and engine version.

## Non-asks

- No pinning demanded: tracking `main` unpinned is fine — the contract test +
  facade are symforge's side of making that cheap; ask 1 is yours.
- No AAP CI coupling into symforge CI; the contract test already encodes your
  surface, and ask 4 keeps it honest.

## Status of your asks on the symforge board

#22 export + compat probe · #23 portable identity-rebind import (BLOCKED on
ask 3 above for its trust design) · #24 update/remove_file · #25 engine_info +
no-background guarantee (verified: no watchers/timers on load; the one caveat
is the lazy rayon parse pool — persistent compute workers on first parse,
pool-size knob included in the task) + search budgets. Serve-side snapshot
restore (spec 026, PR #500) is in CI now; the embed exports ride behind it.

_Landed on symforge `main`; this path is stable._
