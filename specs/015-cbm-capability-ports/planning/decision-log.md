# Decision Log — Program 015

Format: **D-015-NNN** — immutable once `[C]` starts for that sprint unless superseded.

| ID | Date | Decision | Rationale | Alternatives rejected | Sprint |
|----|------|----------|-----------|----------------------|--------|
| D-015-001 | 2026-06-29 | Graph is derived projection in LiveIndex, not SQLite | Constitution I, 007 FR-015 | CBM-style SQLite authority | Program |
| D-015-002 | 2026-06-29 | New tool `detect_impact` vs extend `what_changed` | One-call agent UX; keep what_changed stable | Merge into what_changed only | S1 |
| D-015-003 | 2026-06-29 | zstd team artifact on index.bin not separate graph db | Single snapshot authority | CBM graph.db.zst clone | S1 |
| D-015-004 | 2026-06-29 | Rust resolver before Go/Python | SymForge dogfood; CBM priority aligned | Go first | S3 |
| D-015-005 | 2026-06-29 | Algorithmic semantic before embeddings | Lower cost; AGENTS.md start simple | Nomic bundle in S4 | S4 |
| D-015-006 | 2026-06-29 | Cypher v1: no variable-length paths | Complexity; fail-closed | Full CBM subset day one | S2 |
| D-015-007 | 2026-06-29 | 60/30/10 execution model | Reduce implementation surprises | Code-first port | Program |
| D-015-008 | 2026-06-29 | **Deferred** — Snapshot v5 vs memory-only ResolvedCall | Resolve at S3 Planning Gate | Premature v5 break | S3 |
| D-015-009 | 2026-06-29 | **zstd** for team artifact compression | Matches FR-002; CBM parity; better ratio than gzip | gzip-only (R-07 fallback doc) | S1 |
| D-015-010 | 2026-06-29 | **Deferred** — Leiden vs label-propagation | Resolve at S5 spike (P-S5-005) | Full Leiden port day one | S5 |
| D-015-011 | 2026-06-29 | Defer BM25 FTS; S1 structural rank only | SymForge trigram path; avoid SQLite FTS creep | CBM BM25 in S1 | S1 |
| D-015-012 | 2026-06-29 | Daemon alias `detect_changes` → `detect_impact` + warn | CBM migrator ergonomics | No alias | S1 |
| D-015-013 | 2026-06-29 | **Superiority doctrine** — default adopt CBM; skip **inferior parts only** | User charter: superior SymForge | Copying inferior mechanisms (Soul Map, etc.) | Program |

## Resolved pending (Speckit clarify 2026-06-29)

| Was | Resolution |
|-----|------------|
| PD-03 BM25 | **Closed** → D-015-011 defer |
| PD-04 detect_changes alias | **Closed** → D-015-012 yes+warn |
| D-015-009 zstd vs gzip | **Closed** → zstd |

## Still pending (sprint Planning Gate)

| ID | Question | Resolve at | Owner |
|----|----------|------------|-------|
| PD-01 | Snapshot v5 for ResolvedCall vs overlay-only? | S3 Planning Gate | Engine |
| PD-02 | `get_architecture` tool vs `get_repo_map(detail=architecture)`? | S5 Planning Gate | Protocol |

## Template for new entries

```markdown
### D-015-NNN — Title
- **Date**:
- **Context**:
- **Decision**:
- **Rationale**:
- **Rejected**:
- **Sprint**:
- **Supersedes**: (none)
```
