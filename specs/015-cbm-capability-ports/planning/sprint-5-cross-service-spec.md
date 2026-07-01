# Sprint 5 Planning Spec — Cross-Service & Architecture

**Status**: draft  
**Release**: 8.14.x  
**User stories**: US11, US12  
**Depends on**: S2 graph, S3 resolver (recommended)

## Objective

HTTP route ↔ handler linking and call-graph **architecture clusters** for orient intent.

## US11 — Routes

### CBM deep-read

- `internal/cbm/ac.c` — pattern scan (reference)
- Route extraction passes in pipeline (grep `Route` in pipeline/)

### v1 framework scope ([P] P-S5-005)

| Framework | Language | Patterns |
|-----------|----------|----------|
| axum | Rust | `Router::route`, macro routes |
| express | TS | `app.get/post`, `Router()` |

### Edge kinds

- `Route` node + `HANDLES` edge to handler symbol
- `HttpCall` client edge (stretch — document defer if needed)

### Fixture

`tests/fixtures/cbm_routes_axum/` — minimal axum 0.7 app, 2 routes.

## US12 — Clusters

### CBM deep-read

- `store/store.c` Leiden community detection
- `get_architecture` response shape in mcp.c

### Algorithm choice ([P] P-S5-010 — closes PD-02/D-015-010)

| Option | Pros | Cons |
|--------|------|------|
| Label propagation | Simple, fast | Less accurate |
| Leiden port | CBM parity | More code |

**Deliverable**: benchmark on symforge index — pick one with metrics.

### Surface choice (PD-02)

- Option A: `get_architecture` new full-surface tool
- Option B: `get_repo_map(detail=architecture)`

Recommend **B** for STEL orient absorption — decide at Gate.

## Planning Gate

- [ ] PD-02 closed
- [ ] D-015-010 filled
- [ ] axum fixture spec approved
- [ ] Cluster output format mock in format.rs sketch

**Sign-off**: _________________ Date: _______
