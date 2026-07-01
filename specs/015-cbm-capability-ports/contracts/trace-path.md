# Contract: trace_path

**Feature**: 015 · **Sprint**: S2 · **US**: US6

## Tool surface

**Full**: `trace_path`  
**STEL**: `symforge` intent=`trace` (upgrade from single-hop find_references)

## Input

| Field | Required | Default |
|-------|----------|---------|
| `name` or `symbol` | yes | — |
| `path` | no | disambiguate same-named symbols |
| `direction` | no | `both` |
| `depth` | no | 3 (max 5) |
| `mode` | no | `calls` (future: `data_flow`, `cross_service`) |
| `include_tests` | no | false |

## Output

```json
{
  "start": {"name","path","kind"},
  "paths": [[{"symbol","hop","edge_kind"}]],
  "truncated": false,
  "pagination": {...}
}
```

## Backward compatibility

- Existing `find_references` unchanged (single-hop, flat list).
- `trace_path` alias not required (SymForge uses one name).

## Disclosure

- When multiple definitions match name, require `path` (existing find_references policy).
- Resolver-unresolved calls shown with `confidence < 1.0` marker.
