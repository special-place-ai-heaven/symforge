# A-019 — L0 surface choice (compact-3 vs meta-tool vs full-32)

**Tasks:** T031–T032  
**Verdict:** **OPEN — BLOCKED**

## Blocker

- **B-SFBENCH:** Full battery A/B requires sf-bench harness.
- **B-COMPACT-STUB:** Compact/meta surface filters not implemented (would be non-shipping stub per gap plan — not attempted this session).

## Candidates

| Candidate | Description | Battery result | Equivalence |
|-----------|-------------|----------------|-------------|
| compact-3 | 3-tool STEL compact surface | — | — |
| meta-tool | 1–2 meta-tools replacing 32-tool surface | — | — |
| full-32 | Legacy full surface (informational) | — | — |

## Selection rule

Winner = highest **accepted-session net** while preserving equivalence. If no candidate wins → document blocking pivot.

## Decision

**No winner selected.** Phase 1 L0 shape **not locked**.

**A-019 verdict:** OPEN (blocked)

## Next action

1. Restore sf-bench workspace.
2. Land non-shipping `SYMFORGE_SURFACE` measurement stub (Phase 0.7).
3. Run A/B battery and record winner or pivot here.
