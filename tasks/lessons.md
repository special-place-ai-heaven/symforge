# Lessons

## 2026-05-19 - Keep RTK Adoption Surgical

When importing task packages from an external planning agent, do not treat every RTK surface as default implementation work. First verify against current SymForge code and classify each item as ready, decision-only, evidence-gated, already covered, or rejected.

Rules for future RTK/SymForge task generation:
- No RTK runtime dependency, shared crate, API coupling, shell-hook surface, command rewriting, CLI-output filtering, OpenClaw, Homebrew, HTTP telemetry, `panic = "abort"`, or `lazy_static`.
- Analytics is product-decision-first. Do not create analytics storage or instrumentation tasks until an ADR explicitly accepts persistent local analytics.
- Frecency, parser pooling, regex/glob/Aho-Corasick caching, worktree env caching, and config-registry cleanup are benchmark/evidence-gated, not default backlog.
- Integrity sidecars are scope-decision-first because SymForge does not currently have RTK's installed-hook auto-approval risk surface.
- Trust work starts with ADR 0015 before code, then minimal pure trust core, then control-surface/warning integration.
- Active goal chains should be small, code-grounded, and executable; supersede broad generated packages rather than leaving ambiguous competing task chains active.
