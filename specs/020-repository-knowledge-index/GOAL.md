# Goal — SymForge Repository Knowledge Index

```text
/goal complete SpecKit 020 and ship its implementation until every RED/GREEN/VERIFY gate, adversarial review, safety invariant, and publication check is green, without merging prose into code intelligence, serving stale/unsafe evidence, leaking secrets, opening visible Windows child windows, or committing unrelated user work
```

## Start here

Work in `E:\project\symforge` on branch
`feat/repository-knowledge-index`.

Read, in order:

1. `AGENTS.md`
2. `tasks/lessons.md`
3. `specs/020-repository-knowledge-index/HANDOVER-2026-07-22.md`
4. `tasks/todo.md`
5. the canonical SpecKit files linked by that handover

Do not restart discovery or re-litigate settled product boundaries. The handover is
the current campaign state; the SpecKit is the implementation authority.

## Objective

Ship a live, local-first repository-knowledge lane alongside SymForge's existing
code-intelligence lane:

- catalog the whole repository safely;
- index admitted text-centric knowledge with exact source/file/line/span/content
  provenance;
- retrieve bounded evidence immediately without broad file reads;
- keep code symbols, references, code-text search, and code frecency code-only;
- cover watcher, reconciliation, snapshot recovery, worktrees, and local refs;
- detect documentation divergence through typed, code-backed evidence without
  erasing intent, governance, north-star, security, or history material;
- expose review/remediation and guarded cleanup without silent mutation or deletion.

## Measurable completion criteria

- [x] The four remaining blockers in the handover are resolved consistently across
  `spec.md`, `plan.md`, `data-model.md`, `tasks.md`, contracts, and quickstart.
- [x] Local link, identifier, acceptance-traceability, contradiction, and checklist
  checks pass.
- [x] Fresh independent Architect, Skeptic, and Minimalist review is completed;
  every accepted HIGH/MEDIUM finding is resolved and rejected findings carry
  concrete evidence.
- [x] SpecKit status is frozen only after the review gate passes.
- [ ] Gates `A -> B -> C -> D -> E -> F -> G -> H -> I -> J -> K -> L -> M` in
  `tasks.md` complete in order.
- [ ] Every behavior change has observed RED evidence, minimal GREEN implementation,
  and the gate's VERIFY evidence.
- [ ] Existing code-intelligence behavior and the full shipped MCP surface remain
  compatible unless the frozen spec explicitly changes them.
- [ ] Quickstart format, byte, source, security, watcher, snapshot, bridge,
  authority, curation, and crash-recovery matrices pass.
- [ ] No embedding model, vector database, duplicate prose corpus, or generated LLM
  summary is added without a measured failing corpus and explicit user approval.
- [ ] No secret-positive value or derivative reaches output, logs, snapshots, CCR,
  analytics, review artifacts, hashes, or commits.
- [ ] Windows process checks prove zero visible child windows and zero worker/helper
  descendants after completion.
- [ ] Formatting, lint, focused/full tests, platform gates, adversarial code review,
  staged-diff review, and repository status are clean.
- [ ] `tasks/todo.md` contains the final evidence/review receipt.
- [ ] Only then: commit and push `feat/repository-knowledge-index`.

## Tagged constraints

- `[scope]` Repository catalog, knowledge extraction/retrieval, evidence bridge,
  authority hygiene, curation, multi-source freshness, and lifecycle only.
- `[architecture]` Local-first Rust, in-process immutable publication, `.symforge/`
  snapshots when permitted, deterministic source rebuild otherwise.
- `[separation]` Shared scout/catalog; separate code and knowledge targets/query
  scopes; prose never appears in code intelligence.
- `[identity]` Every result is source-local and generation-pinned with exact safe
  path, line/span, and content/object identity.
- `[freshness]` One captured immutable source set per call; stale off-lock work is
  rejected; degraded evidence is labeled and never promoted to current.
- `[admission]` Bound metadata, sniff, per-file, resident-content, and derived-state
  budgets independently. Rejected huge files are never fully read or hashed.
- `[protected-roots]` Automatic home/OS/drive/filesystem/broad-root indexing is
  refused. Exact explicit override never grants source-local state-write authority.
- `[security]` Detection is fail-closed and whole-hit withholding is shared by
  direct and CCR paths. Never echo a matched value.
- `[authority]` Code governs checked current behavior only; intent/governance/history
  retain separately labeled voice. Time alone never archives or deletes.
- `[mutation]` Review is read-only. Curation is explicit, idempotent, durable,
  guarded, crash-recoverable, and user-visible.
- `[simplicity]` Deterministic structural/lexical retrieval first. No speculative
  sidecar, database, parser framework, or abstraction.
- `[process]` Preserve unrelated working-tree changes. Use `apply_patch` for edits.
  Update `tasks/lessons.md` after corrections.
- `[windows]` Delegated agents remain disabled until their Windows descendant
  lifecycle is proven headless and leak-free; apply specialist reasoning locally
  meanwhile. SymForge itself must never open a visible child window.

## Execution loop

For each gate in `tasks.md`:

1. Re-read the gate contract and affected frozen requirements.
2. Write the narrowest named RED test.
3. Run it and record the expected failure.
4. Inspect shared callers and existing helpers; fix the root once.
5. Implement the minimum GREEN change.
6. Run focused tests, affected existing suites, and the gate's VERIFY checks.
7. Inspect impact, state ownership, generation fencing, and failure behavior.
8. Update `tasks/todo.md` with commands, outcomes, and limitations.
9. Continue only while the tree compiles and the prior gate remains green.
10. If code contradicts the frozen contract, stop and re-plan rather than weakening
    the requirement silently.

## Specialist routing

Delegation is conditional on a verified headless/leak-free Windows lifecycle. Until
then, apply each persona locally in the main runner.

| Stage | Specialist | Bounded responsibility |
|---|---|---|
| Spec consistency | `goal-master` | Maintain this goal, acceptance traceability, and gate ordering. |
| Architecture/Rust implementation | `rust-pro` | Implement one gate at a time using existing SymForge boundaries and patterns. |
| External design challenge | `tech-researcher` read-only | Investigate only a concrete unresolved evidence gap; no broad re-research. |
| Correctness review | `code-reviewer` read-only | Review frozen-spec deltas and implementation diff for regressions/missing tests. |
| Security review | `security-reviewer` read-only | Check admission, secret containment, path/root safety, output/CCR, and curation. |
| Verification | `test-runner` | Run exact gate commands, interpret failures, and preserve test intent. |
| Integration | `git-master` | Final staged-scope audit, intentional commit, push, and CI receipt only. |

When delegation becomes safe, use these exact dispatch forms:

```text
Spawn `agent_type="rust-pro"` with: implement only Gate <X> in
E:\project\symforge from the frozen SpecKit; own <files>; preserve all prior gates;
return diff plus RED/GREEN/VERIFY evidence.

Spawn `agent_type="code-reviewer"` read-only with: review the Gate <X> diff and its
frozen requirements for correctness, regressions, unsafe assumptions, and missing
tests; write findings only, make no edits.

Spawn `agent_type="security-reviewer"` read-only with: review admission, path/root,
secret, output/CCR, persistence, and curation boundaries affected by Gate <X>;
write findings only, make no edits.

Spawn `agent_type="test-runner"` with: run the exact Gate <X> focused and affected
suite commands; report exit codes and failures; change no production behavior.
```

After any delegated completion, verify the worker left zero child processes before
accepting its result.

## Terminal condition

Do not stop at “spec complete,” “implementation compiles,” or “tests mostly green.”
This goal ends only when the frozen SpecKit is implemented, all required evidence is
green, accepted reviews are resolved, no unsafe process remains, the exact staged
scope is approved, and the authorized commit/push succeeds.
