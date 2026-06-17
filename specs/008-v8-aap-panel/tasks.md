# Tasks: SymForge AAP Operator Panel & Presets (v8 8.1)

**Feature dir**: `specs/008-v8-aap-panel/`. Inputs: [plan.md](./plan.md), [spec.md](./spec.md), [`docs/v8-aap-integration.md`](../../docs/v8-aap-integration.md) (E6-E9/A9).
**Gates each phase**: `cargo fmt --check`, `cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets -- --test-threads=1`, `cargo build --release`, embed-clean. All under `#[cfg(feature="server")]`. Read-only against AAP; fixtures only — never mutate a real AAP checkout.

## Phase 1: Setup
- [ ] T001 Create `tests/fixtures/aap/`: `drift/Cargo.lock` (pins symforge an old version), `match/Cargo.lock` (pins the running version), `no-pin/Cargo.lock` (no symforge package), `missing-lock/` (dir, no Cargo.lock).
- [ ] T002 [P] Confirm in `research.md`: sibling precedence (`AAP_ROOT` → `../Agent_Army_Professionals`), `Cargo.lock` symforge-pin extraction, running version via `env!("CARGO_PKG_VERSION")`.

## Phase 2: Foundational — `src/server/aap.rs`
- [ ] T003 Implement `AapDetection::resolve()` (env `AAP_ROOT` then conventional sibling; detected/not-detected + source) and `read_symforge_pin(root) -> Option<String>` (parse `Cargo.lock` `[[package]] name="symforge"`).
- [ ] T004 Implement `EmbedPinComparison` (pinned vs `env!("CARGO_PKG_VERSION")`; `Drift`/`Match`/`PinUnknown`) and `IntegrationMode` (embed/MCP-URL/both/none from configured state). Register `#[cfg(feature="server")] pub mod aap;`.
- [ ] T005 [P] Unit tests `tests/aap_detection.rs`: drift/match/unknown/missing-lock on fixtures; not-detected when no root.
- [ ] T006 **GATE** embed isolation clean (`cargo check --no-default-features --features embed`).

## Phase 3: US1 — AAP panel (P1) 🎯 MVP
- [ ] T007 [US1] `admin/api_v1.rs`: `GET /api/v1/aap` → `AapView { detected, root, mode, pinned_version, running_version, drift, indexed_roots }`; behind the 006 auth+Origin layer.
- [ ] T008 [US1] Admin UI AAP panel section (assets): detection state, integration mode, pinned vs running version, drift WARNING when drifted, "AAP not detected" clean empty-state.
- [ ] T009 [P] [US1] `tests/admin_aap_api.rs`: `/api/v1/aap` returns correct detection/mode/versions/drift for fixtures; unauth non-loopback → 401; bad Origin → rejected.
- [ ] T010 [US1] **GATE** repo gates green.

## Phase 4: US2 — AAP presets (P2)
- [ ] T011 [US2] `aap.rs`: `embed_cargo_snippet()` (always when detected: `symforge = { path = "../symforge", features = ["embed"] }`) and `serve_url_preset(attach_url, key)` (only when serve active). NEVER emit a stdio-spawn config for the AAP embed path.
- [ ] T012 [US2] Admin UI: copy-snippet for embed + serve-URL preset in the AAP panel.
- [ ] T013 [P] [US2] Tests: embed snippet always present for detected AAP; serve-URL preset only with active serve; assert no stdio-spawn config is ever produced for the embed dep.
- [ ] T014 [US2] **GATE** repo gates green.

## Phase 5: US3 — AAP-aware harness scan + banner (P3)
- [ ] T015 [US3] `src/cli/harness.rs`: add a distinct AAP-typed target (detected via `AAP_ROOT`/sibling, not a generic MCP JSON); offer embed-only (default) vs HTTP presets; any write reuses 005 backup and never overwrites the embed path dep.
- [ ] T016 [US3] `src/cli/onboarding.rs`: when AAP detected, the banner mentions both `/admin` and the AAP embed path.
- [ ] T017 [P] [US3] `tests/aap_harness_preset.rs`: AAP listed as a distinct target on a fixture root; embed path never overwritten; backup on write; banner text includes embed path when AAP detected.
- [ ] T018 [US3] **GATE** repo gates green.

## Phase 6: Polish
- [ ] T019 [P] Close G-044 in `docs/v8-gap-closure-plan.md` (E6-E9/A9 done); note G-043/G-045 already satisfied.
- [ ] T020 [P] `specs/008-v8-aap-panel/validation.md`: SC-001..005 evidence (fixtures + render note).
- [ ] T021 **GATE** Final: all repo gates green; embed clean. Checkpoint → commit, merge to main, push (standing rule). Optional live AAP-panel render check (browser) against the real sibling `E:\project\Agent_Army_Professionals`.

## Dependencies
```text
Setup(T001-2) → Foundational(T003-6) → US1(T007-10, MVP) → US2(T011-14) → US3(T015-18) → Polish(T019-21)
```

## Out of scope
AAP-repo/adapter changes; E4 future (shared serve, vsock); writing AAP's live backend DB. G-043/G-045 already done.
