# Compression Ratio Fixture Runbook

This runbook is a stable Markdown fixture for the `get_file_context` compression
ratio regression test. It intentionally includes several headings and enough
paragraph text to resemble a real project note rather than a tiny synthetic
snippet. The test should measure this file by raw bytes and compare the rendered
context output against that raw size.

## Scope

The fixture describes a code-intelligence service that keeps its primary query
path local, deterministic, and memory resident. Operators expect the context
view to identify important headings without echoing the entire document back to
the caller. That is exactly the property the regression test protects.

## Incident Checklist

1. Confirm the workspace root is the intended project.
2. Confirm the index loaded all normal files and quarantined corrupt input.
3. Confirm context responses include source status and completeness status.
4. Confirm small files are handled with an explicit policy instead of a silent
   pass or a weakened threshold.

## Recovery Notes

If a future change expands the Markdown outline renderer so much that this file
fails the ratio threshold, the failure should print raw bytes, context bytes, and
the exact ratio. The owner can then decide whether the extra output is worth the
larger context budget. Until that decision is made, the threshold should remain
strict and the regression should fail loudly.

## Durable Rationale

The fixture is checked into `tests/fixtures/compression_ratio` so it does not
drift with unrelated repository changes. It is deliberately ordinary prose with
headings, lists, and repeated operational language. That makes it suitable for a
stable regression while avoiding any dependency on RTK runtime behavior.
