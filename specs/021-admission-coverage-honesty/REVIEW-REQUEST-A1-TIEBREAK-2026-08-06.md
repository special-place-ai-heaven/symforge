# Tiebreak review — does `PolicyWithheld` disclose a bit that did not exist before?

**For:** a third independent reviewer (Kimi K3), working directly against this repository.
**Branch:** `fix/policy-withheld-skip-reason`. **Base:** `origin/main` @ `93cdd00`.

Two reviewers have already answered this. **They disagree, and you are the tiebreak.**
Read this file fully, then read the code. It is self-contained.

**Do not try to guess which side I want.** I wrote the change under review, I
argued one of the two positions, and I am the least reliable voice here. This
campaign has produced twelve claims that did not survive verification, five of
them mine, including one change shipped and then reverted on measurement.

---

## The change

`src/live_index/store.rs::compatibility_admission_decision` used to map **seven**
distinct `MetadataOnlyReason` variants onto one `SkipReason::UnsupportedLanguage`,
which `Display`s as `"unsupported language"`.

So a secret-detector verdict on perfectly valid TypeScript or Rust was reported to
callers as a **language** problem. An external consumer (TestPilot) hit this on two
valid `.ts` files and concluded TypeScript support was broken. SymForge reproduces
it on its own source: `src/live_index/store.rs`, 0.30 MB of Rust.

The change splits that collapse:

```
SensitivePath | SensitiveContent   -> PolicyWithheld           "withheld by admission policy"
LfsPointer                         -> LfsPointer               "git-lfs pointer"
PlatformPathCollision
  | UnsupportedPathEncoding
  | PathMetadataTooLarge           -> UnsupportedPath          "unsupported path"
UnsupportedTextEncoding            -> UnsupportedTextEncoding  "undecodable text encoding"
```

The two security variants still collapse **together**, so path-rule vs
content-detector remains hidden. Both prior reviewers confirmed that part.

---

## The contested question

`PolicyWithheld` is now a **narrower** set than the old seven-way bucket. A caller
who sees it on a Tier-2 path learns *"a policy demotion occurred"* rather than
*"one of seven things happened"*.

For a path that **cannot** match a sensitive path rule — an ordinary `.rs` or
`.ts` file — the collapse inside `PolicyWithheld` is arguably empty, making the
label a near-oracle for **`SensitiveContent`**: "the secret detector fired here."

Surfaces where the reason string is emitted per-file:

- `search_files` resolve / hits — `format.rs:2927-2928`, `tools.rs:6064`
- `get_repo_map` tree tags — `format.rs:1670-1677`
- daemon cross-project search — `daemon.rs:4977-4979`
- embed `SearchFilesHit.metadata_reason: Option<String>`

### Reviewer 1 (Cursor 2.5): CONFIRMED — acceptable tradeoff

Path-vs-content is preserved, which is the documented contract in
`format.rs:3636-3648`. Policy-vs-technical is a new *public* bit but a defensible
one: hiding it again would require collapsing security back in with encoding/LFS/path,
i.e. reintroducing the misinformation. **Land it.**

### Reviewer 2 (Grok 4.5): PARTIALLY WRONG — this leaks something

Yes, it discloses a bit the old collapse hid: "security/admission policy applied,"
which on innocuous source paths is nearly `SensitiveContent`. Recommendation: if
*"which ordinary files tripped the secret detector"* is sensitive, **do not land
as-is** — keep security demotions in a bucket that still contains at least one
common non-security technical reason, or stop emitting per-file reason strings for
that bucket.

---

## The evidence neither reviewer weighed — check this first

Both reviewers analysed the *index-side* projection in isolation. Neither traced
what the **read path** already discloses about the same file.

`src/protocol/read_gate.rs::admit_disk_read` emits **two different refusals**:

| Cause | Refusal |
|---|---|
| `sensitive_path_rule` matches | `content_withheld_by_admission` |
| recorded `SensitivePath` / `SensitiveContent` | `content_withheld_by_admission` |
| `SensitiveContent` w/ `INDETERMINATE_RULE_ID` (detector failure) | `content_withheld_unscanned` |
| `exceeds_scan_limit` or `decode_searchable_text` err | `content_withheld_unscanned` |
| live `classify_stable_content` -> `SensitiveContent` | `content_withheld_by_admission` |

The split is deliberate and documented: the recovery action differs ("reindex and
retry" vs "reindexing will not change this").

**My reading:** any caller can already learn the policy-vs-technical bit for ANY
path, today, on `origin/main`, by calling `get_file_content` and reading which of
the two refusals comes back. If so, `PolicyWithheld` in search output surfaces the
**same** bit one tool call earlier — it does not create a new one. And under the
old behaviour the honest caller was told "unsupported language" while the curious
caller got the true bit anyway from the read gate one call later.

**This is the claim you are adjudicating. I may be wrong about it.**

### Questions

1. **Is the policy-vs-technical bit genuinely pre-existing?** Trace
   `admit_disk_read` for each population. Is there any file that projects
   `PolicyWithheld` on the search surface but does **not** already yield
   `content_withheld_by_admission` from the read gate? If such a file exists, my
   argument fails and Grok is right.

2. **Is there a caller who can see search/repo-map output but CANNOT call
   `get_file_content`?** This is the strongest version of Grok's position: if some
   surface (cross-project daemon search at `daemon.rs:4977-4979`, or an `embed`
   consumer reading `SearchFilesHit.metadata_reason`) exposes the reason string
   across a boundary where the read gate is not reachable, then the bit really is
   new for that caller. Check the daemon cross-project path specifically.

3. **What is the actual threat model?** SymForge is a local MCP server; the caller
   is typically an agent that already has filesystem access and could read the file
   directly. Does the admission policy exist to (a) stop secrets from being routed
   into an LLM context automatically, or (b) enforce access control against an
   adversary who cannot otherwise read the bytes? Under (a) the oracle concern is
   largely moot; under (b) Grok's objection is serious. Answer from the code and
   comments, not from first principles.

4. **If the bit IS new and IS meaningful**, is Grok's mitigation actually better?
   Folding security demotions back in with `UnsupportedTextEncoding` means telling
   callers a valid, readable, correctly-encoded TypeScript file has an undecodable
   encoding. Is trading one false statement for a weaker oracle a net improvement,
   or is there a third option neither reviewer proposed — e.g. emitting no reason
   string at all for that bucket on the search surface, while keeping the honest
   reason on `get_file_context`?

---

## Already settled — do not re-derive unless you find contrary evidence

Both prior reviewers independently agreed, and I verified each myself:

- **No missed production mapping sites.** Forward map + two reverse maps + two
  compiler-forced test helpers.
- **No snapshot break.** `SkipReason` has no serde derives (`domain/index.rs:1439`);
  snapshots persist `MetadataOnlyReason`, unchanged. Neither reviewer performed a
  live load of a pre-change snapshot — still unverified, flag it if you can test it.
- **The declined TestPilot asks were correctly declined** (machine-readable exclusion
  codes, bounded Tier-2 reads, `force_admit`).
- **`PlatformPathCollision` has no production mint site** — enum definition plus the
  new match arm, nothing else. Harmless but untested in production.
- **The two Display-level tests were not sufficient.** Both reviewers called this
  PARTIALLY WRONG. Already fixed on this branch:
  `security_demotions_project_to_policy_withheld_not_a_language_verdict` in
  `store.rs` now asserts the production projection on entries from a real
  `LiveIndex::load`, including a `.rs` path that can only be a content demotion.

Also already fixed on this branch from the prior round: the stale seven-way-collapse
comments at `query.rs:1258-1264` and `tools.rs:2787-2793`, the overreaching "not
content-derived" claim about `LfsPointer` (it is decided from bytes; it discloses
only that the first <1 KiB match the public LFS grammar), and a `debug_assert`
on the display-only round-trip in `disposition_from_admission`, which would
otherwise relabel a security demotion as an encoding fault.

---

## Deliverable

`specs/021-admission-coverage-honesty/REVIEW-FINDINGS-kimi-2026-08-06.md`:

- **A verdict on the contested question**, and explicitly: which of the two prior
  reviewers is right, or whether both are wrong.
- **A verdict on my read-gate argument** — CONFIRMED / REFUTED, with the trace.
- Answers to questions 1-4 with `file:line` evidence or a command and its output.
- If your honest answer is "this leaks something and should not land," say so
  plainly. That is the single most valuable outcome available here.

## Ground rules

- Read the code before concluding. Every wrong claim in this campaign came from
  reasoning about behaviour instead of reading the implementation.
- Do not modify the repository except to write the findings file.
- Distinguish what you verified, what you inferred, and what you are guessing.
- The full serial suite is ~16-25 min (`cargo test --all-targets -- --test-threads=1`);
  say whether you ran it rather than implying it.
