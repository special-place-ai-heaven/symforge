# SymForge v8 documentation

> **External LLM / new contributor:** start at **[`v8-bootstrap.md`](v8-bootstrap.md)** — single entry point with full session context, code checklist, and links to depth.

Branch: `v8/stel-architecture` · Shipped today: **7.21.1** · Target: **8.0.0** → **8.1.0**

We write ideation down first, validate assumptions, then implement. Details deepen in linked docs.

---

## Reading order

| Doc | Role |
|-----|------|
| **[`v8-bootstrap.md`](v8-bootstrap.md)** | **START HERE** — bootstrap brief for external LLMs (whole v8 session memory) |
| [`ideation.md`](ideation.md) | Vision, principles, non-goals, decision log |
| [`v8-gap-closure-plan.md`](v8-gap-closure-plan.md) | **Binding** pre-flight — gaps, spikes, harness specs; blocks `src/stel/` until §12 green |
| [`v8-master-plan.md`](v8-master-plan.md) | Phased roadmap summary |
| [`v8-architecture-diagrams.md`](v8-architecture-diagrams.md) | 13 mermaid diagrams |
| [`stel-architecture.md`](stel-architecture.md) | STEL charter, gates H1–H8 |
| [`stel-schema.md`](stel-schema.md) | Normative types, controller algorithm |
| [`stel-assumptions.md`](stel-assumptions.md) | Assumption register A-001.. |

---

## External measurement

| Artifact | Role |
|----------|------|
| `E:\project\sf-bench\` | Measurement harness (methodology + corpus) — optional sibling repo |
| `results-v8-8.0-baseline.json` | Pinned at **8.0 tag** — v8 regression reference |
| `sf-bench/RESULTS.md` | **7.21.1 appendix** — informational only |

---

## Phase crosswalk (canonical)

| Master plan | stel-architecture | Delivers |
|-------------|-------------------|----------|
| **0** | Phase 0 | Harness trust, L0 A/B, pre-flight §12 |
| **1** | Phase 1 | Compact surface **H1** |
| **2** | Phase 2 | Router + controller **H3, H4, H5** |
| **3** | Phase 3 | Ledger → **8.0.0** + pin v8 baseline |
| **4** | Phase 4 | **H6, H8** + `symforge serve` → **8.1.0** |

---

## How docs evolve

```text
v8-bootstrap.md   → single file for external LLM / onboarding
ideation.md       → why + decisions
gap-closure       → every gap closed before code
stel-*            → how (types, gates, assumptions)
sf-bench          → proof methodology (not 7.x scores for v8 gates)
```

When direction changes, update **`v8-bootstrap.md` + ideation decision log** first.
