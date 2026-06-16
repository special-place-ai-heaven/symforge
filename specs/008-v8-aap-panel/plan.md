# Implementation Plan: SymForge AAP Operator Panel & Presets (v8 8.1)

**Branch**: `008-v8-aap-panel` (campaign on `review/v8-004-operator-serve`) | **Date**: 2026-06-16 | **Spec**: [spec.md](./spec.md)

## Summary

Add a server-gated `aap` module that detects a sibling AAP checkout (`AAP_ROOT` env → conventional sibling path), reads its `Cargo.lock` for the pinned `symforge` version, and compares it to the running crate version (drift flag). Surface it as `/api/v1/aap` + an AAP panel section in the `006` admin UI, add AAP presets (embed snippet + serve-URL), extend the `005` harness scan with a distinct AAP target, and extend the onboarding banner. No AAP-repo changes; detection is read-only against the sibling + fixtures. All behind `#[cfg(feature="server")]`; embed build stays clean (G-045).

## Technical Context

**Language**: Rust edition 2024 + the existing embedded admin assets. **Deps**: reuse `toml_edit`/`serde`/`sha2` (no net-new); read AAP `Cargo.lock` via `toml_edit` or a targeted parse. **Storage**: none new (read-only detection; AAP writes reuse `005` backup). **Testing**: fixture AAP-shaped dirs (`tests/fixtures/aap/<name>/Cargo.lock` pinning symforge) + `/api/v1/aap` via reqwest; harness AAP-entry + banner tests. **Constraints**: read-only against real AAP; never overwrite AAP embed path dep; server-feature-gated; embed clean. **Project Type**: single crate + embedded UI.

## Constitution Check

Stub constitution → repo gates + invariants:
- **GATE-1**: detection + lock read are read-only; no real AAP checkout mutated in tests (fixtures only).
- **GATE-2**: never replace AAP embed path dep with stdio config; AAP writes backed up (reuse 005).
- **GATE-3**: repo gates green; embed build clean (G-045 preserved).
No violations.

## Project Structure

```text
src/server/
├── aap.rs            # NEW: AapDetection (AAP_ROOT/sibling), read Cargo.lock symforge pin,
│                     #      EmbedPinComparison (pinned vs running crate version + drift),
│                     #      IntegrationMode, AAP presets (embed snippet / serve-URL)
├── admin/api_v1.rs   # MODIFY: GET /api/v1/aap → AapView (detection/mode/versions/drift/roots/presets)
├── admin/assets/*    # MODIFY: AAP panel section (detection, drift warning, copy-snippet, presets)
src/cli/
├── harness.rs        # MODIFY: add an AAP-typed HarnessTarget (detected via AAP_ROOT/sibling), embed-only vs HTTP presets
├── onboarding.rs     # MODIFY: banner mentions /admin AND the AAP embed path when AAP detected
tests/
├── aap_detection.rs      # US1: detect + pin drift/match/unknown on fixtures; not-detected clean
├── admin_aap_api.rs      # US1/US2: /api/v1/aap returns detection/mode/versions/drift/presets; auth enforced
├── aap_harness_preset.rs # US3: AAP listed as distinct target; embed path never overwritten; backup on write
tests/fixtures/aap/    # NEW: drift/ (pins old symforge), match/ (pins running), no-pin/, missing-lock/
```

**Structure Decision**: a focused `src/server/aap.rs` owns detection + pin comparison + presets; the admin/harness/onboarding modules consume it. Reuses `006` `/api/v1` + admin assets and `005` harness/backup. Read-only against AAP; fixtures for tests.

## Phase pointers
- research.md: sibling-path precedence (`AAP_ROOT` then `../Agent_Army_Professionals`); how to extract the symforge pin from `Cargo.lock` (`[[package]] name="symforge" version=...`); running crate version source (`env!("CARGO_PKG_VERSION")`).
- data-model.md: AapDetection, EmbedPinComparison, IntegrationMode, AapPreset + the `/api/v1/aap` AapView DTO.
- contracts (folded): `GET /api/v1/aap` (auth+Origin, read-only); harness AAP target shape; preset content rules (never stdio-overwrite embed).

## Dependencies
Builds on `004`+`005`+`006`. Closes G-044 (E6-E9, A9). G-043/G-045 already satisfied (embed feature present + verified clean every checkpoint). No AAP-repo changes.
