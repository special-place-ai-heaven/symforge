# 031 — untrue self-claims: three sites where the code misdescribes itself

**Status:** implemented.
**Depends on:** spec 030 (`specs/030-llm-onboarding/`) merging first. All three sites live inside 030's
`OWNS` set (`src/protocol/**`, `src/sidecar/**`, `tests/**`), so executing 031 concurrently would collide.
**Anchors verified:** 2026-08-26, against `main` at the time of writing. **They will drift when 030 lands** —
that is why this document exists now rather than later. Re-locate by symbol name, not by line, after 030 merges.

## Why these three are one spec

Each site makes a claim about itself that is not true: a branch that says it is reachable, a test whose name
says it checks something it does not check, and a comment that says a value can be absent when it never is.
None of them is a runtime bug today. All three are the failure mode `CLAUDE.md`'s reporting invariant exists
to catch — *the thing that reports is not the thing that knows* — in its cheapest, most survivable form:
correct behaviour sitting behind an untrue description, waiting for someone to reason from the description.

They were found incidentally during the 030 review, each while verifying something else. That is the point:
none would have been found by looking for them.

## D1 — an `or_else` branch that can never fire

**Site:** `src/protocol/tools.rs:4832` (as of 2026-08-26), in the `get_file_context` budget resolution:

```rust
let context_max_tokens = Some(format::resolve_read_max_tokens(
    params.0.max_tokens
        .or_else(|| knowledge_requested.then_some(1000))
        .or_else(|| large_default_summary.then_some(LARGE_FILE_CONTEXT_DEFAULT_TOKENS)),
    raw_chars,
));
```

**Why it is unreachable.** `large_default_summary` requires `params.0.sections.is_none()` (`tools.rs:4801`).
`knowledge_requested` is `visible_sections(&params.0.sections).as_ref().is_none_or(...)` (`tools.rs:4807-4809`),
and `visible_sections(&None)` returns `None`, so `is_none_or` on `None` is `true`. Every state in which
`large_default_summary` can be true is a state in which `knowledge_requested` is already true, and the previous
`or_else` has already returned.

**Blast radius: none.** `LARGE_FILE_CONTEXT_DEFAULT_TOKENS` is `1000` (`tools.rs:4783`) and so is the literal
above it, so deleting the branch is behaviourally invisible.

**Decision the spec must make, not presuppose:** delete the branch, or keep it and make it reachable. Keeping
it only makes sense if the intent was for large-file summaries to get a *different* budget from
knowledge-requested calls — in which case the two constants should differ and the ordering should be swapped.
Read the commit that introduced it before choosing.

**Observable check:** whichever way it goes, a test that pins the effective budget for the three input shapes
(`max_tokens` set; omitted with `sections: None`; omitted with `sections: ["outline"]`) and asserts the
resulting symbol cap. Today all three collapse to 1000 for a file ≥ 4000 bytes; if D1 is resolved by making
the branch live, that test is what proves it.

## D2 — a test whose name promises a check it does not perform

**Site:** `tests/conformance.rs:395` `minimal_payloads_deserialize_for_all_tools`; the body at `:409` is

```rust
let _: Value = serde_json::from_str(...);
```

**Why it is a defect.** It re-parses the payload as `serde_json::Value` and never deserializes into any tool's
typed input struct. It therefore cannot detect a schema/serde divergence, which is precisely what its name
advertises. Demonstrated concretely during 030: `get_file_context` advertises `path` in its schema `required`
array while a `paths`-only call must become valid — the exact class of mismatch this test is named for — and
it is `tests/schema_roundtrip.rs:135-159` that catches it, not this test.

**Decision the spec must make, not presuppose:** rename it to what it does (it does have value — it proves the
generated payloads are well-formed JSON), or give it the deserialization it claims. If the latter, check first
whether it would duplicate `schema_roundtrip.rs`'s per-tool coverage; a redundant slow test is worse than an
honest name.

**Observable check:** if renamed — a test asserting the new name matches the body. If implemented — feed it a
deliberately divergent fixture (a struct with a `required` field that serde defaults) and assert it goes red.
A negative control is mandatory here: the whole defect is a check that passes vacuously.

## D3 — a comment describing a branch the caller cannot reach

**Site:** `src/sidecar/handlers.rs:988-993`, the "tool path: unlimited" comment on the `max_tokens` handling
in `outline_text_for_generation`.

**Why it is a defect.** `get_file_context` always passes `Some(budget)` — `context_max_tokens` is
unconditionally `Some(...)` at `tools.rs:4827` and reaches `OutlineParams` at `tools.rs:4872`. The `None`
arm the comment describes is dead for that caller, so anyone reasoning about the outline budget from this
comment concludes the tool route is unbudgeted, which is the opposite of true.

**Do not touch the branch before checking the other caller.** The hook route may legitimately pass `None`.
The defect is the comment's claim about the *tool* path, not necessarily the branch itself.

**Decision the spec must make, not presuppose:** correct the comment, and separately decide whether the `None`
arm still has a live caller. If it does not, the arm goes too; if it does, the comment must name which route
reaches it.

**Observable check:** a test that asserts `get_file_context` never reaches the unlimited arm — e.g. by
asserting a budget is always present in the `OutlineParams` the server route constructs.

## Scope

In: the three sites above and their tests.
Out: any behaviour change to `get_file_context`'s budget resolution beyond what D1's decision requires; the
030 deliverables; parameter-name friction (item 8 of 030, which is its own question).

## Sequencing

1. 030 merges.
2. Re-locate all three sites by symbol name — `get_file_context`, `minimal_payloads_deserialize_for_all_tools`,
   `outline_text_for_generation` — and re-verify each claim above against the merged code before acting.
   Two of the three sites are files 030 edits heavily; treat every line number here as stale.
3. Make the three decisions (delete-vs-fix, rename-vs-implement, comment-vs-branch) explicitly and record them
   before writing code.
4. Then implement, with the negative control on D2 as the non-negotiable one.


## Recorded Decisions (Implemented)

- **D1 (`src/protocol/tools.rs` `get_file_context`):** Deleted the unreachable `.or_else(|| large_default_summary.then_some(LARGE_FILE_CONTEXT_DEFAULT_TOKENS))` branch and the dead `LARGE_FILE_CONTEXT_DEFAULT_TOKENS` constant. `large_default_summary` implies `visible_default_sections`, which implies `knowledge_requested`, so `.or_else(|| knowledge_requested.then_some(1000))` always resolved first to the identical `1000`-token default. Pinned by `read_context::tests::d1_file_context_budget_resolution_pins_all_three_input_shapes`.
- **D2 (`tests/conformance.rs` `minimal_payloads_deserialize_for_all_tools`):** Upgraded `minimal_payloads_deserialize_for_all_tools` to validate every tool's generated minimal payload against its schema `required` keys and declared property JSON types via `validate_minimal_payload_against_schema` (complementing `tests/schema_roundtrip.rs`'s per-tool typed struct deserialization), and added the mandatory negative control `minimal_payload_validator_rejects_missing_or_mistyped_required_field`.
- **D3 (`src/index_lifecycle/guidance/read_context.rs` `outline_text_for_generation`):** Corrected the `max_bytes` comment to accurately state that `Some(n)` is used by `get_file_context` (which always resolves a concrete budget via `resolve_read_max_tokens`) and explicit `max_tokens` callers, `None if include_savings_footer` is the compact sidecar hook default (`200 * 4` bytes), and `None` (with `include_savings_footer == false`) is unbudgeted (`0`) for direct projection callers that omit `max_tokens`. Kept the `None` arm for direct projection callers and pinned both budgeted and unbudgeted projection paths in `read_context::tests::d3_outline_projection_caps_symbols_when_budgeted_and_stays_unbudgeted_when_none`.
## Provenance

Found during the 030 adversarial review: D1 while tracing the default token budget, D2 while checking whether
any pinned contract would block making `path` optional, D3 while establishing which render path
`get_file_context` actually uses. Each was a by-product of verifying a different claim. Independently
re-confirmed for D1 and D2 by a separate zero-context session; D3 confirmed consistent.
