# Contract: search compaction (US3)

**Surface**: `search_text` (primary); same ranking helper may be shared with
`search_symbols` file grouping.

## Ranking

1. Group matches by repo-relative `file_path`.
2. Per match line score:
   - `+5` if line matches `(?i)\b(error|fatal|panic|exception|failed)\b`
   - `+3` if query substring present (case-insensitive)
   - `+2` if enclosing symbol name matches query token
3. Sort lines within file by score desc, stable path order tie-break.
4. Sort files by best line score in file desc.

## Caps

| Cap | Default | Override |
|-----|---------|----------|
| Lines per file | 10 | `OutputLimits` existing |
| Files | 20 | `OutputLimits` existing |

**Error preservation**: Lines with error score MUST appear in output even if
they exceed per-file line cap (may exceed total budget → triggers CCR per US2).

## Disclosure

Footer when capped:

```text
(showing {shown}/{total} matches, ranked by relevance)
```

Result-status: `ranked: true`, `truncated: true` if any omitted.

## Frecency

Ranking MUST NOT call `bump_frecency`.

## Tests

- `tests/search_compaction.rs`: error lines preserved, grouping order, footer.
