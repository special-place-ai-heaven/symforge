# Claude Workflow Review — Recovered Artifact

> Recovered mechanically from the interrupted Claude workflow. No new model call was made.

## Recovery status

- Original requested synthesis/report was never written.
- Raw workflow output is preserved at `docs/reviews/wplq7sl80.output`.
- Raw artifact size: 300,612 bytes.
- Workflow launched 54 agents.
- Workflow reports: 10 reviewers completed, 21 findings survived its internal verification, 0 were refuted by both verifier lenses.
- Many verifier jobs failed after Claude exhausted the five-hour quota.
- This is evidence for local inspection, not a completed merge-safety verdict.

## Confirmed findings visible in the recovered output

1. `detect_impact` applies its new source-focused default without disclosing filtered paths when the result becomes empty.
2. `what_changed` can report “No uncommitted changes matched” when only non-source files changed, without explaining the default filter or `code_only=false`.
3. The current `code_only` classifier excludes unrecognized but legitimate source types such as SQL, shell, PowerShell, Proto, Terraform, Dockerfile, and Makefile.
4. Feature 018 contract documentation does not describe browse-mode `(name, kind)` deduplication or the new `detect_impact include_data` behavior.
5. The overlay browse implementation in `src/live_index/view.rs` lacks the `(name, kind)` deduplication added to the primary engine.
6. Default compact `get_repo_map` still uses a weaker path-containment heuristic that can admit parent-relative or UNC-style out-of-root paths.

## Next step

Parse and verify every recovered finding locally against the final working tree before merge. Do not invoke Claude, Fable, workflows, or delegated reviewers.
