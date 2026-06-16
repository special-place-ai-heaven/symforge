# SymForge v8 — Release Notes

User-facing behavior changes for the v8 line. The release-please-managed
`CHANGELOG.md` records per-version commit history; this file is the hand-curated
home for v8 breaking changes and migration guidance that need a human-readable
call-out (the `CHANGELOG.md` preamble is scoped to `embed`-facade changes only).

## Breaking: default `tools/list` surface is now compact-3

**What changed.** The default MCP tool surface advertised by `tools/list`
shrinks from the legacy 32-tool surface to the compact-3 surface
(`symforge`, `symforge_edit`, `status`). This applies to BOTH transports —
stdio and the `/mcp` Streamable HTTP server — because both resolve the surface
through the single selection path `protocol::surface_probe::surface_profile_from_env`.

**Why.** The compact-3 surface keeps the `tools/list` schema payload under the
H1 budget (<= 5,000 bytes) and routes all intelligence through the STEL facade
(L0 -> L1..L4) inside one MCP call, instead of eagerly advertising every handler.

**Opt-out / backward compatibility.** Clients that require the legacy full
surface set the environment variable:

```
SYMFORGE_SURFACE=full
```

This restores the legacy 32-tool surface unchanged. The other explicit values
are unaffected:

| `SYMFORGE_SURFACE` | Profile | `tools/list` |
|--------------------|---------|--------------|
| unset / unrecognized | `Compact` (NEW default) | `symforge`, `symforge_edit`, `status` (3) |
| `compact` | `Compact` | same 3 |
| `full` | `Full` (opt-out) | legacy full surface |
| `meta` | `Meta` | meta surface (1 tool) |

**Scope of the change.** Only the default arm of `surface_profile_from_env`
flips `Full` -> `Compact`. The internal tool router still registers every
handler; the compact surface only changes what is *advertised*, not what is
*reachable* (the facade dispatches to the same handlers). The init allowlist
(`SYMFORGE_TOOL_NAMES` in `src/cli/init.rs`) and the registration conformance
suite are unaffected — they describe the registered handler set, not the
advertised surface.

See `specs/004-v8-operator-serve/contracts/surface-default.md` (US2, FR-008/009)
for the full contract and `tests/surface_default_compact.rs` for the conformance
test.
