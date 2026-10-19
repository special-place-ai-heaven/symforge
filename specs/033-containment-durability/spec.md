# Feature Specification: Containment and Durability Hardening

**Feature Branch**: `033-containment-durability`

**Created**: 2026-09-18

**Status**: Tasks generated — analysis remediations applied

**Input**: Close every kept finding in `docs/reviews/agent-review-2026-09-18.md` (F-001 … F-009). That review is the authority for *what* is broken; this spec is the authority for *scope, stories, and acceptance*. Rejected items R-001 … R-007 stay rejected.

**Precedent**: `specs/021-admission-coverage-honesty/` (defect ledger → independently testable stories). Feature 010 owns trust *evaluation*; this feature closes F-001…F-009 (containment, sidecar bind, hasher skip, durable persist, splice bounds, token ACL, init writers, lifecycle docs).

---

## Why this feature exists

Nine defects from one full-repo review share a shape: a check exists, a fallback or follow-symlink or in-place write undoes it, and a later surface reports success on bytes or state it did not confine or did not observe durable.

Operators and agents using MCP, sidecar HTTP, `symforge init`, `symforge trust`, `checkpoint_now`, and daemon token files must not:

- receive file contents from outside the opened workspace through a symlink or junction;
- start an unauthenticated sidecar on a routable bind by setting one environment variable;
- accept a project-config trust hash that walked a junction out of the reviewed tree;
- be told a checkpoint or editor config write succeeded when a crash can leave the previous durable file torn or absent;
- panic on a stale byte range instead of a structured refusal;
- store a daemon token under Windows default ACLs when Unix already decided owner-only is the barrier;
- read module docs that claim a live mutation surface is production-dark.

Rejected findings (token-length leak, unused `file_from_workdir`, unused cwd fallback, catalog walker, admin static Bearer, daemon spawn flags, hook marker fail-open) are **out of scope**. Do not “fix” them. Do not add Bearer auth to loopback sidecar without a separate hook-compat campaign.

---

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Outside-workspace bytes never enter retrieval (Priority: P1) — F-001

An operator opens a workspace that contains a file symlink to a secret outside the repository, or on Windows a directory junction (`mklink /J`) whose *directory* target is outside the repository so the secret is reached as `junc/secret.txt`. An agent asks for that in-repo path via `get_file_content`, searches untracked text for a unique string from the secret, or hits sidecar source-read after a freshen. The secret is not returned, is not inserted into the live index under the in-repo name, and is not indexed by watcher reindex of that path.

**Why this priority**: This is the only finding that is a guaranteed confidentiality failure given a malicious or careless tree. Other stories are footguns, crash durability, or panics. A cloned repo with one outbound link is enough.

**Independent Test**: Temporary repo whose only secret lives outside the root. Unix: in-repo file symlink to that file. Windows: `mklink /J` from an in-repo directory to the secret’s parent directory, then retrieve `junc/secret` — a junction cannot target a file. Retrieval, untracked search, sidecar source-read after freshen, and watcher/stable-read of those paths all refuse or skip. The live index must not contain the secret. A same-tree ordinary file remains readable (positive control).

**Acceptance Scenarios**:

1. **Given** an in-repo relative path that is a symlink to a file outside the repository, **When** `get_file_content` is asked for that relative path, **Then** the outside bytes are not returned and are not published into the live index under that relative name.
2. **Given** the same outbound link, **When** untracked `search_text` uses an anchored regex over a unique string that exists only in the outside file, **Then** that string is not a hit.
3. **Given** the same outbound link, **When** sidecar `/workflows/source-read` runs after a freshen of that path, **Then** the response does not contain the outside file’s contents and the index does not contain that path’s bytes.
4. **Given** a `safe_repo_path` failure (including “outside after following a link”), **When** a freshen helper resolves the path, **Then** it refuses and does not reindex; it does not join the relative path onto the canonical root as a fallback.
5. **Given** a disk read, stable-read, worktree-text, or watcher reindex of a path whose `symlink_metadata` is a symlink or reparse point, **When** that path is opened, **Then** the open does not follow the link and the lane skips or refuses.
6. **Given** an ordinary in-repo regular file with no outbound link, **When** the same retrieval, search, and sidecar lanes run, **Then** they still return that file’s contents (positive control).
7. **Given** an in-repo directory junction (or directory symlink) whose target is an outside directory containing `secret.txt`, **When** `get_file_content`, untracked `search_text`, or watcher reindex is asked for `junc/secret.txt`, **Then** the outside bytes are not returned and are not indexed. Leaf-only `symlink_metadata` on the nested file is not sufficient; the lane MUST also refuse when `safe_repo_path` / canonicalize places the path outside the workspace.

---

### User Story 2 — Sidecar does not bind a routable interface by accident (Priority: P2) — F-002

An operator sets the sidecar bind host to `0.0.0.0` or a LAN address. Without an explicit non-loopback opt-in, spawn fails. With opt-in, spawn may proceed only if a token is also required for that non-loopback listener, and a warning is recorded. Default bind remains loopback. Loopback sidecar stays hook-compatible: this story does **not** add Bearer to same-user loopback.

**Why this priority**: Default bind is already loopback. The defect is an operator footgun the daemon and `symforge serve` already close. Independent of path containment.

**Independent Test**: Spawn with a non-loopback bind and no opt-in fails. Spawn with opt-in plus the required token configuration warns and starts. Loopback spawn without a new token requirement still starts.

**Acceptance Scenarios**:

1. **Given** `SYMFORGE_SIDECAR_BIND` set to a non-loopback address and no explicit non-loopback opt-in, **When** the sidecar is spawned, **Then** spawn fails and no listener is left bound.
2. **Given** the same non-loopback bind **and** explicit opt-in **and** a required token for that listener, **When** spawn is attempted, **Then** it MUST start and MUST record a warning that the bind is off-loopback by operator choice.
3. **Given** the default or loopback bind, **When** spawn runs without a new Bearer requirement, **Then** the sidecar starts (hook-compat preserved).
4. **Given** non-loopback opt-in without a token, **When** spawn is attempted, **Then** it refuses to start.

---

### User Story 3 — Trust hash does not walk a junction out of the reviewed tree (Priority: P2) — F-003

An operator runs `symforge trust project-config accept` on a project whose `.symforge/config` contains a Windows junction (or a symlink) pointing outside the project. The hasher does not include the outside tree. Evaluation is fail-closed (Untrusted, with a warning) rather than recording a hash of the wrong predicate.

**Why this priority**: A wrong trust record is the wrong predicate for later edit-safety checks. Independent of sidecar bind. Shares the trust store file with User Story 7; implementers must not collide.

**Independent Test**: Windows-only junction under `.symforge/config` pointing at an outside tree; hash/evaluation does not include that tree (Untrusted + warning). Unix: symlink-to-dir under config is skipped/refused the same way. Positive control: a normal config file in that directory is hashed.

**Acceptance Scenarios**:

1. **Given** a junction or symlink under the project-config directory pointing outside the project, **When** the config hasher walks that directory, **Then** it does not recurse into or hash the outside target tree.
2. **Given** that walk, **When** trust is evaluated, **Then** the result is Untrusted (or equivalent fail-closed) and a warning names the skipped unsafe path.
3. **Given** only regular files and directories with no reparse/symlink, **When** the hasher runs, **Then** those files are included (positive control).

---

### User Story 4 — A reported checkpoint is actually on disk (Priority: P2) — F-004

An operator asks for an explicit snapshot persist (`checkpoint_now` with verify-after-write). The process reports success only after snapshot bytes (and team-artifact bytes/metadata when that path is used) have been written and flushed such that a crash after the success return cannot resurrect a torn or missing durable file from a tmp that never reached stable storage. Existing verify-after-write round-trips still succeed.

**Why this priority**: Reporting “checkpoint succeeded” without a durability boundary violates the reporting invariant. Independent of containment.

**Independent Test**: The persist helper sequence is tmp write → file sync → rename (and parent-directory sync on Unix). Existing checkpoint round-trip tests still pass. A helper-level test or file-wrapper spy observes `sync_all` before rename.

**Acceptance Scenarios**:

1. **Given** a snapshot persist, **When** success is reported, **Then** the durable snapshot file is the result of a tmp write that was synced before rename.
2. **Given** a team-artifact export, **When** success is reported, **Then** both the compressed artifact and its metadata file used the same durable sequence (no in-place metadata write without sync).
3. **Given** `checkpoint_now(verify_after_write=true)`, **When** the existing round-trip tests run, **Then** they still pass.

---

### User Story 5 — Stale spans refuse; they do not panic (Priority: P2) — F-005

An agent issues a structural edit or a symbol diff whose byte range is inverted, past the buffer, or not a character boundary on text. The tool returns a structured error. The process does not panic. Valid in-range splices and diffs still work.

**Why this priority**: A panic is a user-visible crash of the MCP server, not a tool error. Independent of persist.

**Independent Test**: Unit tests for `end > len`, `end < start`, and a non-char-boundary `start` on a multibyte string — all `Err`, no panic. A valid ASCII range still splices (positive control).

**Acceptance Scenarios**:

1. **Given** a splice or edit whose `start > end` or `end > len`, **When** the splice is applied, **Then** the caller receives `Err` and the process does not panic.
2. **Given** a text slice whose start or end is not a character boundary, **When** symbol-diff extraction indexes that range, **Then** it returns `Err` or skips that symbol without panicking.
3. **Given** a valid in-range range on matching bytes, **When** splice or diff runs, **Then** the edit or hash proceeds (positive control).

---

### User Story 6 — Windows daemon token is owner-only (Priority: P2) — F-006

A Windows operator starts the daemon. The token file’s DACL does not grant read to Everyone or Users. Unix remains `0o600` at create. The token’s confidentiality barrier matches the comment that already pins it on owner-only.

**Why this priority**: Unix already treated file mode as the barrier. Windows `fs::write` inherits the directory ACL; on a shared ACL that is not owner-only. Independent of sidecar bind.

**Independent Test**: After writing the token file on Windows, the DACL has no Everyone/Users read ACE. cfg-gated test on Windows CI; Unix test still asserts 0o600.

**Acceptance Scenarios**:

1. **Given** a freshly written daemon token file on Windows, **When** its DACL is inspected, **Then** it has no Everyone or Users read ACE.
2. **Given** the same write on Unix, **When** mode is inspected, **Then** it is `0o600` (positive control; no regression).

---

### User Story 7 — Trust-store accept/revoke cannot tear the store (Priority: P2) — F-007

An operator accepts or revokes trust. A crash mid-write leaves the previous `trust.json` intact and parseable, or the file absent (fail-closed Untrusted). It never leaves a half-written JSON that is neither the old store nor the new one *and* unparseable for a different reason than absence. Load stays fail-closed on corrupt.

**Why this priority**: Crash during accept currently overwrites in place; torn JSON stops all structural edits until re-accept. Independent of hasher containment except shared files.

**Independent Test**: Persist uses tmp + sync + rename. A crash-mid-write simulation (failing spy or leftover tmp) leaves the previous file intact.

**Acceptance Scenarios**:

1. **Given** an existing valid trust store, **When** a persist is interrupted before rename, **Then** the previous file is still valid JSON.
2. **Given** a completed persist, **When** the store is loaded, **Then** it contains the new records.
3. **Given** a corrupt file on disk, **When** trust is loaded, **Then** evaluation remains fail-closed Untrusted (no silent trust).

---

### User Story 8 — `symforge init` cannot tear editor MCP JSON (Priority: P2) — F-008

An operator runs `symforge init` against Claude, Codex, Cursor, or other client config files. Killing the process mid-write leaves the previous config parseable. The same atomic persist already used for harness/operator-profile files is reused. Test fixtures that `fs::write` into temp dirs are unchanged.

**Why this priority**: Torn editor settings take down the user’s editor, not just SymForge. Independent of trust.json.

**Independent Test**: Production client-config writers use atomic persist. Crash-mid-write leaves the previous file. Existing init tests still pass.

**Acceptance Scenarios**:

1. **Given** an existing parseable client config, **When** init is interrupted before the durable replace, **Then** the previous file is still parseable by that editor.
2. **Given** a completed init write, **When** the file is read, **Then** it contains the merged SymForge configuration.
3. **Given** existing harness/operator-profile atomic writers, **When** init is changed, **Then** it reuses that helper rather than inventing a third persist sequence.

---

### User Story 9 — Lifecycle docs name live call edges (Priority: P3) — F-009

A reviewer reading `src/index_lifecycle` module docs is told the truth: production already calls `project_source_authority` and confined writes. Darkness language remains only on submodules that still have no production call edges. The T051 darkness *test* still matches whatever it currently requires; this story does not reopen the darkness campaign.

**Why this priority**: A false “nothing in production calls this” skip is the reporting-invariant failure this directory exists to prevent. Docs-only; no behavior change.

**Independent Test**: Parent module docs no longer claim production has no callers. Submodules that remain uncalled may keep darkness language. T051 still passes.

**Acceptance Scenarios**:

1. **Given** the `index_lifecycle` crate-module docs, **When** a reviewer reads them, **Then** they name the live `project_source_authority` / confined-write call edges (or equivalently stop claiming production darkness for that module).
2. **Given** submodules that still have no production callers, **When** their docs are read, **Then** darkness language may remain.
3. **Given** the T051 darkness suite, **When** it runs after the doc edit, **Then** it still passes.

---

### Edge Cases

- Symlink to a *file* inside the same repository: in-scope for User Story 1 — still a follow; refuse/skip rather than reading through the link. Ordinary regular files stay the positive control.
- Dangling symlink: refuse/skip as not a regular non-link file; do not fallback-join.
- Junction to a directory used as a nested project path: User Story 1 refuses follow on read/reindex; User Story 3 refuses hasher recursion.
- `safe_repo_path` error because the path does not exist yet: freshen must not invent an outside path via join-fallback; treating a missing in-repo leaf as “no-op freshen” is allowed.
- Sidecar bind host that is a hostname rather than an IP: same loopback predicate as the daemon (loopback names vs routable).
- Non-loopback sidecar with opt-in but empty token: refuse (Story 2 scenario 4).
- Unix symlink in `.symforge/config`: hasher skips like a reparse point (Story 3).
- Snapshot persist when the tmp write succeeds and `sync_all` fails: report failure, do not rename, do not claim checkpoint success.
- Splice `start == end` (pure insert) and `start == 0` / `end == len`: valid; must succeed.
- Windows token file replacing an existing file: DACL is owner-only after the write, not only on first create.
- Init writing a *new* client config that did not exist: atomic create is enough; no previous file to preserve.
- Do not enable Bearer on loopback sidecar.
- Do not wire or “fix” `select_runtime_data_base`.
- Do not add Bearer to admin static assets (R-005).

---

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A path-containment failure (including a path that canonicalizes outside the workspace after following a link) MUST refuse freshen and MUST NOT reindex under the in-repo relative name.
- **FR-002**: Disk content admission, stable reads, worktree-text admission, and watcher reindex MUST open or stat the named path without following a symlink or Windows reparse point; a link is skip/refuse, not a regular in-repo file. Nested paths under a directory junction/symlink MUST also refuse when canonicalize / `safe_repo_path` places them outside the workspace. A leaf-only `symlink_metadata` check is not sufficient for `junc/secret.txt`.
- **FR-003**: `get_file_content`, untracked `search_text`, and sidecar source-read after freshen MUST NOT disclose outside-workspace bytes reached only through an in-repo link.
- **FR-004**: Sidecar spawn MUST refuse a non-loopback bind unless an explicit operator opt-in is set; with opt-in it MUST warn; without a token it MUST NOT start on that non-loopback bind.
- **FR-005**: Loopback sidecar MUST remain startable without a new Bearer requirement (hook compatibility).
- **FR-006**: Project-config trust hashing MUST NOT recurse into or hash through a symlink or reparse point; evaluation MUST fail closed with a warning when such an entry is present.
- **FR-007**: Snapshot persist and team-artifact persist MUST sync file data before rename (and sync the parent directory on Unix); success MUST NOT be reported if that sequence fails.
- **FR-008**: Byte-range splice and symbol-diff body extraction MUST return a structured error for inverted, out-of-bounds, or non-character-boundary ranges and MUST NOT panic.
- **FR-009**: Daemon token persist on Windows MUST result in a DACL with no Everyone/Users read ACE; Unix MUST remain owner-only `0o600`.
- **FR-010**: Trust-store persist MUST be atomic (tmp + sync + rename) for both library and CLI writers.
- **FR-011**: Production `symforge init` client-config writers MUST use the existing atomic persist helper; test-only fixture writes may stay in-place.
- **FR-012**: `index_lifecycle` parent-module docs MUST name live production call edges or stop claiming production darkness; T051 MUST remain the executing darkness authority.
- **FR-013**: Every negative acceptance scenario MUST ship with a positive control in the same test (or an adjacent test in the same module) so a total-refusal implementation cannot satisfy the story.
- **FR-014**: Each story’s tests MUST be observed failing against the defective code before the machinery lands (RED-first).

### Key Entities

- **Outbound link**: An in-workspace directory entry whose metadata is a symlink or Windows reparse point and whose target is not opened/followed for content, hashing, or reindex.
- **Freshen refusal**: A no-op or error from a freshen helper that does not publish bytes into the live index.
- **Loopback bind**: A sidecar/daemon listen address the existing daemon loopback predicate already accepts.
- **Non-loopback opt-in**: An explicit operator flag, parallel to the daemon’s, required before a routable sidecar bind.
- **Durable persist sequence**: Write to a sibling temporary file, `sync_all` the file, rename into place, and on Unix `sync_all` the parent directory.
- **Trust store**: The on-disk JSON that records accepted project-config hashes; load is fail-closed.
- **Daemon token file**: The OS-tagged file whose confidentiality is owner-only access.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In a workspace whose only unique secret is outside the root and reached by one in-repo file symlink, and on Windows by one directory junction to the secret’s parent plus the nested relative `junc/secret`, three retrieval surfaces (`get_file_content`, untracked `search_text`, sidecar source-read after freshen) return zero disclosures of that secret and the index does not contain it, while an ordinary in-repo file on the same run is still returned.
- **SC-002**: Sidecar spawn with a non-loopback bind and no opt-in fails 100% of the time in the story’s unit tests; loopback spawn without a new token requirement succeeds 100% of the time in those tests.
- **SC-003**: Trust hashing of a config tree that contains one outbound junction/symlink never includes the outside tree’s files in the hash input; evaluation is Untrusted.
- **SC-004**: After a reported-successful snapshot or artifact persist, the durable file is the renamed, synced object; a simulated `sync_all` failure prevents a success report.
- **SC-005**: Three illegal splice/diff ranges (`end < start`, `end > len`, non-char boundary) each produce a structured error and zero panics; one legal range succeeds.
- **SC-006**: On Windows, a freshly written daemon token file has zero Everyone/Users read ACEs; on Unix, mode remains `0o600`.
- **SC-007**: Interrupted trust-store and init client-config persists leave the previous parseable file in 100% of the story’s crash-simulation tests.
- **SC-008**: A reviewer can find the live `project_source_authority` call edges named in `index_lifecycle` parent docs; T051 still passes.

## Assumptions

- The 2026-09-18 review’s kept findings F-001 … F-009 are in scope; R-001 … R-007 are not.
- Default sidecar bind remains `127.0.0.1`; this feature does not change the default.
- Loopback sidecar Bearer is out of scope (hook compatibility). Admin static assets stay ungated (R-005).
- `select_runtime_data_base` stays untouched.
- Catalog bulk walk already skips non-files and canonicalize-outside entries (R-004); this feature does not change the catalog walker except where User Story 1’s shared read helpers are used.
- Existing `atomic_write` / `write_record_atomic` / confined-lease writers are the persist pattern to reuse, not a third invention.
- `paths::metadata_is_reparse_point` and `state_directory_metadata_is_unsafe` are the Windows junction predicates to reuse.
- Daemon `resolve_daemon_bind_host` / `is_loopback_bind_host` is the loopback predicate to reuse or copy; sidecar may use a sibling env name rather than the daemon’s opt-in env.
- Feature 020 T051 remains the executing darkness authority; this feature only corrects docs that contradict live call edges already allowed by later activation work.
- Constitution I (reporting invariant), II (RED-first), IV (verification gates), and VI (independent review) apply to every story.
- Worktrees isolate implementers; stories that share a file (F-003 and F-007 both touch `src/edit_safety/trust.rs`) MUST be sequential in that worktree, not parallel writers.

## Out of Scope

- Bearer on loopback sidecar or admin static routes.
- Changing catalog walk policy except via shared no-follow read helpers.
- Repairing unused `select_runtime_data_base` / `file_from_workdir` production wiring.
- Reopening Feature 020 darkness activation or changing T051’s sealed source set except as required to keep the test honest after a doc-only edit.
- Performance work unrelated to the nine findings.
- Batching User Story 1 API changes together with User Story 2 bind-policy changes in one commit (review constraint: independently implementable; do not mix).
