# Feature Specification: Intelligence Pattern Ports

**Feature Branch**: `007-intelligence-pattern-ports`

**Created**: 2026-06-16

**Status**: Draft

**Input**: User description: "Feature 007 — Intelligence Pattern Ports (v8 8.1.x). Port selective code-intelligence UX patterns from competitive research (SoulForge) onto SymForge's existing LiveIndex + STEL stack. SymForge remains an MCP code-intelligence server, not a terminal coding agent."

## User Scenarios & Testing *(mandatory)*

The "users" of SymForge are AI coding agents (and the operators driving them)
attached to the MCP server. Each story is a slice of agent experience that can
ship and be demonstrated on its own.

### User Story 1 - Agent sees blast radius after an edit (Priority: P1)

An agent performs a successful structural mutation (rename a symbol, replace a
symbol body, edit within a symbol, a batch edit, or an apply through the unified
edit tool). Without making a second tool call, the agent sees a compact impact
line in the edit response telling it how many files depend on what it just
changed, and — when temporal data is available — which files historically change
alongside the edited file.

**Why this priority**: This is the highest-value, lowest-cost port. Agents
routinely edit a symbol and then have to spend another tool call to learn what
they might have broken. Inlining the blast radius on the success path removes a
round trip and directly reduces "edit-then-break-something-unseen" mistakes.

**Independent Test**: Apply a structural edit to a symbol that has known
dependents in a fixture repo; assert the edit response contains an impact suffix
reporting the correct dependent count (and co-change partners when git history
exists). Apply an edit to a symbol with zero dependents and assert the footer is
omitted or reports zero per the contract.

**Acceptance Scenarios**:

1. **Given** a fixture repo where symbol `foo` is referenced by 3 other files,
   **When** the agent replaces `foo`'s body via a structural edit tool, **Then**
   the edit response includes an impact suffix reporting 3 dependents.
2. **Given** the same repo with git history showing `a.rs` and `b.rs` co-change,
   **When** the agent edits a symbol in `a.rs`, **Then** the impact suffix lists
   `b.rs` among co-change partners.
3. **Given** a symbol with no dependents and no co-change history, **When** the
   agent edits it, **Then** the response is per contract (footer omitted or an
   explicit zero) and the trust envelope is unchanged.
4. **Given** a failed/rejected edit, **When** the agent submits it, **Then** no
   impact footer is appended (footer is success-only).

---

### User Story 2 - Agent orients before over-reading (Priority: P1)

An agent onboarding to a repository reads the orientation prompts and the repo
map resource. Those surfaces explicitly teach the doctrine: the map orients, the
tools prove; a file's absence from the map does NOT mean it is absent from the
repo; rankings and truncations are disclosed. The agent therefore knows to use
search/find tools to confirm presence rather than concluding "not in the map →
not in the repo".

**Why this priority**: Cheap to ship (prompt + resource text), and it prevents a
systemic agent failure mode — treating a deliberately ranked/truncated map as an
exhaustive index and then making false-negative decisions.

**Independent Test**: Snapshot the onboarding/architecture prompt content and the
repo map resource content; assert both contain the orientation doctrine
statements (map-orients-tools-prove; absence-from-map != absence-from-repo;
truncation disclosed).

**Acceptance Scenarios**:

1. **Given** the MCP prompt surface, **When** an agent requests the
   onboarding/architecture prompt, **Then** the returned text states that the map
   orients and tools prove, and that absence from the map is not absence from the
   repo.
2. **Given** the repo map resource, **When** an agent reads it, **Then** the
   resource discloses that the listing is ranked/truncated and points to the
   tools that prove presence.

---

### User Story 3 - Compact map shows what matters first (Priority: P2)

An agent requests the compact repo map. Instead of a purely alphabetical file
listing, the agent sees the highest-leverage files first — those with the most
dependents and the most churn — within the same token budget. Files with a
meaningful number of dependents are annotated so the agent can see fan-in at a
glance.

**Why this priority**: Improves the signal of the most-used orientation surface
without enlarging it. P2 because it is more involved than the P1 text/footer
ports and depends on ranking inputs.

**Independent Test**: Build a fixture repo where one file has many dependents and
high churn and another has none; request the compact map; assert the high-fan-in
file ranks above the low-fan-in file and is annotated with its dependent count,
while the `full` and `tree` map modes remain unchanged.

**Acceptance Scenarios**:

1. **Given** a repo where `core.rs` has 8 dependents and `leaf.rs` has 0, **When**
   the agent requests the compact map, **Then** `core.rs` appears before `leaf.rs`.
2. **Given** a file with N>=2 dependents, **When** it appears in the compact map,
   **Then** it is annotated with its dependent count per the display contract.
3. **Given** the `full` and `tree` map modes, **When** requested, **Then** their
   ordering is unchanged by this feature.
4. **Given** any compact-map request, **When** ranking is computed, **Then**
   frecency signals are not mutated as a side effect.

---

### User Story 4 - Compact find answers fuzzy concepts (Priority: P2)

An agent issues a multi-word, fuzzy query through the unified find intent (for
example "stel planner find"). The find intent ranks results across both symbol
names and file paths, boosting co-change neighbors, and returns a compact ranked
list — without the agent needing to know whether the target is a symbol or a
file, and without a new public tool being added.

**Why this priority**: Consolidates a capability today split across separate
search/explore paths into one fuzzy find, improving agent ergonomics. P2 because
it touches the STEL planner and query fusion, which is more involved than the P1
ports.

**Independent Test**: Issue a multi-word query through the find intent against a
fixture; assert results are ranked across symbols and paths with co-change boost,
and assert that issuing the query does not bump frecency.

**Acceptance Scenarios**:

1. **Given** a multi-word query matching both a symbol and a path, **When** the
   agent uses the find intent, **Then** results include both, ranked by relevance
   with co-change neighbors boosted.
2. **Given** the find intent, **When** it executes, **Then** no new public MCP
   tool is introduced (the capability lives inside the existing find intent).
3. **Given** repeated find queries, **When** they run, **Then** frecency is not
   bumped by the find/discovery operation.

---

### User Story 5 - Impact intent gives one-envelope blast radius (Priority: P2)

An agent asks for impact through the unified intent and receives dependents and
co-change partners chained into one response envelope, rather than having to
combine multiple tool calls. When temporal data exists, the edit-planning surface
also mentions co-change partners so the agent can plan a safe edit.

**Why this priority**: Rounds out the impact story started in Story 1 for the
explicit "show me the blast radius" request. P2 because it builds on the same
inputs as the other P2 work.

**Independent Test**: Invoke the impact intent on a symbol with both dependents
and co-change history; assert one envelope contains both. Invoke edit-planning on
the same symbol; assert it mentions co-change when temporal data is present.

**Acceptance Scenarios**:

1. **Given** a symbol with dependents and co-change history, **When** the agent
   uses the impact intent, **Then** one response envelope reports both dependents
   and co-change partners.
2. **Given** temporal data exists for a symbol's file, **When** the agent runs
   edit-planning, **Then** the plan output mentions co-change partners.
3. **Given** no temporal data, **When** edit-planning runs, **Then** it omits the
   co-change line gracefully (no error, no empty/garbage line).

---

### Edge Cases

- **No git history**: Co-change data is unavailable. Impact footer and impact
  intent MUST degrade to dependents-only; orientation/map MUST still function.
- **Zero dependents and zero co-changes**: Footer behavior follows the contract
  (omitted, or an explicit zero), never a misleading or malformed suffix.
- **Failed/rejected edit**: No footer is appended; footers are success-only.
- **Truncated map**: The compact map MUST disclose that it is ranked/truncated;
  high-fan-in files surface first but the listing must not imply completeness.
- **Ranking tie**: When two files have equal rank inputs, ordering MUST be
  deterministic (stable tie-break), not arbitrary across runs.
- **Frecency side effects**: No discovery/search/find/map/impact path may bump
  frecency.
- **Embed build**: New ranking/find/footer code MUST NOT introduce server or
  network dependencies into the embed build.
- **Transport**: Behavior on stdio and on the serve `/mcp` transport MUST be
  equivalent for the same repo state.

## Clarifications

### Session 2026-06-16

The following decisions were pre-resolved with the operator-authorized
recommended defaults from the agent brief (§6). They are recorded here and
applied to the requirements below.

- Q: Should the impact footer apply to all structural tools or only the unified
  edit tool? → A: All successful structural mutations (symbol-body replacement,
  edit-within-symbol, batch edits, and the unified edit tool's apply path).
- Q: Footer format — prose or machine-parseable tag? → A: A plain machine-parseable
  suffix `[impact: N dependents · cochanges: a, b, c]`; the existing trust
  envelope is unchanged.
- Q: Compact map ranking — add a new `detail=ranked` mode or change the existing
  `compact` ordering? → A: Change the `compact` ordering only; preserve `full` and
  `tree` exactly.
- Q: Find fusion — add a new tool or keep it inside the existing find intent?
  → A: Inside the existing STEL `find` intent only; no new (4th compact) public
  tool.
- Q: Ship before or after the 004 operator-serve spine merges? → A: After 004
  lands, unless the operator waives; no serve/auth/ledger work in 007. Running
  this chain through implement is the operator's waiver to build the stdio-side
  patterns now without touching 004 scope.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: On a successful structural mutation, the system MUST append a
  compact impact suffix to the edit response reporting the count of files that
  depend on the edited symbol/file.
- **FR-002**: When git temporal data is available, the impact suffix MUST include
  the top co-change partner files for the edited file.
- **FR-003**: The impact suffix MUST be applied across all successful structural
  mutation paths (symbol-body replacement, edit-within-symbol, batch edits, and
  the unified edit tool's apply path), not only one of them.
- **FR-004**: The impact suffix MUST be omitted on failed or rejected edits, and
  MUST follow the single documented grammar `[impact: N dependents · cochanges:
  a, b, c]` (the `cochanges:` clause appears only when temporal data yields
  partners); the existing machine-readable trust envelope MUST remain unchanged.
- **FR-005**: The MCP onboarding/architecture prompts MUST contain the orientation
  doctrine: the map orients and tools prove; absence from the map is not absence
  from the repo.
- **FR-006**: The repo map resource MUST disclose that its listing is
  ranked/truncated and direct the agent to the tools that prove presence.
- **FR-007**: The compact repo map MUST order files by an importance ranking that
  combines dependent count and churn, replacing pure alphabetical ordering.
- **FR-008**: In the compact repo map, a file with a meaningful number of
  dependents (N >= 2) MUST be annotated with its dependent count using the
  display form `path (→N)` per the display contract.
- **FR-009**: The `full` and `tree` repo map modes MUST remain unchanged in
  ordering and content by this feature.
- **FR-010**: The unified find intent MUST support multi-word fuzzy queries ranked
  across both symbol names and file paths, with co-change neighbor boosting.
- **FR-011**: The find fusion MUST NOT introduce a new public MCP tool; it lives
  inside the existing find intent.
- **FR-012**: The impact intent MUST return dependents and co-change partners in a
  single response envelope.
- **FR-013**: The edit-planning surface MUST mention co-change partners when
  temporal data exists, and MUST omit that line gracefully when it does not.
- **FR-014**: No discovery, search, find, map, or impact operation introduced or
  modified by this feature may mutate (bump) frecency signals.
- **FR-015**: The system MUST NOT introduce a second authoritative/persistent
  index (no SQLite "Soul Map" or parallel graph store) to satisfy any requirement
  in this feature.
- **FR-016**: All new code paths MUST preserve embed-build isolation (the embed
  feature compiles without server/network dependencies) and transport parity
  (stdio and serve return equivalent results for the same repo state).
- **FR-017**: Ranking and fusion MUST be deterministic: identical repo state and
  inputs MUST produce identical ordering, with stable tie-breaks.

### Key Entities *(include if feature involves data)*

- **Impact Footer**: A compact, machine-friendly suffix on an edit response.
  Attributes: dependent count, top co-change partner file names, presence/absence
  rule, success-only. Derived from the reverse-import index and git temporal data.
- **Ranked Map Entry**: A file line in the compact repo map. Attributes: path,
  importance score (from dependents + churn), dependent-count annotation when
  N >= 2. Ordering is by importance, not alphabetical.
- **Find Result**: A ranked entry from the find intent. Attributes: matched
  symbol or path, relevance score, co-change boost. No new tool; an output of the
  existing find intent.
- **Orientation Doctrine Text**: The canonical wording embedded in prompts and the
  map resource expressing map-orients-tools-prove, absence != absence, and
  truncation disclosure.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For 100% of successful structural mutations on symbols with known
  dependents, the agent learns the dependent count from the edit response with
  zero additional tool calls.
- **SC-002**: The impact footer reports the dependent count with 100% agreement
  with the dedicated impact/dependents query for the same symbol and repo state.
- **SC-003**: 100% of onboarding/architecture orientation surfaces (prompts and
  the map resource) state the orientation doctrine (map-orients-tools-prove;
  absence != absence; truncation disclosed).
- **SC-004**: In the compact map, the highest-fan-in file ranks above any
  zero-fan-in file in 100% of fixture cases, while `full` and `tree` output is
  byte-unchanged from before the feature.
- **SC-005**: A multi-word fuzzy find query returns the intended target (symbol or
  path) within the top results, ranked across both symbols and paths, with no new
  public tool added.
- **SC-006**: Across all new/modified discovery, find, map, and impact paths,
  frecency is bumped zero times (verified by test).
- **SC-007**: The full verification gate passes: `cargo fmt --check`,
  `cargo check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --all-targets -- --test-threads=1`, `cargo build --release`; and the
  embed build (`cargo check --no-default-features --features embed`) stays green.

## Assumptions

- The "users" are AI coding agents and their operators attached over MCP; there is
  no human end-user UI in scope.
- This feature depends on the existing 004 operator-serve spine for the transport
  parity target, but does NOT implement or duplicate 004 serve/auth/ledger work.
  Per brief §6 Q5 the default is to ship after 004 lands; the operator running this
  chain through implement constitutes a waiver to build the stdio-side patterns
  now, provided no 004 serve/auth/ledger scope is touched (documented in research).
- The reverse-import/dependent index, git temporal index (`git_temporal`), and
  existing sidecar post-edit impact logic already exist and are reused, not forked.
- "Co-change" data may be absent (no git history); all co-change-dependent output
  degrades gracefully to dependents-only.
- The frecency invariant (discovery must not bump frecency) is pre-existing and
  binding; this feature must preserve it.
- Out of scope (reject list): SQLite Soul Map / parallel persistent index;
  grep/glob interception; request_tools/release_tools lazy schema loading;
  terminal-agent features (TUI, sessions, memory, task router, Neovim, providers);
  LLM-generated symbol summaries in the index; MinHash clone detection (8.2+); a
  hard 10k file cap (keep fail-closed discovery; recency subset is 8.2+ only).
