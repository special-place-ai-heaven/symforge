# Contract: `symforge setup` (US2)

**Surface**: a new clap subcommand `Commands::Setup` (cli/mod.rs) → `cli::setup::run`.

## Args (`SetupCliArgs`)
| Flag | Type | Meaning |
|------|------|---------|
| (none) | — | interactive: scan → present summary → prompt → confirm → apply. |
| `--non-interactive` | bool | drive with pre-supplied answers; no terminal read, no browser, no network probe beyond the bind (FR-014). |
| `--installation-type <in-harness\|server\|both>` | enum | pre-answer the install type (E2). |
| `--port <u16>` | u16 | preferred bind port (else the verified-free suggestion). |
| `--harnesses <id,...>` | list | which detected harnesses to configure (else all detected). |
| `--yes` | bool | auto-confirm the restated action plan (for scripts). |

## Flow (FR-004→FR-013)
1. **Scan (read-only)**: `HarnessRegistry::known()` + reachability of any remembered
   server → summary of OS, per-harness state, running-server, suggested free port.
   Changes nothing (FR-004).
2. **Choose**: install type (E2), harness subset, port (pre-filled free suggestion) (FR-005/006).
3. **Restate**: print the exact actions (files, server) and require confirm / `--yes` (FR-008).
4. **Apply**: `harness_apply::plan` → `apply` (restorable backups, idempotent, BOM-safe) (FR-009).
5. **Server mode**: start serve on a verified-free port; on reachability, report dashboard
   + attach URLs; offer browser open (FR-010/011).
6. **Persist**: write `OperatorSetupProfile` (FR-012).
7. **Re-run**: detect existing profile + running server → refresh/no-op, never duplicate (FR-013).

## Guarantees
- Non-interactive defaults (spec Assumption): configure detected harnesses, both types,
  loopback no-key, verified-free port.
- Every modified config has a timestamped restorable backup; re-apply adds no duplicate (SC-002).
- Reported URL is reachable on the bound port (SC-005, FR-020).
- All side effects via seams; fixtures-only verification (FR-017/018).
