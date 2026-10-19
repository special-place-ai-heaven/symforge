# Contract: No-follow disk read

**Stories:** US1 (F-001), US3 (F-003)
**Implement in:** `src/paths.rs` helper; callers listed in research.md §1

## Helper

```text
path_must_be_regular_non_link(path) -> Result<Metadata, io::Error>
```

1. `symlink_metadata(path)`.
2. If `file_type().is_symlink()` OR `metadata_is_reparse_point(&meta)` → `Err` (not a regular file we may open).
3. If not `file_type().is_file()` → `Err`.
4. Return `Ok(meta)`.

Hasher directory walk inlines `metadata_is_symlink_or_reparse` (no extra sibling function). Unsafe dirs: do not recurse; record warning. Unsafe files: do not hash; record warning. If any warning, trust evaluation is Untrusted.

## Callers

| Lane | Today | After |
|------|-------|-------|
| `admit_disk_read` | `fs::read` | helper then read |
| `admit_worktree_text` | `is_file()` (follows) | helper; missing → `Ok(None)` |
| `observe_disk_beneath` | lexical then `admit_disk_read` | helper inside admission (no extra switch) |
| watcher freshen stat | `fs::metadata` | helper / `symlink_metadata` |
| `read_pass` | `File::open` then following `fs::metadata` for `path_after` | helper then open; `path_after` from opened file or `symlink_metadata` |
| `scout_single_path` | following `std::fs::metadata` | `symlink_metadata` / no-follow |
| freshen path resolve | `safe_repo_path` then `Err => join` | `safe_repo_path` only; Err = no reindex |

## Observability

- Refusal messages MUST NOT include the outside file’s contents.
- Index MUST NOT gain a document whose bytes were read through a link.

## Tests (RED first)

Repo with secret outside root + in-repo file symlink (Windows: also junction):

- `get_file_content` on the link path: no secret, no index insert.
- untracked `search_text` anchored on the secret: no hit.
- sidecar `/workflows/source-read` after freshen: no secret **and** the index does not contain those bytes.
- Windows: directory junction to the secret’s parent, then `junc/secret` — not a file-targeted junction.
- ordinary in-repo file: still served (positive control).
