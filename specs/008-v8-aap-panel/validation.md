# 008 AAP Panel — Validation (2026-06-16)

Verified on `review/v8-004-operator-serve` (worktree `E:\project\symforge-review`). Implemented across `9c6136d` (foundational), `b2967ef`+`dd21bb2` (US1/US2), `7deb465` (US3), finished + verified in the main loop after transient API overload killed the implementing subagents mid-run (work was committed in slices and never lost).

## Gates
- `cargo check` — GREEN
- `cargo clippy --all-targets -- -D warnings` — GREEN
- 008 tests (`--test-threads=1`):
  - `aap_detection` — GREEN (drift/match/pin-unknown/missing-lock; not-detected clean)
  - `admin_aap_api` — **6/6** (drift reported; no-pin → pin-unknown no false drift; not-detected empty-state; presets embed-always/serve-url-when-active/never-stdio; unauth keyed → rejected; bad Origin → rejected)
  - `aap_harness_preset` — **6/6** (AAP env-path detection; banner mentions /admin + embed; embed-only default, HTTP only when serve active; embed path never a stdio-spawn config; not-detected → no presets)
- Full-suite regression + `build --release` + embed-clean: run at the checkpoint gate.

## Success-criteria mapping
- **SC-001** drift detected + warned / match → no drift → `admin_aap_api` drift + no-pin tests.
- **SC-002** no AAP sibling → clean not-detected, rest unaffected → `aap_not_detected_is_clean_empty_state`.
- **SC-003** embed snippet always / serve-URL only when active / never stdio overwrite → `aap_presets_*` + `embed_path_dep_is_never_a_stdio_spawn_config`.
- **SC-004** AAP a distinct harness target → `aap_target_env_path_detects_fixture_root`.
- **SC-005** embed build free of server code → existing embed-clean gate (run at checkpoint).

## Scope note
G-044 (E6-E9 / A9) implemented: AAP panel (detect sibling, integration mode, embed-pin drift), presets, AAP-aware harness target + banner. **G-043 (AAP embed retained) and G-045 (embed isolation) were already satisfied** and verified clean at every prior checkpoint. No AAP-repo changes; detection is read-only against fixtures (the real `E:\project\Agent_Army_Professionals` checkout was never mutated).

## Residual / honest
- Verified against fixture AAP dirs; a live AAP-panel render against the real sibling repo is an optional supplementary check.
- `indexed_roots` surfaces what the running server knows; deep AAP-backend project enumeration (live AAP DB) is out of scope (E4 future).
