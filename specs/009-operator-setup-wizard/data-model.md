# Phase 1 Data Model: Operator Setup Wizard

Entities from the spec's Key Entities, grounded in the reuse map. Most reuse existing
types (`HarnessRegistry`/`HarnessState`, `AuthConfig`); the only new persisted type is
`OperatorSetupProfile`.

## E1 — OperatorSetupProfile (NEW, persisted)

`<project>/.symforge/operator-setup.json`. Mirrors `OnboardingState` (`onboarding.rs:20`).

| Field | Type | Rule |
|-------|------|------|
| `installation_type` | enum `InHarness \| Server \| Both` (E2) | the chosen setup shape (FR-005). |
| `port` | `u16` | last bound/preferred operator-server port (FR-012); re-runs + admin reuse it. |
| `auth_posture` | enum `LoopbackNoKey \| NetworkKeyed` | reflects the bind (FR-007); `NetworkKeyed` never stores the key bytes — only that a key is required. |
| `harnesses` | `Vec<HarnessId>` | which harnesses were configured (for idempotent re-run / refresh). |
| `updated_ms` | `i64` | last-write stamp. |

**Rules**: `load()` returns `None`/default on missing or malformed (fresh run, never a
hard error — D5). `save()` is atomic (temp+rename, like `harness_apply::atomic_write`).
No secret material is persisted (the key lives in the operator's env/keystore, not here).

**State**: `Absent` → *(wizard completes)* → `Present{...}` → *(re-run)* → refresh or
no-op (FR-013).

## E2 — Installation type

`enum InstallationType { InHarness, Server, Both }` — selects which actions the wizard
performs (FR-005). `InHarness` configures stdio harness entries only (no dashboard);
`Server` starts the operator server + dashboard; `Both` does both. Choosing
dashboard access implies a server mode (spec Assumption — the dashboard is serve-only).

## E3 — Harness target (REUSE)

Reuses `HarnessRegistry` + `HarnessId` (harness.rs:28) + `HarnessState` (harness.rs:98):
`NotInstalled | Absent | PresentCurrent | PresentStale | Malformed(reason)`. The wizard
renders this read-only in the scan summary (FR-004) and selects a subset to configure
(FR-006). No new type — the wizard consumes the existing scan.

| Source field | From | 009 use |
|---|---|---|
| id / human name | `HarnessId` | display + selection |
| config location | `HarnessRegistry::known_with(home, working_dir)` | apply target (fixtures via temp home) |
| state | `HarnessState` | "not installed / configured-current / stale / unreadable" summary |

## E4 — Server session descriptor (NEW, transient — not persisted)

The running operator server as the wizard/admin sees it.

| Field | Type | Rule |
|-------|------|------|
| `bound_addr` | `SocketAddr` | the actually-bound address (D1); source of all reported URLs (FR-020). |
| `dashboard_url` | `String` | `http://<bound_addr>/admin` (ADMIN_PATH). |
| `attach_url` | `String` | the `/mcp` attach URL on the same address. |
| `reachable` | `bool` | HTTP GET `/api/v1/summary` succeeded within the timeout (D6) — only then are the URLs reported. |

**Invariant (FR-020)**: a URL is reported only when `reachable == true` for the
`bound_addr` it names — no advertised-but-dead URL.

## E5 — Port candidate (NEW, transient)

A port being evaluated for the default/suggested bind.

| Field | Type | Rule |
|-------|------|------|
| `port` | `u16` | candidate (preferred = `DEFAULT_LISTEN`'s 8787, else OS-assigned via `:0`). |
| `free` | `bool` | verified by an actual bind attempt at evaluation time (D1) — not a guess. |

**Invariant (US1)**: the wizard suggests / serve binds only a `free == true` candidate;
an explicit occupied address never substitutes (fails loud, FR-003).

## E6 — Seams (NEW, behavioral — not data)

Injectable boundaries for testability (D7, FR-017): `SetupSink` (ask/confirm/summary/
status), `BrowserOpener` (open_url → Opened|Skipped). Real impls do terminal/OS I/O; test
impls (`ScriptedSetupSink`, `NoopBrowserOpener`) record and assert with zero side effects.

## Relationships

```text
Wizard run ──reads──▶ HarnessRegistry scan (E3, reuse)
           ──writes─▶ OperatorSetupProfile (E1)  [.symforge/operator-setup.json]
           ──uses───▶ harness_apply::{plan,apply,write_backup} (reuse, restorable backups)
           ──binds──▶ PortCandidate (E5) ──▶ ServerSessionDescriptor (E4)
           ──through▶ SetupSink + BrowserOpener (E6 seams)
Admin verb ──reads──▶ OperatorSetupProfile.port ──reachability(E4)──▶ reuse-or-start
```

Every reported URL traces to a `reachable` `bound_addr`; every config write traces to a
restorable backup; every prompt/open traces through a seam.
