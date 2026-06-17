# Quickstart: v8 Trust Remediation — validation guide

How to prove 010 works end-to-end. Two layers: the **per-phase gate** (mechanical, run
after every phase) and the **acceptance checks** (per user story). Implementation detail
lives in `tasks.md`; this is the run/verify guide.

## Prerequisites
- Repo on `010-v8-trust-remediation`, `target/` kept warm across the campaign.
- The dev MCP harness binary version is irrelevant to correctness (currently 7.27.0);
  010 is verified by `cargo` against the on-disk 8.0.0 source. Live STEL-surface dogfood
  uses the **locally-built** 8.0.0 binary.

## Per-phase gate (FR-019 / SC-007) — run after EACH of phases A–F

```sh
cargo fmt --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --all-targets -- --test-threads=1
cargo build --release
cargo check --no-default-features --features embed   # embed stays network/server-free
```

All six MUST pass before the next phase starts. A check that cannot run is reported
unverified with the reason (Constitution VIII).

## The three named regression tests (must exist + pass)
| Test | Story | Proves |
|------|-------|--------|
| `status_index_matches_daemon_proxy_after_symforge_serve` | US2 | status counts == served index (TR-01). |
| `compact_surface_index_not_loaded_message_never_mentions_blocked_tools` | US4 | no compact recovery string names a gated tool (TR-02). |
| `symforge_edit_if_match_rejected_after_concurrent_disk_change` | US3 | guarded apply rejects a concurrent divergence via injected interleave (TR-06). |

## Acceptance checks (per story)

### US1 — honest labels (Phase A, zero behavior)
- Read every status + envelope field; confirm each is `Measured` or carries
  heuristic/observational/deferred (SC-001). Grep the surfaces for `net`/`saved`/`active`/
  `pending`/`validated`; each survivor must satisfy its word in code.
- Confirm golden replay still passes (zero behavior change).

### US2 — status truth (Phase B)
- Locally build + run `symforge serve`; run a query that populates the index; read
  `status`; confirm `index_state: Ready` with counts matching the served query (SC-002).
- Confirm a wired-but-failing ledger reports `Disabled(reason)`, distinct from
  `Unavailable` (FR-008).

### US3 — edit safety (Phase C)
- Run the TR-06 regression: guarded apply + injected concurrent change ⟹ rejected, on-disk
  change intact, no false "guarded apply succeeded" (SC-003). Negative control succeeds.

### US4 — recoverable cold start (Phase D)
- Simulate a fresh default attach with no pre-indexed workspace: confirm either auto-index
  populates, or the recovery message names only callable actions on the active surface
  (SC-004). Confirm the desktop launch path discovers the project root (not `%USERPROFILE%`).

### US5 — economics grounded (Phase E)
- Run the same op over a small and a large file; confirm predicted figures differ in
  proportion to size (SC-005); confirm a non-serve branch becomes reachable for a small
  request. Confirm `expected_equiv` is asserted or removed.

### US6 — public record + enforced honesty (Phase F)
- Read README / AGENTS.md / CLAUDE.md / init allow-list: confirm they describe the
  compact-3 default with the 32-tool surface as a documented opt-out (SC-006).
- Confirm `docs/v8-capability-matrix.md` maps each capability → assumption ID → proof state.
- Force a violation (a surface claiming a capability whose assumption is OPEN) on a scratch
  branch; confirm the honesty CI gate FAILS (FR-018). Confirm honest OPEN-labeling passes.

## Keystone (SC-008)
Final dogfood with the locally-built 8.0.0 binary: reconnect MCP; `status` compact reports
the real index; `symforge` orient query succeeds; `status` full → counts MATCH the served
query; `symforge_edit` preview is honest. An agent trusting the self-reported numbers/status
is no longer misled — no surface asserts more than the code delivers.
