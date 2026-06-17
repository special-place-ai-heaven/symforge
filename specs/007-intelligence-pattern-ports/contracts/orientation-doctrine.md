# Contract: Orientation Doctrine

**Feature**: 007 · **Requirements**: FR-005, FR-006 · **Story**: US2

## Doctrine statements (canonical wording)

The following meanings MUST appear (exact phrasing may be tuned for tone but the
three semantics are mandatory):

1. **Map orients, tools prove** — "The map orients; the tools prove."
2. **Absence is not absence** — "Absence from the map is not absence from the
   repo — confirm with `search_symbols` / `search_text` before concluding
   something is missing."
3. **Truncation disclosed** — the compact map listing is ranked/truncated;
   disclose using the existing completeness vocabulary ("Completeness" /
   "truncated by result cap"), not new parallel phrasing.

## Placement

| Surface | Site | Requirement |
|---------|------|-------------|
| Onboarding prompt | `prompts.rs::build_onboard_instructions` (~L345-381) | statements 1 + 2 |
| Architecture-map prompt | `prompts.rs::build_architecture_map_instructions` (~L268-300) | statements 1 + 2 |
| Repo-map resource body + `get_repo_map` compact output | `tools.rs` compact arm footer (`format!("{result}{hint}")`, ~L3526-3534) | statements 1 + 2 + 3 |

- The repo-map doctrine is added in the `get_repo_map` compact **footer** (after
  the budgeted body) so it covers BOTH the `get_repo_map` tool and the
  `symforge://repo/map` resource in one place, and is not lost to the
  `build_with_budget` truncation.
- Editing `resources.rs` alone does NOT change the map body (it only routes) —
  do not rely on it for the doctrine text.

## Constraints

- Doctrine is 1-2 short lines; must not push real map content past the byte
  budget (place in the footer, not inside the budgeted `lines` vec).
- Disclosure wording consistent with `format_context_envelope` /
  `search_completeness_label` (Constitution Principle III).

## Test obligations

- `prompts.rs::tests`: assert the onboarding + architecture prompt bodies contain
  statements 1 and 2 (substring assertions; pattern after
  `test_code_review_prompt_includes_resource_links`).
- `resources.rs::test_read_static_repo_map_resource` (extend): assert the map body
  contains the doctrine substring(s).
- A regression that drops a doctrine line fails a test (no silent loss).
