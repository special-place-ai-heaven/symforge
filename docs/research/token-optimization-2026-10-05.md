# Full-surface token optimization evidence

This work preserves the full native SymForge tool surface. Feature tasks are scored by independent acceptance and the sum of provider input and output tokens across every response, attempt, and feedback turn. Cached input and reasoning-token subsets are not added twice. Missing usage is unknown. A smaller tool result, a README percentage, or a comparison only against stock SymForge does not establish a net saving against ordinary tools.

## Ripgrep availability cohort

The frozen feature adds size sorting to ripgrep at `d5b85d44057ff729a89be9c6549958c45d95aa99`. Ordinary and stock full-40 SymForge availability arms used identical task instructions, model settings (`gpt-6-luna`, high reasoning), source, offline dependency seed, independent suite and 18-check oracle. This stock arm was not the current release candidate. Two pilots preceded five alternating pairs; pilots are excluded from the comparison. The ordinary arm could choose its file and terminal reads without a forced full-file handicap.

All 12 sessions passed acceptance and reconciled provider usage. No native SymForge invocation was observed in any session's complete MCP lifecycle capture. This cohort measures availability under that workflow; it does not isolate symbol retrieval. Exact provider-visible schema bytes were not captured.

| Pair | Ordinary input + output | Stock full-40 input + output | Saving, ordinary minus stock |
| --- | ---: | ---: | ---: |
| 1 | 390,423 | 471,260 | -80,837 |
| 2 | 699,846 | 804,320 | -104,474 |
| 3 | 668,311 | 424,159 | 244,152 |
| 4 | 501,778 | 541,789 | -40,011 |
| 5 | 938,076 | 353,297 | 584,779 |
| Total | 3,198,434 | 2,594,825 | 603,609 |

The paired median saving is **-40,011**, unscaled median absolute deviation is **64,463**, and range is **-104,474 to 584,779**. Three of five pairs cost more with SymForge available. The predeclared positive-total-and-positive-median rule returns **NO_WIN**. The two pilots cost 496,107 and 951,864 tokens respectively and do not enter these statistics. The aggregate difference cannot be attributed to native tool use.

The original campaign outcome remains `INSUFFICIENT`: its final identity collector required a source-commit field omitted by the release builder. The input manifest and source inventory separately pin the same commit. The later reconciled analysis is additive and does not replace the frozen outcome.

## Product improvements and evidence limits

An earlier Dapper candidate used 1,127,134 tokens against ordinary tools' 911,108, with four of five pairs losing tokens and median saving -78,351. It improved on stock SymForge's 1,943,701 tokens, but fails the ordinary-tool NET criterion. Local codec/pruner/encoding diagnostics and unequal-coverage captures also did not qualify.

The release implementation is supported as correctness and retrieval work: byte-verified caller declarations, grammar-derived caller identity, explicit ambiguity and incomplete coverage, compact references that omit declaration bodies, and bounded code context retained when auxiliary knowledge exhausts its budget. Tests cover caller identity/ranges, Unicode identifiers, ambiguity, omitted source bodies, and code/knowledge provenance. Context-cache recovery handles are authoritative metadata carried through final bounding; source text cannot forge one.

All 40 tools remain provisioned. Exactly 69 parameter-description leaves across 31 tools were shortened, with no parameter, type, default, required-field or enumeration changes. The three caller/site routing descriptions satisfy the existing 200-character limit. No new runtime dependency or Python sidecar was introduced. These changes do not establish an end-to-end NET percentage.

The hook HTTP reader also contained a controlled correctness defect: a complete 503 Content-Length:0 response held open past 50 ms was misclassified as unavailable by the EOF-dependent reader. The repair completes length/chunk framing without requiring connection close, retains EOF framing, rejects truncated/ambiguous responses, preserves UTF-8 and newline bytes, bounds response size, and checks a shared deadline before return. A failed regression test on the old reader and 74 passing focused hook tests support this change.

## Evidence pins and harness status

Raw provider captures and account material stay outside git. The sanitized 12-slot accounting analysis is SHA-256 `72971874316755ce98ab23cf51c9ad9552fe3c55fef5309e6f99823e30b44328`; its CSV is `0abedf7e511241dc998683d327c961f2be4cd4974112b1e5a73b28f938fd141c`. The frozen campaign outcome is `82f2d1ebcf14f3102075a382e2d4043a75a22def5a72c61748d0467b1ff2506d`.

The guided feature cohort remains unlaunched. Preparation found and repaired a harness path that selected the old developer prompt; the additive runtime adapter passed all 12 real-manifest prompt checks before any provider session. NET research is deferred. This release carries verified product improvements without a NET-saving claim. The language and operating-system cohorts remain separate.

## Release verification

The historical Linux diagnostic candidate built offline with Rust 1.96.0 and unchanged Cargo.lock. An actual MCP smoke check initialized that binary, listed all 40 tools, retrieved two fixture symbols, and shut down cleanly. An independent binary copy prevented subsequent Cargo builds from changing that artifact. It predates the final hook and test-fixture repairs and is not the published release binary.

Windows checks exposed loopback transport failures in unchanged-base fixtures. Narrow fixture changes preserve the real TCP/Axum/middleware paths, request/status/body assertions and deadlines. API-key controls passed 20 fresh processes, root-guard controls passed 20 processes with four tests each, and the body-consuming daemon-proxy fixture passed 50 fresh processes. Those controls do not measure provider usage.

The historical pre-integration Windows gate run passed all eight commands: formatting, diff checks, strict all-target Clippy, the 10-test source-pin target, all 19 serial hook subprocess tests, default binary build, `cargo test --lib --bins --tests -- --test-threads=1`, and `cargo test --no-default-features --features embed --all-targets --no-run`. The full serial suite recorded 139 target results: **4,627 passed, 0 failed, 19 ignored**. Embed was a compile-only check. The final natural hook case also passed 20/20 fresh subprocesses with the scoped connection-holding fixture; finite repeats do not establish general parallel reliability. Original product deadlines and route/status/body assertions remain.

The gate receipt is SHA-256 `4cd64b0b5ec07e3b3b9a661602c2735eb26a28c8287e8bc8d94abab78bd14587`; the aggregate receipt is `2aae723a87bb98efeed628f0312c37099bb843246190f64f65cfc56d2d33baaf`. The full-suite log is `72b4ff45aacc07b2d2c3c1885dd13ca309730414f8d0ba2a685fea559f588284`. That freeze's V1 source fingerprint covers 203 files and 10,522,872 normalized bytes, SHA-256 `5921e3f31eb85316c9f8309e68bbb1616e5c41ea34a793d1d2288ae5035e9123`. Fingerprint normalization does not alter runtime persistence bytes.

### Subsequent corpus validation exposed a regression

After source integration through PR #771, remote Linux and Windows CI both failed two real-corpus replay cases at the same parser assertion. The earlier local release worktree had no phase0 corpus repositories. Its replay helper returned early when the markers were absent, so the recorded exit-zero suite and pass count did not establish execution of those cases. The release workflow stopped before version preparation or publication.

The new optional reference-evidence path reached the tree-sitter helper for configuration and narrative formats, which previously assumed code-only admission. The repair makes unsupported AST grammars return an error and optional evidence return absent for JSON, TOML, YAML, Markdown, text and env, retaining their dedicated extractors. A regression covers all six formats. Before the repair, an actual-corpus replay run in CI mode reproduced **5 passed, 2 failed, 0 skips**, and the new unit case reproduced the same panic. The first attempted unit invocation executed zero tests because its filters conflicted; that invocation supplies no regression evidence.

The subsequent local runner checks corpus markers, records actual repository commits and marker hashes, enables CI mode for replay, and rejects replay skip messages. The upstream CI clone helper follows branch heads rather than frozen commits; local commits are recorded separately. The repaired source fingerprint uses the same algorithm over 203 files and 10,523,831 normalized bytes, SHA-256 `a3d474a386189f3ab8aacb60aff2b787dda6df41df7f00fa59bcad60ddb5f4bf`. Post-repair verification and publication are separate observations from the earlier local pass.

Post-repair focused checks passed: **419 parsing cases, 0 failed, 2 diagnostic probes ignored**, and **7 actual-corpus replay cases, 0 failed, 0 ignored, zero skip messages** with CI=1. Their full-log SHA-256 values are `e8ae111ebb592854c900898a7f0a6e5779811d7c152d79adef6c4ba3c09a98fa` and `0116afea81fac5a7fab254aa1553e6d7482744fc0176ce64254d86b4daae39cd`. The focused receipt is `94407aed9140c2f4cfed36433c4e260e46971a3e6d4789961908f4af2c4ebe5c`.

A subsequent complete-suite attempt first failed at compilation with unavailable dependency rlib artifacts and no test results. An isolated build directory compiled successfully, but its suite later failed a different local HTTP 500 hook fixture: **3,967 passed, 1 failed, 7 ignored across 59 targets**, with later targets unexecuted. The unchanged response behavior failed 2 of 40 fresh exact-test processes, both classified as `sidecar_port_stale`. Keeping the final response alive until client close with the fixture's existing bounded helper passed 40/40. This terminal wait has no subsequent request to block. The repair changes the fixture and diagnostic assertion text, preserving product deadlines, listener bounds, route/status/body assertions and test count. Those finite controls do not establish general parallel reliability or a kernel-level cause. Failed logs remain evidence, SHA-256 `27f114bead8a975ca5cab1d1d2c5a15369c0c01cec45d87b3d0723b588b955ae` and `e16b64b7a0cbfdec926bc2584f07ae3f23014c1fe9e45c6e7c1085c2d8c533b5` respectively.

The final post-repair Windows runner completed all eight canonical commands with exit zero. The full serial suite recorded **139 targets, 4,628 passed, 0 failed, 19 ignored, zero replay skip messages**, with all three corpus markers and their actual local commits recorded. The separate source-pin and hook targets passed 10 and 19 cases. Embed all-target compilation passed without executing embed tests. Clippy used a separate check-artifact directory from the isolated linked-test build after the earlier compiler artifact failure; no existing cache was deleted. Final receipt SHA-256 is `2c6a35b7a31c58ec47c184beb73a1043838fbab6f9fcabf5887931db3af54290`; full-suite log is `acc96b2c7f1c2ba6c7c29627ba3ce4087ff16faae65a4fe1740100a8b43a35b1`; embed compilation log is `53281452757e941cf3805d886905a88663a6a3680cade6cc7a94e35a206a95b7`. Ignored cases remain outside the passing-assertion count. Publication is independently verified after source integration.
