# GROVE handover: SymForge secret detection precision + agent-driven secret remediation

Owner: the repository owner. Handed over 2026-09-30 from the Claude orchestration session.
Repo: `https://github.com/special-place-ai-heaven/symforge` (the owner develops on Windows 11; you work in your own clone).
Product: SymForge, a Rust MCP server that indexes repositories and serves code and knowledge to LLM agents.
Its first duty is to NEVER disclose secret bytes to an agent.

---

## 0. Setup and isolation rules (read first, non-negotiable)

You work on your OWN cloud machine, in your own clone. The owner's machine and other agents'
branches are never touched.

1. Clone from latest `main` (spec docs already merged via #743). Implement on a fresh branch:
   `git clone https://github.com/special-place-ai-heaven/symforge.git && cd symforge && git fetch origin && git checkout -b feat/034-secret-remediation-impl origin/main`
   Deliverable 1 is already on `main` (#738) — do not reopen it. Do not reuse the closed spec branch name `feat/034-secret-remediation`.
2. Install symforge itself (for its MCP code-navigation tools) into ONE npm prefix, and make sure
   that prefix's bin is on PATH. Cloud VMs often set `NPM_CONFIG_PREFIX` to a different folder than
   `~/.npmrc`, which silently installs updates where PATH never looks. Be explicit:
   `npm install -g symforge@latest --prefix ~/.npm-global && export PATH="$HOME/.npm-global/bin:$PATH" && hash -r && symforge --version`
   Only linux-x64, macOS and windows-x64 builds exist; there is no linux-arm64 package.
3. Work only on your own branches. Other agents merge to `main` while you work, so rebase onto
   `origin/main` before every PR update, and force-push only your own branches, with `--force-with-lease`.
4. Never commit the owner's `AGENTS.md` changes (you will not have them; do not recreate them).
5. Never print, log, commit, or paste a real secret. Tests use synthetic values only
   (for example `<40-alnum>` shapes generated in the test).
6. Run builds headless; never open GUI or console windows on any shared desktop.
7. Always run cargo with `CLAUDE_PROJECT_DIR` unset or empty. Some tests honour it and would
   otherwise write into whatever checkout it names.
8. Keep this HANDOVER.md in the spec folder as the brief of record; add your spec-kit files beside it.

## 1. Merge authority (owner's explicit instruction for THIS track)

You may merge your PRs to `main` yourself ONLY when ALL of these hold:
- Every CI job is green on the exact head you merge: rust, windows-native, embed-build, embed-musl,
  darwin-serve-port, npm, release-version, conventional-commits.
- An independent review by a seat that did NOT write the code (LINUS, with HOLMES on contested
  points) returns MERGE, with every BLOCKER and MAJOR fixed and re-checked.
- For security logic, each new test is shown RED on the unfixed code and GREEN on the fix
  (revert the fix, run the test, restore). A test that passes either way proves nothing.
- The branch is rebased onto current `origin/main`, and the rebase changed nothing but the pin (see §4).
Merge command (squash only, explicit conventional subject AND body; the default squash body breaks
release-please):
`gh pr merge <N> --squash --delete-branch --subject "<conventional title> (#<N>)" --body "<one short plain paragraph, no colons at line starts, no parentheses>"`
Release-please then cuts a release automatically; do not hand-edit versions or CHANGELOG.

## 2. Deliverable 1: withhold plaintext credential values under quoted keys — DONE on main

**Status (implement track):** landed on `main` via #738 (`d8ee3d0c`). Do **not** re-implement.
Quoted credential keys (`"password": "…"` and peers) are withheld when the value looks like a
credential; label-like / placeholder values stay Clean. `SECRET_POLICY_VERSION` is 7. Precision
matrix row `PJ1` is Sensitive.

Owner decision (historical): a plaintext password in a repository file must be withheld from the
agent. That work is complete; this HANDOVER's remaining track is Deliverable 2 (§3 / feature 034).

## 3. Deliverable 2: agent-driven secret remediation (spec-kit feature 034)

Goal: when SymForge withholds a secret, the agent gets actionable options. SymForge then fixes the
file LOCALLY, without the agent ever seeing the secret or any key.

Process: this repo uses spec-kit. Use `.specify/` and the speckit flow in order:
specify, clarify, plan, tasks, analyze, implement. Create `specs/034-secret-remediation/`.
Never skip a phase. The spec PR (docs only) can merge before implementation.

Design (settled; the spec must encode it, and review may challenge it with evidence):
1. **Actionable refusal.** When content is withheld, add structured metadata to the tool result
   (`_meta["symforge/withheld"]`). Per finding include:
   - a stable finding id and the rule id;
   - the line range;
   - the value's SHAPE, never bytes (for example "40-char base62 after api_key =");
   - a confidence level;
   - the available remediation actions.
   The human-readable refusal text stays as today. Today's refusal renderer is
   `format::content_withheld_by_admission` and `content_withheld_by_path_rule` in
   `src/protocol/format.rs`; line ranges come from `recorded_refusal_naming_lines` in
   `src/protocol/read_gate.rs`.
2. **One tool**, for example `secret_remediate(scope, finding_ids, action, preview=true)`.
   - `scope` is one file, a list of files, or the whole repo.
   - Actions:
     - `externalize`: move the value to a gitignored `.env` (create it and add the `.gitignore`
       entry if missing), and rewrite the use in the file's own idiom (`process.env.X`,
       `os.environ["X"]`, `std::env::var("X")`, `${X}` for config formats).
     - `encrypt`: delegate to SOPS with age on JSON, YAML, TOML or ENV values. Encryption needs only
       the recipient PUBLIC key, taken from `.sops.yaml` or `SOPS_AGE_RECIPIENTS`, so SymForge never
       holds a private key. If `sops` is not installed, report this action as unavailable.
     - `dismiss`: record a reviewed false positive in a committed file, bound to the content digest
       of that line, so a later real secret on the same path is still caught. This is where the
       deferred per-repo allowlist returns, redesigned.
   - `preview` returns a MASKED diff (values shown as `«secret:N»`) plus the list of files that would
     be created or modified. `apply` writes atomically with rollback and is idempotent (reuse the
     edit lane's apply and idempotency pattern in `src/protocol/edit_tools.rs`).
3. **Approval by the human, not the agent.** `apply` is a write tool, so the harness permission
   prompt is the approval gate. Content in the repo (a prompt injection) must never be able to
   trigger an apply without that prompt.
4. **Honest report after apply** (repo rule: never report success you did not observe):
   - which files were written;
   - a fresh re-scan result for them;
   - a note that the old value remains in git history since commit X.
5. **Never:**
   - a tool that returns secret bytes;
   - a key as a tool argument;
   - a decrypt action for the agent;
   - a custom crypto format;
   - a write without preview and approval.
Past lessons to respect:
- An earlier allowlist was rejected for five reasons: it bound path+rule instead of content; the
  refusal handed the agent a self-service bypass; revocation was never observed; it was read
  through symlinks with no size cap; and health echoed unscanned allowlist text. `dismiss` must avoid all five.
- `refuse_by_policy` in `read_gate.rs` must stay syscall-free, because it is an existence-oracle
  contract. Disk reads happen only in the read lanes.
- Every caller-supplied path goes through `discovery::resolve_repo_path` (symlink, alias, case and
  credential-path refusals).

## 4. Repo gates (every PR, locally before push)

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --lib --bins --tests -- --test-threads=1
cargo test --no-default-features --features embed --lib -- --test-threads=1
cargo test --test preventive_runtime_dark_v11
python execution/conventional_commits.py check-range origin/main..HEAD
```

- **FULL_SOURCE_PIN_V1.** Any `src/` change fails `preventive_runtime_dark_v11`. Confirm the diff
  adds no reference into `src/index_lifecycle`, then paste the failing assertion's LEFT tuple into
  the pin and rerun. After a rebase, only the pin conflicts; recompute it once at the tip.
- **Embed gate.** A `#[cfg(test)]` helper used only by server-gated code must be
  `#[cfg(all(test, feature = "server"))]`, or the embed gate fails on an unused import.
- **Spawning processes.** Never add a raw `std::process::Command` spawn outside
  `process_util::hidden_command` (the guard test `test_no_raw_command_spawns_outside_hidden_command`).
- **Builds.** Never kill a build mid-write; that corrupts `target/`. Recover by deleting
  `target/debug/incremental`. Clean `target/` when done; it grows past 20 GB. Cap parallelism
  (for example `-j 8`) on small VMs.
- **Known flakes, not yours.** Hook and sidecar loopback tests can fail locally with os error 10054,
  and `daemon_proxy_success_without_receipt_does_not_reuse_home_evidence` fails in suite order on
  main too. CI is the authority.
- **Reading code.** Use symforge's own MCP tools if available (`get_symbol`, `search_text`,
  `find_references`) rather than whole-file reads; `src/protocol/tools.rs` is 1.5 MB.

## 5. Seat routing (suggested)

- LARRY: implements against the named seams above, one deliverable at a time.
- LINUS: independent review per PR. HOLMES: second opinion on contested findings. Their brief is
  the diff plus the goal, not the implementer's conclusions.
- JEZ: CI watching, pin refresh flow, and release follow-through after merge.
- JUNIO: branch, worktree, rebase and merge operations under §0 and §1.
- GROVE: owns sequencing (Deliverable 1, then the spec PR, then implementation), and the final report.

## 6. Final report to the owner (one message, short)

- PR numbers with merge SHAs, or why each is not merged.
- For each deliverable: what is proven (CI links, RED/GREEN test names, review verdicts) versus assumed.
- Anything skipped, blocked, or left for the owner to decide.
