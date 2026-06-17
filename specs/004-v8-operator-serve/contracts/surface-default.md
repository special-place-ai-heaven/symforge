# Contract: Default tool surface = compact-3

## Selection (existing `surface_profile_from_env`)
| `SYMFORGE_SURFACE` value | Profile | Tools |
|--------------------------|---------|-------|
| unset / unrecognized | **`Compact` (NEW default)** | `symforge`, `symforge_edit`, `status` (3) |
| `compact` | `Compact` | same 3 |
| `full` | `Full` (opt-out) | legacy full surface |
| `meta` | `Meta` | meta surface |

## Change
Only the default arm of `surface_profile_from_env` flips `Full` → `Compact`. Explicit `full`/`meta`/`compact` values are unchanged. Applies to BOTH stdio and `/mcp` (single selection path).

## Backward compatibility (FR-009)
- Clients requiring the 32-tool surface set `SYMFORGE_SURFACE=full`. This is the documented escape hatch; the change must be called out in release notes.

## Acceptance (FR-008/009, SC-004)
- No env set → `tools/list` returns exactly the 3 compact tools.
- `SYMFORGE_SURFACE=full` → `tools/list` returns the legacy surface unchanged.
- Conformance test covers both (mirror existing surface conformance tests so legacy expectations under `full` still pass).
