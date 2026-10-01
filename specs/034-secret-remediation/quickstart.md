# Quickstart: validating Agent-Driven Secret Remediation

Validation guide for **implementation** follow-on. Spec-track PR is docs-only and uses the Spec-track gates section below.

## Spec-track gates (this PR)

Docs-only; no `src/` changes expected:

```bash
python execution/conventional_commits.py check-range origin/main..HEAD
git diff --name-only origin/main...HEAD   # expect specs/034-secret-remediation/** only (+ HANDOVER already present)
```

Optional sanity: confirm D1 is on the merge base (`git merge-base --is-ancestor d8ee3d0c HEAD`).

## Implementation gates (later PR; HANDOVER §4)

```bash
export CLAUDE_PROJECT_DIR=
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --lib --bins --tests -- --test-threads=1
cargo test --no-default-features --features embed --lib -- --test-threads=1
cargo test --test preventive_runtime_dark_v11
python execution/conventional_commits.py check-range origin/main..HEAD
```

Use synthetic secrets only. Cap cargo parallelism on small VMs (`-j 8` or lower). Never kill mid-write.

## RED-first oracles (implementation)

Every oracle MUST be observed failing before machinery lands (Constitution II). Pair negatives with positives in the same test.

### US1 — Withheld meta

1. `admission_refusal_carries_withheld_meta_without_secret_bytes`
2. `clean_read_omits_withheld_meta`
3. `path_rule_or_unscanned_does_not_invent_content_finding_ids`

### US2 — Externalize

4. `externalize_preview_masks_and_writes_nothing`
5. `externalize_apply_moves_to_gitignore_env_and_rescans_clean`
6. `externalize_apply_is_idempotent_and_rolls_back_on_mid_failure`

### US3 — Encrypt

7. `encrypt_unavailable_without_sops_or_recipient`
8. `encrypt_apply_with_sops_leaves_no_plaintext_value` (env-gated if CI lacks sops)
9. `no_raw_command_spawn_for_sops` (guard inventory)

### US4 — Dismiss

10. `dismiss_binds_content_digest_not_path_rule_alone`
11. `dismiss_revocation_restores_withholding`
12. `dismiss_loader_size_cap_and_no_symlink_escape`
13. `health_does_not_echo_unscanned_dismissal_text`

### Cross-cutting

14. `prompt_injection_file_cannot_skip_write_permission_gate` (document harness assumption)
15. `resolve_repo_path_refusals_hold_for_remediation_scope`
16. `refuse_by_policy_remains_syscall_free` (unit/contract)

## Manual smoke (optional)

1. Index a toy repo containing a synthetic password in JSON.
2. Read the file via MCP — observe refusal + `_meta.symforge/withheld`.
3. `secret_remediate` preview externalize — inspect masked diff.
4. Apply with client approval — confirm `.env` + clean re-read.
