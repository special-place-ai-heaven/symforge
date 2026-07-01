# Sprint 1b Planning Spec — Search rank + hooks

**Status**: draft  
**Release**: 8.10.1  
**User stories**: US3–US4  
**Depends on**: S1a shipped (detect_impact + artifact)

**Balance**: [agent-workload.md](./agent-workload.md) — 2 agent waves, ≤4 [C] per wave.

## Objective

Ship discovery improvements without coupling to impact/artifact work (S1a).

## In scope

| US | Deliverable | Tasks |
|----|-------------|-------|
| US3 | Graph-augmented search rank + search `mode` | C-S1B-001..002 |
| US4 | PaginationEnvelope + hook augment | C-S1B-003..004 |

## Out of scope (moved)

- IndexMode Fast/Standard/Deep → **S4** (pairs with semantic Deep pass)
- detect_impact, team artifact → **S1a**

## Planning Gate

- [ ] P-S1B-001..007 complete
- [ ] EV-S1-003..004 refreshed in code-evidence.md

**Sign-off**: _________________ Date: _______

## Linked tasks

[tasks.md](../tasks.md) § Sprint 1b
