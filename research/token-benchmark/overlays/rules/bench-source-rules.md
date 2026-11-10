---
paths:
  - "**/*.py"
  - "**/*.go"
  - "**/*.ts"
  - "**/*.rs"
---

# Source rules (benchmark fixture)

Marker: BENCH-RULE-SENTINEL-7F3A. The harness counts this marker in each
transcript to observe when the client injected this file.

This file stands in for the path-scoped rule files a real project ships. The
client loads it when the agent reads or edits a matching file through the
client's own tools. It is much smaller than the injected rule text the
2026-11-09 audit measured (a median of about 4,000 tokens across five files,
section B.2); the harness records its byte size and hash in every cell. Edit it
only together with the benchmark version.

## Reading code

- Read the function you are about to change and its direct callers.
- Prefer a bounded read of the region you need over a whole file.
- Check the tests that cover the code before and after a change.

## Changing code

- Keep the diff to what the task asks for. Do not reformat unrelated lines.
- Match the surrounding style: naming, error handling, comment density.
- Do not add a dependency for something a few lines of code can do.
- Keep public signatures stable unless the task asks to change them.

## Verifying

- Run the narrowest test command that covers the change.
- Report failures with the command and its output; do not hide them.
- A change is done when its tests pass, not when it compiles.

## Reviewing

- Report a defect as path:line with one sentence on the failure it causes.
- Separate defects from style remarks.
- Say plainly when you found nothing.
