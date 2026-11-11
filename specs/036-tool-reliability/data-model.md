# Data Model: Tool reliability, correctness floor

All entities are in-memory. Nothing is persisted and no snapshot format changes.

## ToolContract (static, one per tool)

| Field | Meaning |
|---|---|
| `tool` | tool name |
| `conditions` | list of `(argument, applies_when: argument = value, on_mismatch: SelectMode or Refuse)` |
| `primary` | the content the tool exists to return (range, body, site list, tree) |
| `optional_sections` | named sections in the order they are dropped under pressure |
| `explicit_arguments` | arguments whose presence marks content as explicitly requested |
| `typical_call` | argument set used by the payload test |

The set of valid argument names is not stored here; it is read from the tool's published input schema.

Rules: every tool in the registered tool list has exactly one contract (a test enumerates both). A condition may name only arguments present in the schema.

## CandidateSite

| Field | Meaning |
|---|---|
| `path`, `line`, `byte_range` | where the identifier occurs |
| `text` | the source line |
| `class` | `Definition`, `Confirmed`, `Probable`, `Unresolved`, `Excluded` |
| `rule` | confirmed: `ImportPath`, `QualifiedPath`, `ModuleDeclaration`, `IndexedReference`, `SameFileUnshadowed`; probable: `SameFile`, `UniqueName`, `UnnormalisedPath`, `MemberAccess` (the name follows a receiver, so it may be a method or field of another type); excluded: `InComment`, `InString`, `OtherDefinition` (the occurrence is itself the definition of a different symbol); unresolved: `NoRule` |
| `kind` | the `ReferenceKind` of the joined index record, when one exists |
| `enclosing_symbol` | name of the enclosing symbol, when known |
| `is_test` | whether the file or enclosing item is test code |

## ResolvedSet

| Field | Meaning |
|---|---|
| `target` | identifier plus optional defining path |
| `scope` | paths searched |
| `sites` | every `CandidateSite` in scope, including the target's own definition (class `Definition`), so the set equals an exact text search |
| `out_of_scope` | count of sites the scope removed, by class |
| `unsearched` | count of in-scope files not searched, by reason (policy-withheld, unreadable) |

Invariants: `sites` contains every exact occurrence the text engine reports in `scope`. For scopes A ⊆ B, `sites(A) ⊆ sites(B)` and `out_of_scope(A) = |sites(B)| − |sites(A)|`. "Complete" may be claimed only when no site is `Unresolved` and no in-scope source file is unsearched.

Derived views: references = sites of a symbol; dependents of file F = files holding a confirmed site of any symbol F exports, or a `ModuleDeclaration` of F; rename writes confirmed sites only and lists the rest; impact and edit plan read the same set.

The class and its rule are one enum (`Confirmed(ConfirmRule)`, `Probable(ProbableRule)`, ...), so a write path that takes `Confirmed` cannot be handed a probable site.

## FileCheck (per file used by an answer)

`Unchanged` (size and mtime equal, outside the racy window) | `UnchangedByContent` (inside the window, or no in-memory `indexed_at` because the file came from a restored snapshot, and the digest is equal) | `Refreshed` (differed, re-read and re-indexed before answering) | `Failed(OsResponse)`.

`indexed_at` is held in memory beside the indexed file and is not written to snapshots.

An answer's caveats are the `Failed` entries of its own files, plus, on empty and complete claims only, the lost-observation note of FR-017.

## OsResponse (static table row)

| Field | Meaning |
|---|---|
| `platform` | windows or unix |
| `code` | raw OS error code |
| `class` | NotFound, Denied, DeletePending, InUse, PathTooLong, Corrupt, ResourceExhausted, Unknown |
| `transient` | whether to retry within the bound |
| `probe` | follow-up check that separates look-alike causes |
| `remedy` | one sentence shown to the caller |

## Outcome

A new type, separate from the public `OutcomeClass`: `Complete` | `Partial { omitted, next_call }` | `ProvenEmpty { searched, unsearched }` | `Ambiguous { candidates: [(label, call)] }` | `CannotDetermine { reason, remedy }` | `InvalidRequest { valid_arguments or mode }`.

State is per call; there are no transitions.
