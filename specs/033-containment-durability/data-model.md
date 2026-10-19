# Data Model: 033-containment-durability

No new persisted schema. This feature hardens existing entities.

## OutboundLink

- **Identity:** A directory entry under a workspace or project-config path.
- **Observation:** `symlink_metadata` (never `metadata`).
- **Fields:** `is_symlink`, `is_reparse_point` (Windows), `is_regular_file`, `is_dir`.
- **Invariant:** Content, hash, and reindex lanes never follow. Hasher does not recurse.
- **Transitions:** Detected → skipped/refused (with warning on trust walk). Never Detected → hashed/indexed.

## FreshenRefusal

- **Identity:** Result of a freshen path resolve or no-follow check.
- **States:** `ResolvedRegularFile` | `ContainmentFailure` | `LinkOrReparse` | `Missing` (no-op).
- **Invariant:** `ContainmentFailure` and `LinkOrReparse` never publish bytes into the live index.
- **Forbidden transition:** any error → `Ok(canon_root.join(relative))`.

## BindDecision

- **Inputs:** configured host, opt-in flag, token presence.
- **States:**
  - `Loopback` — start, no Bearer.
  - `NonLoopbackRefused` — no opt-in, or opt-in without token.
  - `NonLoopbackAuthenticated` — opt-in + token; warn; Bearer required.
- **Invariant:** A process in `NonLoopbackRefused` has no listener.

## DurableFile

- **Identity:** Snapshot `index.bin`, team `index.bin.zst` + `artifact.json`, `trust.json`, editor MCP JSON.
- **Write sequence:** tmp bytes → `sync_all` → rename → (Unix) parent `sync_all`.
- **Success reporting:** only after the sequence returns `Ok`.
- **Crash mid-sequence:** previous durable bytes remain parseable, or file absent (fail-closed for trust).
- **Out of this entity:** the daemon token file. US6 is Unix `0o600` / Windows owner-only DACL on `fs::write` (including replace). Token persist is not required to use this atomic sequence.

## ByteRange

- **Fields:** `start: u32`, `end: u32`, buffer length, (for `&str`) char boundaries.
- **Valid:** `start <= end <= len`, and on text both ends are char boundaries.
- **Invalid:** structured `Err`; never panic.

## TrustHashInput

- **Included:** regular non-link files under `.symforge/config`.
- **Excluded:** symlink, reparse, junction targets.
- **If any excluded entry present:** evaluation Untrusted + warning (fail-closed), not a hash of a partial tree presented as the reviewed predicate.

## Validation rules (from spec)

| Entity | Rule |
|--------|------|
| OutboundLink | Follow is unrepresentable in the read helper’s success path |
| FreshenRefusal | Join-fallback unrepresentable (no `Err` arm that returns `Ok`) |
| BindDecision | Non-loopback without opt-in+token cannot be `Ok` |
| DurableFile | Success enum/return only after sync+rename (token ACL is a separate US6 rule) |
| ByteRange | Constructor/`apply_splice` returns Result |
| TrustHashInput | Reparse/symlink never in file list |
