# Feature Specification: Tool reliability, correctness floor

**Feature Branch**: `feat/036-tool-reliability`

**Created**: 2026-11-11

**Status**: Draft

**Input**: Owner, 2026-11-11, verbatim: "we need to fix this. It is costing tokens and time" and "we must prevent any possibilty that a tool can fail. this means no assumptions and 100% decarative results based on known patterns, known OS responses, environments". The owner also accepted that assumptions are allowed when a probe or precheck backs them.

## Background

On 2026-11-11, 28 tools were each given to an agent that had no context and was not told which tool to use. The agents picked the intended tool first in 22 cases. In 13 cases the tool's answer forced extra calls or a fall-back to the built-in file tools. The causes were read in the code and reproduced against the installed 11.11.0 build:

- A path-scoped reference query returned 6 of 8 references and still printed "Completeness: full".
- A dependents query answered "none" for a file that another file declares and uses, and listed about 20 unrelated files for a second file because their module names end the same way.
- Default output budgets cut the section the caller asked for: an explicit 75-line range, a named symbol body, a caller list whose header printed "Callees (5):" with no names, and a repository map that stopped alphabetically before reaching the source directory.
- Arguments that apply only in one mode were accepted and ignored in the others. Unknown arguments were dropped.
- Every answer carried "UNVERIFIED against disk" because ten stale unreadable records marked the whole index degraded, while a direct probe read all 1,760 files.

This spec covers the correctness floor. Later phases are listed under Out of Scope.

## User Scenarios & Testing *(mandatory)*

The user is a coding agent calling a tool, and through it the person paying for that agent's tokens and time.

### User Story 1 - Nothing that matches is silently dropped (Priority: P1)

An agent asks who uses a symbol or who depends on a file. Every textual occurrence of that name in the indexed project is accounted for in the answer: confirmed as a use, shown as probable, listed as unresolved, or counted as excluded with the reason. Narrowing the question never makes a real use disappear without saying so.

**Why this priority**: A missing caller leads to a wrong edit. This was the most damaging failure in the sweep, and reference search, dependents, rename, impact and edit planning all inherit it.

**Independent Test**: Run reference and dependents queries on fixtures whose true answer is known from an exact text search, and compare.

**Acceptance Scenarios**:

1. **Given** a function with 8 references in 3 files, **When** the agent asks for references scoped to the defining file, **Then** all 8 are confirmed, or each one not confirmed is listed as unresolved with its location, and no completeness claim is made that the listing contradicts.
2. **Given** a file declared by its parent with a module declaration and used through qualified paths, **When** the agent asks for its dependents, **Then** the parent file is listed.
3. **Given** two modules whose names end the same way, **When** the agent asks for dependents of one, **Then** files that import only the other are not listed.
4. **Given** a rename dry run, **When** call sites sit behind an explicit import of the target or call it in its own file with no local of that name in between, **Then** they are reported as sites the rename will change.
6. **Given** a rename, **When** a site is only probable (same spelling, no path or import tying it to the target), **Then** it is listed and not written.
5. **Given** a name that also occurs in comments and string literals, **When** the agent asks for references, **Then** those occurrences are counted as excluded and not shown as uses.

---

### User Story 2 - What the caller asked for is in the answer (Priority: P1)

An agent asks for a line range, a symbol body, a caller list or a repository overview. The requested content arrives whole in the first answer. If the answer has to be shortened, optional sections go first, the remainder is ordered by relevance, and the answer contains the exact next call that returns the rest.

**Why this priority**: Truncation was the most frequent cause of extra calls and of falling back to built-in reads.

**Independent Test**: For each tool, make its typical call with default arguments on a fixture and check that the primary content is complete.

**Acceptance Scenarios**:

1. **Given** an explicit request for 75 lines, **When** the default budget is smaller than those lines, **Then** all 75 lines are returned with line numbers.
2. **Given** a request for a symbol's context, **When** the output must be shortened, **Then** the callers and callees are named and optional sections are dropped before any name is.
3. **Given** a repository overview on a project with many documentation directories, **When** the default budget applies, **Then** source directories appear in the answer.
4. **Given** an answer that cannot fit, **When** it is shortened, **Then** it states how many items were left out and gives the literal call that returns them.
5. **Given** a range that reaches past the end of the file, **When** it is read, **Then** the answer states the last line of the file.

---

### User Story 3 - No argument is silently ignored (Priority: P2)

An agent passes an argument the tool does not know, or one that does not apply to the mode in use. The tool either honours it or refuses with a message that names the valid arguments. It never returns an answer computed as if the argument were absent.

**Why this priority**: An ignored argument produces a confident answer to a different question. It is cheaper to fix than P1 and affects fewer calls.

**Independent Test**: Call each tool once with an unknown argument and once with each mode-specific argument outside its mode.

**Acceptance Scenarios**:

1. **Given** an overview call with a path and a depth, **When** the detail level does not use them, **Then** the tool switches to the mode that does or refuses and names that mode.
2. **Given** an unknown argument, **When** the call is made, **Then** it is refused with the list of valid arguments.
3. **Given** an empty list for an optional argument, **When** the tool's description gives the empty value a meaning, **Then** that meaning applies; otherwise it is treated as absent. In neither case is an unrelated argument demanded.

---

### User Story 4 - Trust statements are about this answer (Priority: P2)

An agent reads an answer and needs to know whether it reflects the files on disk now. Each file the answer draws on is checked against the disk before the answer is given. A caveat appears only when one of those files could not be confirmed, and it names the file and the reason.

**Why this priority**: A warning on every answer carries no information and costs tokens on every call. A stale answer after an outside edit is a correctness failure.

**Independent Test**: Edit a file outside the tool, query it at once, and check the answer; separately, hold a stale unreadable record for one file and query another.

**Acceptance Scenarios**:

1. **Given** an index with stale unreadable records for files A, **When** the agent queries file B that is readable and unchanged, **Then** the answer carries no freshness caveat.
2. **Given** a file edited by another program a moment ago, **When** the agent searches it, **Then** the answer reflects the new content.
3. **Given** a file locked by another process, **When** the agent reads it, **Then** the tool retries within a bounded time and, if it still fails, names the file and the cause reported by the operating system.
4. **Given** a file changed within the same clock tick as its last indexing, **When** it is queried, **Then** it is compared by content and not trusted on timestamp alone.

---

### User Story 5 - Every answer states what kind of answer it is (Priority: P3)

An agent or a script reading the result can tell, from one field, which of six outcomes it got: a complete answer, a partial answer with a way to continue, a proven empty result with what was searched, an ambiguous request with the candidates and the call that selects one, an answer that cannot be determined with the reason and remedy, or an invalid request.

**Why this priority**: It makes the other four stories checkable by machine and removes guesswork from prose, but agents can already act on correct text.

**Independent Test**: Drive each tool into each outcome it can produce and read the outcome field.

**Acceptance Scenarios**:

1. **Given** a search with no match, **When** it returns, **Then** the outcome is the proven-empty kind and it states which files were searched and which were not.
2. **Given** a symbol name with several definitions, **When** it is requested without a path, **Then** the outcome is the ambiguous kind, the candidates are listed, and one literal call per candidate is given.
3. **Given** a multi-word symbol query with no exact match, **When** it returns, **Then** matches for all the words are returned and the answer says which matching rule produced them.

### Edge Cases

- A file is deleted between the freshness check and the read.
- A file on Windows is pending deletion while still open, which the operating system reports as access denied.
- An operating-system error code that is not in the known table: it is reported verbatim and recorded so the table can grow, and the outcome is "cannot determine", never "none".
- A name used only inside macro arguments.
- A name that is both a local variable in one file and an exported function in another.
- A re-export chain between the definition and the use.
- A project with files excluded by the admission policy: an empty result says how many in-scope files were not searched.
- Clients that already send arguments the tools ignore today: after this change those calls are refused.
- Inputs frozen by the version 11 public contract keep their accepted shape.

## Requirements *(mandatory)*

### Functional Requirements

**Request contract**

- **FR-001**: Every tool MUST refuse a call that carries an argument it does not define, and the refusal MUST name the arguments it accepts.
- **FR-002**: Every argument that applies only in certain modes MUST, when given outside them, either select the mode that uses it or cause a refusal that names that mode. No such argument may be ignored.
- **FR-003**: An empty list or empty string for an optional argument MUST be treated as absent, except where the tool's published description gives the empty value a meaning.
- **FR-004**: Each tool's arguments, their conditions, its primary content and its typical task MUST be declared in one place that both the request check and the tests read.

**Reference resolution**

- **FR-005**: Reference search, dependents, symbol context, rename, impact and edit planning MUST obtain their candidate sites from one shared resolver.
- **FR-006**: The resolver's candidates MUST include every exact occurrence of the identifier in the indexed files in scope.
- **FR-007**: The resolver MUST classify each candidate as confirmed (tied to the target by an import, a qualified path, a module declaration or an index record), probable (same spelling with supporting but not conclusive evidence), unresolved or excluded, and MUST record the rule that decided it.
- **FR-008**: Answers MUST show confirmed sites, show probable sites under their own label, list unresolved sites with their locations, and give the count of excluded sites by reason.
- **FR-008a**: A tool that writes MUST change only confirmed sites, and MUST list the probable and unresolved sites it left alone.
- **FR-009**: A module declaration MUST count as a dependency of the declaring file on the declared file.
- **FR-010**: Qualified paths, including those relative to the current module, the parent module and the crate root, MUST be normalised before they are compared with a target. For a language with no normalisation rule yet, path evidence MUST yield probable, never confirmed.
- **FR-011**: A file MUST NOT be reported as a dependent because part of its import path has the same final name as the target.
- **FR-012**: A scoped query MUST return a subset of the unscoped query, and every site the scope removes MUST be counted in the answer.
- **FR-013**: An answer MUST NOT state that it is complete unless the resolver left no unresolved candidate in scope and every in-scope source file was searched. Files withheld by the admission policy that are not source code do not block the statement; their count is shown.
- **FR-014**: Edit planning MUST accept a path so that a name with several definitions can be pinned to one.

**Disk verification**

- **FR-015**: Before answering, the tool MUST compare each file the answer uses with the disk by size and modification time, and MUST re-read and re-index a file that differs. An answer that claims an empty or complete result MUST first compare every file in its scope, or state how many it did not compare.
- **FR-016**: A file whose modification time is not older than its indexing time by more than the filesystem's timestamp resolution MUST be compared by content.
- **FR-017**: A freshness caveat MUST appear only when a file used by the answer could not be confirmed, and MUST name that file and the reason. Until file-event delivery is proven (next phase), an empty or complete claim made while the index reports lost observations MUST say that files the index has never seen may exist.
- **FR-018**: A stale or unreadable record for one file MUST NOT change the answer or the caveats of a query that does not use that file.
- **FR-019**: File access failures MUST be classified from the operating system's own error code through a table that states, for each code and platform, the class, whether it is transient, the follow-up check, and the remedy shown to the caller.
- **FR-020**: Transient failures MUST be retried within a time bound that applies to the whole call, not to each file.
- **FR-021**: On Windows, access denied on a file MUST be distinguished from deletion pending.
- **FR-022**: An error code absent from the table MUST be reported verbatim and recorded, and MUST yield the "cannot determine" outcome.
- **FR-023**: Health output MUST name each file it counts as unreadable and the classified cause.

**Output**

- **FR-024**: Content the caller requested explicitly (a line range, a named symbol, a named section, a caller list) MUST NOT be shortened by a default budget.
- **FR-025**: When an answer must be shortened, optional sections MUST be removed before primary content.
- **FR-026**: A shortened answer MUST order what remains by relevance to the request, state how much was left out, and include the literal call that returns the rest.
- **FR-027**: File content MUST be returned with line numbers, and a range past the end of the file MUST state the file's last line.
- **FR-028**: A repository overview MUST rank source directories ahead of documentation and specification directories, and MUST honour a path and depth.
- **FR-029**: An empty result MUST state what was searched, and MUST show matches found in documentation, test or policy-withheld files as a count with the call that reveals them.
- **FR-030**: A symbol query of several words with no exact match MUST return symbols matching all the words and say so.
- **FR-031**: A change summary MUST count paths the same way the version control status does.
- **FR-032**: An impact answer MUST rank affected items so that not all share one level.

**Outcome**

- **FR-033**: Every tool result MUST carry exactly one of six outcomes in its machine-readable metadata: complete, partial, proven empty, ambiguous, cannot determine, invalid request. An internal error is reported as cannot determine with that reason. The existing result-status metadata keeps its published values.
- **FR-034**: The partial outcome MUST carry the continuation; the ambiguous outcome MUST carry the candidates and one literal call per candidate; the cannot-determine outcome MUST carry the reason and remedy.

**Proof**

- **FR-035**: Text search and reference candidates MUST be compared with an independent exact text search on real repositories, and change summaries with the version control tool, in the automated suite.
- **FR-036**: The suite MUST check FR-012, that a rename's written sites are exactly the confirmed references of the same symbol, and that no probable or unresolved site is written.
- **FR-037**: The suite MUST inject each file access failure in the table, plus a file that vanishes between check and read and a modification-time tie, and check the outcome.
- **FR-038**: For each tool, the suite MUST check that the typical call with default arguments contains the primary content declared under FR-004.

### Key Entities

- **Tool contract**: for one tool, its arguments and the conditions under which each applies, its primary content, its optional sections in drop order, and its typical task.
- **Candidate site**: one occurrence of an identifier, with file, line, classification (confirmed, probable, unresolved, excluded) and the rule that decided it.
- **File stamp**: the size, modification time and content digest recorded when a file was indexed.
- **OS response entry**: platform, error code, class, transient or not, follow-up check, remedy text.
- **Outcome**: one of the six kinds, with its continuation, candidates or reason.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the 13 tasks that failed the 2026-11-11 sweep, an agent with no context reaches the correct answer in at most 2 tool calls, with no use of built-in file tools.
- **SC-002**: Across the comparison repositories, reference and text queries miss zero occurrences that an exact text search finds in the searched files.
- **SC-003**: Zero false "none" answers on the dependents fixtures, and zero dependents listed that do not use the target.
- **SC-004**: Zero calls in the suite where an argument was accepted and had no effect.
- **SC-005**: On an unchanged, readable project, zero answers carry a freshness caveat.
- **SC-006**: After an outside edit, the first query on that file reflects the edit in 100% of trials.
- **SC-007**: The typical call for every tool contains its primary content in full.
- **SC-008**: Median answer size for the 13 sweep tasks does not grow by more than 20% against the 11.11.0 build, measured in tokens for the calls that reach the answer.

## Assumptions

- Refusing unknown and inapplicable arguments changes behaviour for clients that send them today. The owner's standard ("no assumptions") decides this in favour of refusal; the release note states it.
- Argument shapes frozen by the version 11 public contract are kept; the refusal applies to what those shapes do not define.
- Timestamp resolution is taken from a conservative constant in this phase. Measuring it per filesystem belongs to the environment probes in the next phase.
- Until the watcher sync check of the next phase exists, the per-answer disk check covers files the index knows. Files the index has never seen are covered only by the caveat in FR-017.
- Every slice changes observable behaviour, so each pauses for the owner's approval before it merges.
- The resolver confirms by import path, qualified path, module declaration and index record. It does not type-check. A name that merely has one definition, or sits in the defining file, is probable. Sites it cannot decide stay "unresolved" and visible. Path rules are written for Rust first; other languages confirm through their indexed import records and are otherwise probable.
- Work is based on `chore/deps-2026-11`, which contains the unmerged audit repairs and the opt-in core profile that this feature edits.
- The 13 sweep tasks and their expected answers are fixed from the adoption sweep notes of 2026-11-11 and are re-run with the same method.

## Out of Scope

- Watcher sync token and environment probes.
- Merging overlapping tools and changing the default tool surface.
- Field telemetry of abandoned calls.
- A language-server or SCIP precise tier.
- The compact facade.
