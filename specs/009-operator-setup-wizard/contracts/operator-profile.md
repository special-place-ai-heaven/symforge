# Contract: OperatorSetupProfile (`.symforge/operator-setup.json`)

**Surface**: persisted operator-local state; `cli::operator_profile::{load,save}`.

## Shape
```json
{
  "installation_type": "in-harness | server | both",
  "port": 8787,
  "auth_posture": "loopback-no-key | network-keyed",
  "harnesses": ["claude-code", "cursor"],
  "updated_ms": 1750000000000
}
```

## Rules
- Location: `resolve_symforge_dir(project)/operator-setup.json` (paths.rs:45). Project-local.
- `load()` → `None`/default on missing OR malformed (fresh run; never a hard error, D5).
- `save()` is atomic (temp + rename), mirroring `harness_apply::atomic_write`.
- **No secret material**: `network-keyed` records only that a key is required; the key
  bytes live in the operator's env/keystore, never in this file.
- Consumed by: re-run detection (FR-013) and the admin verb's remembered-port reuse (FR-015).

## Invariant
The persisted `port` is the last **verified-bound** port (not a guess) — so a later
reachability check is meaningful.
