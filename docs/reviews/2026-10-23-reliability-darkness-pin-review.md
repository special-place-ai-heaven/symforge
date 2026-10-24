# Reliability triage: darkness pin review, 2026-10-23

Status: agent review prepared; owner review pending. This is review evidence, not a human attestation or permission to weaken a gate.

Review baseline: `aadad9de82d3df5a9533545aa49c1c32ccfbbd54` (v11.10.0), branch `fix/reliability-triage-2026-10-23`. A checkpoint commit may advance HEAD; comparisons in this record remain against that baseline. Final production source: `def781f6`; its F2/F3 delta was reviewed against checkpoint `2cc08257`. The lead accepted the source review, the authorized test oracle supplied the new pin values, and the complete preventive target passed after the intentional refresh. Owner/human no-doctest attestation remains pending.

## Contract and sanctioned refresh

The existing [2026-10-10 review packet](2026-10-10-darkness-pin-review.md#d-recomputing-the-pins-as-the-test-file-defines-them) and the assertions in `tests/preventive_runtime_dark_v11.rs` require review of the complete source diff before refreshing the semantic seals. They do not require preserving an obsolete digest after an intentional reviewed change.

- `EXCLUDED_RUNTIME_SOURCE_PIN_V1` seals every regular file under `src/index_lifecycle/` plus `src/server_api.rs`.
- `FULL_SOURCE_PIN_V1` seals every regular file under `src/`, regardless of extension, including inline tests and untracked candidates.
- Both triples contain SHA-256, file count, and CRLF-to-LF-normalized byte count. Sorted source-relative paths and lengths are hash-framed by `source_set_fingerprint`, with separate domain prefixes.
- `WORKFLOW_FINGERPRINTS` seals each complete workflow using LF-normalized FNV-1a-64 and byte count. Its purpose includes detecting YAML-scalar, condition, and gate-location changes that a cargo-line census misses.

Retain the existing wired roster, seven exact mount/prose exceptions, excluded path list, cargo-line multiset/census, Cargo-config pin, target topology, and traversal safeguards unless an actual independently reviewed change requires updating one. The current delta does not justify changing them. Existing declared source cardinalities are 89 excluded and 312 total; final oracle output must confirm the observed cardinalities.

## Source boundary review

The inspected delta contains 14 modified source files and no source additions/removals in the tracked/untracked status census. Review covered actual diff hunks, including implementation bodies and inline tests, plus import/re-export/registration and guarded-token deltas.

| Change | Boundary evidence |
|---|---|
| Rust insertion prefix repair: `edit_safety/structural.rs`, `parsing/languages/{mod,rust}.rs`, `protocol/{edit,edit_tools}.rs`, lifecycle `embed_{batch,mutation}.rs` | Lifecycle adapters call existing structural editing infrastructure outward. The new `build_insert_before_at_position`/indentation calls do not introduce an outside caller into lifecycle. The new parser re-export targets only the Rust insertion-prefix helper. No foreign-type impl, exported ABI hook, outward lifecycle re-export, or production registration bridge was added in the inspected delta. Final F2 maps permanent prefix refusals to `InvalidReplacement` in two existing embed adapters. Final F3 resolves symbols, plans offsets, and stages from immutable file captures taken under the Phase-1 read guard; live-byte comparison before staging and the existing on-disk pre-image commit guard remain. Neither correction introduces a lifecycle boundary bridge. |
| Lifecycle freshness: `index_lifecycle/embedded.rs` | Includes a production ordering correction in `publish_observed_current` as well as test-only hooks/tests. It must not be described as a test-only delta. The ordering change stays within the existing binding/runtime path; test hook storage stays in the cfg-gated test module. This is the M03 publication-ordering repair; external F3/H03 instead concerns stale-capture handling in `protocol/edit.rs`. |
| Snapshot integrity: `live_index/persist.rs` | Private shared encode/decode helpers and format-11 checksum framing reuse existing local/artifact/reset/write paths. No new lifecycle or server-API entry is introduced. |
| JavaScript aliases: `domain/index.rs`, `live_index/single_file.rs` | Two extension aliases reuse JavaScript and the existing cold/update admission seam; added runtime tests do not introduce a production lifecycle bridge. |
| HTTP authority gate: `server/{auth,mcp_http,serve}.rs` | Router policy helpers stay within server/auth. The allow-list helper moves within the server layer; no lifecycle/server-API bridge is added. |

No added inbound `index_lifecycle` or `server_api` guarded-token edge was found outside the excluded set. That lexical observation is diagnostic, not a compiler call-graph proof; the source-diff review is what supports accepting the new seal. The excluded set's three changed files are `embed_batch.rs`, `embed_mutation.rs`, and `embedded.rs`; `server_api.rs` is unchanged.

Read-only Terminal Commander receipts, all observed exit 0 in `E:/project/symforge`:

- Complete excluded-source diff: `job_01a16890df36720e8a46f89b54b54576`.
- Structural/parser/protocol delta: `job_01a1689650df76f49bfec303676808af`.
- Server delta: `job_01a16896a17070b9bfb5b35b23416e26`.
- Guarded-token delta: `job_01a16890e6fa71c68540fcf0d95e96a6`.
- Alias/impl/registration delta: `job_01a16892da3870e19bcc50654a9a8fe3`.

The final F2/F3 source/test delta against checkpoint `2cc08257` was then reviewed: `src/index_lifecycle/{embed_batch,embed_mutation}.rs`, `src/protocol/edit.rs`, and `tests/embed_edit_bodies.rs`. The six-file correction packet also includes the intentional seal refresh in `tests/preventive_runtime_dark_v11.rs` and this review document; their pin/closure evidence appears below. Receipt `job_01a1689c8645769d870860c35384d368` completed with observed exit 0 and no masked pipeline. It adds no inbound lifecycle/server-API edge, foreign-type impl, outward lifecycle re-export, production registration, or exported ABI hook. A test-only thread-local capture hook stays under the existing cfg-gated protocol test module. Any further source edit requires reviewing that new delta before recording final pin tuples.

## Workflow execution review

Compared both parsed YAML documents against explicit baseline `aadad9de`, not the checkpoint HEAD. All selected cargo/rustdoc-bearing step dictionaries are identical, including complete run scalars, step conditions, shells, environments, and working directories. All job-level and workflow-level execution contexts are also identical.

| Workflow | Identical selected steps | Identical cargo/rustdoc lines |
|---|---:|---:|
| `ci.yml` | 22 | 28 |
| `release.yml` | 10 | 17 |

The ASCII-space/tab-normalized line multisets remain identical; their combined census is still 45 occurrences, 36 distinct lines, two workflows. No `CARGO_LINES` or census change is justified.

The workflow delta changes Node setup/testing and release-please action-wrapper execution. The added condition applies to the Node recovery-contract test, not to a Rust/doctest gate. The wrapper source downloads and integrity-checks the pinned JavaScript action, performs bounded read retries, and invokes its existing action entry. It contains no cargo/rustdoc or child-process command launcher. Behavior reached from external scripts/actions remains the guard's documented coverage residual; this review does not claim whole-program execution proof.

Explicit-baseline parsed-YAML/context/line-multiset comparison: `job_01a1689825d4763199c7b505a0dd6508`, observed exit 0, no masked pipeline. No Cargo gate was run during this read-only investigation. A human no-doctest review has not been attested by this record.

## Final closure evidence

The lead confirmed frozen production source `def781f6` and independent review of the final F2/F3 delta. Original campaign baseline remains `aadad9de`; source files were not edited during pin closure.

The same command ran before and after the intentional refresh, from `E:/project/symforge`, using the existing target/cache and environment `SYMFORGE_DAEMON_AUTOSPAWN=off`, build jobs 2, dev/test debug info 0:

```text
cargo test -p symforge --features embed --test preventive_runtime_dark_v11 -- --nocapture --test-threads=1
```

| Run | Result | Terminal Commander receipt |
|---|---|---|
| Oracle, before refresh | 10 tests: 7 passed, exactly the 3 expected seal/fingerprint failures; observed exit 101 | `job_01a168a293b670ca823ed5b4fe99e888` |
| Entire target, after refresh | 10 passed, 0 failed, 0 ignored, 0 filtered; observed exit 0 | `job_01a168a51d2772b1a9ad4bd5fddf5ccf` |

Both runs had `pipeline_exit_masked=false` and process cleanup complete. The walk-policy and observation-control fixtures deliberately catch panics; those tests passed in both runs.

These original closure values came from the test oracle assertions and describe source `def781f6`. The hosted-policy follow-up below supersedes only the full-source triple:

| Seal | Final digest/fingerprint | Files | LF-normalized bytes |
|---|---|---:|---:|
| Excluded source | `3a07d1fa97037ed41b8a0c8c11a0b8b6693fd691995729561ae36ad2c589b63f` | 89 | 2256251 |
| Full source | `3bec6a1a3ae7a2b60fd1596c18b7a41e30743ee83b107633f1785a042603eca4` | 312 | 12238202 |
| `ci.yml` | `5b0a8d2def6e8c63:17377` | — | 17377 |
| `release.yml` | `0b3131b900d0f34d:114385` | — | 114385 |

Only the two source triples and two workflow fingerprints changed, with rationale comments linking this record. Wired rosters, excluded pathsets, the seven exceptions, `CARGO_LINES`, the 45/36/2 census, Cargo configuration/topology, and traversal logic remain unchanged. Scoped rustfmt check passed with observed exit 0: `job_01a168a5165476dea0ff58cd157a6060`.

The lead also reported the final correction gates green: embedded insertion integration 2/2 (`job_01a1689d59a271bda78bc6a8caf4fc4d`) and batch tests 17/17 (`job_01a1689f2d7872b5b50af1e74c86fcc1`), both observed exit 0 with cleanup complete. These are lead-provided receipts, distinct from this agent's pin-oracle runs.

Automated closure is complete. Owner review and human no-doctest attestation are pending; this record does not claim either occurred. Pin equality and passing tripwires do not establish general compiler call-graph completeness or behavioral correctness.

## Hosted-policy follow-up (base a55d1564)

After the original closure, the hosted source-policy gate required the cfg(test)-only Rust probe to use the existing hidden-process helper. The exact source delta is one constructor substitution in `src/edit_safety/structural.rs`:

```diff
-            let mut command = std::process::Command::new("rustc");
+            let mut command = crate::process_util::hidden_command("rustc");
```

The surrounding `rust_insert_boundary_tests` module is cfg(test). Arguments, validation, and assertions are unchanged. This introduces no production lifecycle bridge; it uses the existing process helper outward from test code. The full-source seal includes inline tests, while the excluded set does not contain `edit_safety/structural.rs`.

The exact `source_set_fingerprint` framing was reproduced: separate NUL-terminated domain, u64 little-endian record count, sorted UTF-8 source-relative paths with byte-length prefixes, CRLF-to-LF-only content normalization, and content-length prefixes. Before computing working bytes, both old seals were recomputed from Git blobs at `a55d1564baec769149f16efde3c055bb182afbab` and asserted equal to that commit's pins. The working pathset was identical; its sole changed normalized file was asserted to equal precisely the one constructor replacement, adding nine bytes.

| Set | Old | New/current | Files | LF-normalized bytes |
|---|---|---|---:|---:|
| Full source | `3bec6a1a3ae7a2b60fd1596c18b7a41e30743ee83b107633f1785a042603eca4` | `49bb61f511bd5e14100cbda2408d7888f6fe333c1ff286236f45e7a96ad7c202` | 312 | 12238202 → 12238211 |
| Excluded source | `3a07d1fa97037ed41b8a0c8c11a0b8b6693fd691995729561ae36ad2c589b63f` | unchanged | 89 | 2256251 |

Native Terminal Commander evidence, `E:/project/symforge`, both observed exit 0 and unmasked:

- Baseline-pin assertions, exact source-delta assertions, and working seals: `job_01a168e82d4175fab7b5da2594685b6e` (Python/hashlib with Git blob framing).
- Independent working-tree implementation produced identical full and excluded triples: `job_01a168e9ac317722ba9848be1882f249` (Node crypto with an independent byte-normalization/framing implementation).
- Actual one-line source diff inspected: `job_01a168e653c47625b69944d83eb62251`.

Only the full-source digest and normalized-byte total are refreshed for this follow-up. Excluded source seal, domains, counts, rosters, pathsets, workflow fingerprints, cargo policies, and traversal logic remain unchanged. The six-file scope wording above was also corrected to include the pin test and review document.

No Cargo command or build-target recreation was performed for this follow-up. The original 10/10 Rust-target receipt above remains historical evidence for its original source snapshot; it is not a rerun on these new bytes. Owner/human no-doctest attestation remains pending.
