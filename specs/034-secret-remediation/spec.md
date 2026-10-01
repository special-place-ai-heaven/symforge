# Feature Specification: Agent-Driven Secret Remediation

**Feature Branch**: `feat/034-secret-remediation`

**Created**: 2026-09-30

**Updated**: 2026-10-01

**Status**: Ready for review (spec track; implementation deferred)

**Input**: User description: "When SymForge withholds a secret, the agent gets actionable options (shape, confidence, remediation actions) via structured metadata without ever seeing secret bytes. SymForge then remediates LOCALLY via one tool — externalize to gitignored .env, encrypt with SOPS/age (public key only), or dismiss a reviewed false positive bound to line content digest — always preview-first with human harness approval on apply, and an honest post-apply report including a fresh re-scan and the note that the old value remains in git history."

**Prerequisite (Deliverable 1, already on main)**: Quoted credential-key withholding (`"password": "…"` and kin) landed as `#738` / `d8ee3d0c`. This feature builds on that detector precision; it does not re-open detector matching rules except where remediation must re-scan after apply.

**Clarifications (2026-10-01)**: Settled design from `HANDOVER.md` §3 is treated as authoritative. Open points resolved as Assumptions below — no `[NEEDS CLARIFICATION]` markers remain. Clarify phase complete before plan.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Actionable Refusal Metadata (Priority: P1)

An automated coding agent asks SymForge to read or search a file that contains a withheld secret. Today the agent receives a human-readable refusal that names the path, rule ids, and line ranges — but nothing structured it can act on, and no safe remediation path. With this feature, the same refusal text remains, and the tool result additionally carries machine-readable metadata describing each finding's stable id, rule, line range, value SHAPE (never bytes), confidence, and the remediation actions currently available. The agent can propose a remediation to the human without ever seeing the secret.

**Why this priority**: Without actionable metadata, remediation cannot be targeted; every later story depends on findings being addressable. SymForge's first duty is still never disclosing secret bytes — metadata must enrich the refusal without widening disclosure.

**Independent Test**: Trigger an admission-policy refusal on a file with a known synthetic secret; assert the human refusal text is unchanged in meaning, `_meta["symforge/withheld"]` is present with one finding per matched rule instance, each finding includes shape (not value), and no tool output / meta / log contains the secret bytes.

**Acceptance Scenarios**:

1. **Given** a repository file with a synthetic credential that admission withholds, **When** an agent invokes a read/search tool that hits the refusal, **Then** the response includes the existing human-readable refusal and `_meta["symforge/withheld"]` listing each finding with id, rule id, line range, shape, confidence, and available actions.
2. **Given** the same withheld file, **When** any response channel is inspected (text, `_meta`, logs), **Then** no secret byte sequence from the file appears.
3. **Given** a path-rule refusal (credential filename such as `.env`), **When** the tool returns, **Then** withheld metadata either lists path-rule findings with remediation limited to what is safely available for that class, or explicitly marks remediation unavailable for path-rule-only refusals — never invents content-finding ids for unscanned bytes.
4. **Given** a file with no secret findings, **When** a normal read succeeds, **Then** `_meta["symforge/withheld"]` is absent.

---

### User Story 2 - Preview and Externalize (Priority: P1)

The agent (or operator) calls a single remediation tool with one or more finding ids and action `externalize`, with `preview=true` (default). SymForge returns a MASKED diff (secret values shown as `«secret:N»`) and the list of files that would be created or modified — including a gitignored `.env` and a `.gitignore` entry if missing — rewritten in the file's own idiom (`process.env.X`, `os.environ["X"]`, `std::env::var("X")`, `${X}` for config). After the human approves via the harness write-tool permission prompt, the same call with `preview=false` / apply writes atomically with rollback and is idempotent. The agent never receives the secret or any key material.

**Why this priority**: Externalize is the primary safe remediation for developer secrets in source; preview-first plus harness approval is the approval gate. Together with US1 this is the MVP.

**Independent Test**: Seed a tracked file with a synthetic secret; run preview and assert masked diff + planned paths; run apply after simulated approval and assert `.env` holds the value, source uses env lookup, `.gitignore` covers `.env`, and a re-read of the source file no longer withholds for that finding.

**Acceptance Scenarios**:

1. **Given** a withheld content finding in a JS/TS, Python, Rust, or config file, **When** `secret_remediate` is called with `action=externalize` and `preview=true`, **Then** the result is a masked diff and a file list — no secret bytes, no writes.
2. **Given** a successful preview, **When** apply runs (human-approved write), **Then** writes are atomic with rollback on failure, idempotent on repeat, and the secret is moved to a gitignored `.env` (created and gitignore-updated if needed) with the source rewritten in that file's idiom.
3. **Given** apply completed, **When** the tool reports success, **Then** the report names written files, includes a fresh re-scan result for them, and notes that the old value remains in git history since a named commit (or "uncommitted working tree" when no commit exists yet).
4. **Given** content in the repository that tries to instruct the agent to apply without approval, **When** apply is attempted, **Then** apply still requires the harness write permission prompt — repository text alone never triggers a write.

---

### User Story 3 - Encrypt with SOPS/age (Priority: P2)

For JSON, YAML, TOML, or ENV values where the operator prefers encryption in place, the agent selects `action=encrypt`. SymForge uses SOPS with age, taking only the recipient PUBLIC key from `.sops.yaml` or `SOPS_AGE_RECIPIENTS`. SymForge never holds a private key. If `sops` is not installed, the action is reported unavailable (in withheld metadata and in the tool result) rather than failing opaquely. Any `sops` subprocess MUST go through the repo's hidden-command spawn helper so no console window opens.

**Why this priority**: Valuable for ops/config secrets, but depends on operator tooling and public-key setup; externalize covers the common developer case first.

**Independent Test**: With `sops` present and a public recipient configured, preview+apply encrypt on a synthetic value; assert ciphertext lands, plaintext absent from the file, and re-scan is clean for that finding. With `sops` absent, assert action unavailable.

**Acceptance Scenarios**:

1. **Given** `sops` installed and a public recipient available, **When** encrypt preview/apply runs on a supported format, **Then** the value is SOPS-encrypted in place and the agent never sees plaintext or any private key.
2. **Given** `sops` is not installed (or no recipient is configured), **When** withheld metadata or the tool lists actions, **Then** `encrypt` is marked unavailable with a clear reason — never partially applied.
3. **Given** any encrypt attempt, **When** process spawn is audited, **Then** no raw `std::process::Command` appears outside `process_util::hidden_command`.

---

### User Story 4 - Dismiss Reviewed False Positive (Priority: P2)

An operator reviews a finding as a false positive and chooses `action=dismiss`. SymForge records the dismissal in a committed project file bound to the content digest of that line (not merely path+rule), so a later real secret on the same path is still caught. This is the redesigned return of the deferred per-repo allowlist, avoiding the five rejected failure modes of the earlier design.

**Why this priority**: Necessary for operable precision, but must not become a self-service bypass handed to the agent through the refusal text alone.

**Independent Test**: Dismiss a synthetic false-positive line; assert subsequent reads of the unchanged line succeed; change the line bytes to a new synthetic secret and assert withholding returns; assert dismissal file has size caps / no symlink follow / health does not echo unscanned dismissal text.

**Acceptance Scenarios**:

1. **Given** a reviewed false positive, **When** dismiss apply succeeds, **Then** a committed dismissal record bound to that line's content digest exists, and re-read of the unchanged line is no longer withheld for that finding.
2. **Given** a dismissal on path P for digest D, **When** the line at P changes to a different secret, **Then** withholding applies again — path+rule alone never silences a new secret.
3. **Given** the dismissal store, **When** compared to the five rejected allowlist failure modes, **Then** none recur: no path+rule-only bind; refusal text does not hand the agent a self-service bypass; revocation is observable; reads do not follow symlinks unconstrained and observe a size cap; health does not echo unscanned allowlist/dismissal text.

---

### Edge Cases

- Multiple findings in one file: remediation may target a subset by `finding_ids`; preview/apply MUST only touch named findings.
- Scope of whole-repo remediation: MUST still resolve every path through `discovery::resolve_repo_path` and MUST refuse credential-directory roots and symlink/alias/case tricks the same way other write tools do.
- `refuse_by_policy` remains syscall-free (existence-oracle contract); disk reads for remediation happen only in read/remediate lanes, never inside `refuse_by_policy`.
- Unscanned / undecodable files: no content finding ids; remediation MUST NOT invent shapes or claim a re-scan it did not perform.
- Concurrent edits between preview and apply: apply MUST detect conflict (edit-lane pattern) and refuse rather than clobber; report observed failure, not success.
- Idempotent re-apply after success: second apply reports already-remediated / no-op with observed evidence, not a fresh false success story.
- Git history note when the secret was never committed: report working-tree / uncommitted honestly rather than inventing a commit id.
- Encrypt on unsupported format: action unavailable for that finding, not a custom crypto format.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: When content is withheld by admission for secret findings, the tool result MUST include `_meta["symforge/withheld"]` with per-finding: stable finding id, rule id, line range, value SHAPE (never bytes), confidence, and available remediation actions.
- **FR-002**: Human-readable refusal text MUST remain; metadata MUST NOT replace or weaken it. Refusal text MUST NOT embed a self-service bypass instruction.
- **FR-003**: No tool, `_meta` field, log line, preview, or apply report MAY return secret bytes or private key material to the agent.
- **FR-004**: The system MUST expose exactly one remediation tool (name `secret_remediate` unless review renames for registry consistency) with parameters covering scope (one file, file list, or whole repo), `finding_ids`, `action` (`externalize` | `encrypt` | `dismiss`), and `preview` (default true).
- **FR-005**: `preview=true` MUST perform no writes and MUST return a masked diff (`«secret:N»`) plus the list of files that would be created or modified.
- **FR-006**: Apply (`preview=false`) MUST be a write tool gated by the harness permission prompt; repository content alone MUST NOT be able to trigger apply.
- **FR-007**: Apply MUST write atomically with rollback on failure and MUST be idempotent, reusing the edit lane's apply and idempotency patterns.
- **FR-008**: `externalize` MUST move the value to a gitignored `.env` (create file and `.gitignore` entry if missing) and rewrite the use in the file's own idiom for supported languages/formats.
- **FR-009**: `encrypt` MUST delegate to SOPS with age on JSON/YAML/TOML/ENV values using only a recipient PUBLIC key from `.sops.yaml` or `SOPS_AGE_RECIPIENTS`; if `sops` or a recipient is missing, the action MUST be reported unavailable. No decrypt action for the agent. No custom crypto format.
- **FR-010**: `dismiss` MUST record a reviewed false positive in a committed file bound to the content digest of that line, avoiding the five rejected allowlist failure modes named in HANDOVER §3.
- **FR-011**: After a successful apply, the tool MUST honestly report: which files were written (observed), a fresh re-scan result for them (observed), and that the old value remains in git history since commit X (or an honest uncommitted/working-tree note).
- **FR-012**: Every caller-supplied path MUST go through `discovery::resolve_repo_path`. `refuse_by_policy` MUST stay syscall-free.
- **FR-013**: Any subprocess spawn for `sops` (or related helpers) MUST use `process_util::hidden_command` only.
- **FR-014**: Tests MUST use synthetic secrets only; RED-before-GREEN evidence is required for each new security behavior (Constitution II / HANDOVER §1).

### Key Entities

- **Withheld finding**: One detected secret instance addressable for remediation — id, rule, line range, shape, confidence, available actions.
- **Remediation preview**: Masked diff + planned file set for a proposed action; no durable side effects.
- **Remediation apply result**: Observed writes, re-scan summary, history note; success only when those observations succeeded.
- **Dismissal record**: Committed, content-digest-bound false-positive marker with size/symlink constraints.
- **Shape descriptor**: Human/machine description of a value's form (e.g. "40-char base62 after api_key =") carrying zero secret bytes.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a synthetic withheld file, 100% of admission refusals that carry content findings also carry `_meta["symforge/withheld"]` with shape-not-bytes; verified by acceptance tests that fail if meta is missing or if secret bytes leak into the serialized result.
- **SC-002**: Externalize preview never modifies the worktree; externalize apply leaves source clean on re-scan for the targeted finding and keeps `.env` gitignored — demonstrated RED then GREEN.
- **SC-003**: Encrypt is either fully unavailable (no `sops`/recipient) or fully applied via SOPS; zero partial plaintext left in the targeted value — demonstrated with positive and negative controls in the same tests.
- **SC-004**: Dismiss silences only the matching content digest; a mutated line on the same path is withheld again — counterexample fails the feature.
- **SC-005**: Zero emission of secret bytes or private keys across the acceptance suite (result text, `_meta`, tool errors, health).
- **SC-006**: Prompt-injection fixture in-repo cannot cause apply without going through the write-tool permission path (harness-level; documented test strategy in quickstart).

## Assumptions

- Deliverable 1 (quoted credential keys) is already merged on `main` (`#738`); detector matching work is out of scope for this feature except re-scan after remediation.
- Tool name `secret_remediate` is the working name from HANDOVER; final advertised name must satisfy `verify-tools.cjs` / tool-list pins and may be adjusted in implementation tasks without changing behavior.
- Supported externalize idioms for v1: JS/TS (`process.env.X`), Python (`os.environ["X"]`), Rust (`std::env::var("X")`), and `${X}` for common config formats; other languages may report externalize unavailable until extended.
- `.env` target is repository-root `.env` unless an existing project convention is discovered in-plan; multiple env files are out of scope for v1.
- Dismissal store path and schema are chosen in plan/research; must be committed, digest-bound, size-capped, no symlink escape, and invisible to health as unscanned text.
- Confidence levels reuse detector confidence if already present; otherwise a documented discrete enum (`high` / `medium` / `low`) derived from rule class.
- Whole-repo scope is allowed but subject to the same path resolution and credential-root refusals as other mutating tools; operators are expected to prefer file scope.
- Human approval IS the MCP client / harness write permission prompt for apply; SymForge does not invent a second in-band approval protocol in v1.
- No agent-facing decrypt tool will ship in this feature or as a silent follow-on in the same PR.
