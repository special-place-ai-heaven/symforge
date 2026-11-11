# Answer table for the reliability fixture

Identifier: `target_fn`, defined at `src/parent/leaf.rs:3`. Occurrences are exact,
case-sensitive, boundary-respecting, one row per occurrence. Classes and rules are
those of `specs/036-tool-reliability/data-model.md`. This file is listed in `.ignore`
so the fixture does not index its own answers.

| # | Location | Class | Rule | Why |
|---|---|---|---|---|
| 1 | `src/parent/leaf.rs:3` | Definition | (target itself) | `pub fn target_fn` |
| 2 | `src/parent/leaf.rs:8` | Confirmed | SameFileUnshadowed | `max(target_fn(10), 1)` in the defining file, no local of that name in `calls_locally` |
| 3 | `src/parent/leaf.rs:11` | Probable | SameFile | `let target_fn = ...` binds a local that shadows the function |
| 4 | `src/parent/leaf.rs:12` | Probable | SameFile | `target_fn(5)` calls the shadowing local, not the function |
| 5 | `src/parent/mod.rs:4` (first) | Confirmed | QualifiedPath | `leaf::target_fn(1)` through the declaration `pub mod leaf;` (line 1) |
| 6 | `src/parent/mod.rs:4` (second) | Confirmed | QualifiedPath | `self::leaf::target_fn(2)` |
| 7 | `src/parent/mod.rs:9` | Confirmed | QualifiedPath | `super::leaf::target_fn(3)` inside `mod inner` |
| 8 | `src/user.rs:1` | Confirmed | ImportPath | `use crate::parent::leaf::target_fn;` |
| 9 | `src/user.rs:4` | Confirmed | IndexedReference | `target_fn(7)` behind that import |
| 10 | `src/unrelated.rs:6` | Excluded | OtherDefinition | `fn target_fn(&self)` is a different symbol, `Thing::target_fn` |
| 11 | `src/unrelated.rs:11` | Excluded | InComment | `// The name target_fn in a comment ...` |
| 12 | `src/unrelated.rs:12` | Excluded | InString | `"target_fn"` |
| 13 | `src/unrelated.rs:15` | Probable | MemberAccess | `thing.target_fn()` is a method call on another type |

Total: 13 occurrences in 4 files. `src/other/leaf.rs` and `src/short.rs`, `src/long.rs`,
`src/lib.rs` contain none.

## Other expected answers

- Dependents of `src/parent/leaf.rs`: `src/parent/mod.rs` (module declaration, rows 5-7)
  and `src/user.rs` (rows 8-9). Not `src/unrelated.rs`, which imports
  `crate::other::leaf::other_fn` only.
- Dependents of `src/other/leaf.rs`: `src/unrelated.rs`, `src/other/mod.rs`.
- References of `target_fn` scoped to `src/parent/leaf.rs`: rows 2-4 and 1, with the
  rows in other files counted as outside scope (rows 5-13).
- `src/parent/leaf.rs`: imports `std::cmp::max`; callers of `target_fn`: `calls_locally`,
  `run_parent`, `via_super`, `use_it`; callees: `step_a` .. `step_e`.
- `src/long.rs`: 300 lines; `long_body` spans lines 4-300; line N (6..298) carries the
  token `LONG_LINE_NNNN`.
- `src/short.rs`: 8 lines.
- `docs/notes.md`: the term `zebrafish_quorum` occurs only there.
- Impact of changing `target_fn` (body edit): hop 1 are its callers `run_parent`, `via_super`,
  `calls_locally`, `use_it`; hop 2 is `top` in `src/lib.rs` (calls `run_parent`). A ranked
  answer must not give all of them the same level.
