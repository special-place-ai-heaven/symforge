# Code Mode payload and first-map benchmark (F11–F13)

Run from the checkout with Python 3.10+, Git, and an **already built** SymForge executable:

```sh
python -B research/code-mode-perf/bench.py --binary target/debug/symforge > code-mode-perf.json
```

On Windows, use `py -3` and pass `target/debug/symforge.exe` (or an installed binary's path). The script needs no dependencies beyond the Python standard library and Git. It does not build SymForge, fetch anything, use a model, read an existing checkout as input, or publish results. It creates Git-initialized synthetic fixtures (12 and 2,206 staged Rust files) and an isolated SymForge home in a temporary directory, uses MCP stdio with `SYMFORGE_SURFACE=code`, auto-index enabled, and daemon disabled, then removes the directory. It emits JSON only after all fixture and response checks pass.

## F11: bundle when you need its evidence

For known source locations, read the windows and return only the content you need. Both reads run in a single `code_execute` request:

```python
entry = file_content(path="src/core/entry.rs", start_line=3, end_line=5, show_line_numbers=False)
fields = file_content(path="src/core/types.rs", start_line=1, end_line=4, show_line_numbers=False)
entry["content"] + "\n" + fields["content"]
```

Use `symbol_context(..., bundle=True)` when you need relationship evidence or the definitions of types you have not located yet. If you only want particular evidence discovered by the bundle, project its returned fields in the script. For the fixture, the benchmark compares the full bundle and two full window results as well as two projections that return the same seven source lines. A window does not establish caller coverage or discover dependencies. Projecting a bundle reduces the returned payload, but still runs the bundle query.

## F12: separate startup from map latency

- **Cold:** start a process on a fresh fixture with no pre-existing SymForge state for that root. Submit `repo_map(detail="compact")` immediately after the MCP handshake, without asking for status first. Record the first RPC separately from the first successful map; a loading response is timed but never counted as a map.
- **Ready-first:** use a separate fresh fixture and process. Poll `status` about every 20 ms until it reports `index_ready: true` and all 2,206 files. Then send that process's first map query. Launch-to-ready includes status observation delay.
- **Warm:** repeat the same map in the same ready process. Require identical map text on all samples, across both process types as well as repeats.

Default sample sizes: five cold processes, five ready-first processes, and seven warm requests in each ready process (35 warm requests total). Cold and ready phase order alternates per trial. These are fresh project/process trials, not OS page-cache cold trials. Warm means repeated in-process calls, not restarting with a persisted snapshot. The fixture matches the reported tracked-file count, not private repository content, language mix, Windows environment, client, or daemon topology. The benchmark does not try to reproduce the reported 3.8 seconds.

## F13: choose the smallest result that preserves the evidence

- If totals are enough, `search_text(query="perf_fixture_tag", group_by="count", limit=50)` returns exact counts for the returned files. Here `limit` bounds files, not hits. The count form cannot replace locations or context.
- If you need locations and previews, use the default context-free rows. The lean renderer already groups consecutive rows by path if they fit the single-line table.
- If you need multiline context, group `search_text(..., context=1, limit=50, max_per_file=8)` rows by `row["path"]` in the script and return one heading per file plus each hit's line and `row["context"]` (see `GROUP_CONTEXT` in `bench.py`). This preserves hit locations and numbered context but omits the separate preview and `enclosing_symbol` fields. Raw multiline records repeat the path. This recipe still returns each contextual window; it does not merge overlaps or bypass the hit limits. Both hit forms use the same per-file cap; the released default only returned five of the fixture's eight hits per file.

The harness checks all 24 hit locations and all 24 three-line windows against the actual generated files, and checks that count mode returns eight hits in each of the three files. F11/F13 get one unmeasured pass followed by five measured passes by default, with rotated case order.

## Check bounded structural search on an already-built candidate

```sh
python -B research/code-mode-perf/verify_structural_notice.py --binary target/debug/symforge
```

This separate gate runs one ordinary and one dense structural query through both `code run` and MCP `code_execute` with the daemon disabled. It checks the result the model sees after the script returns just a count. An ordinary cap must report exact omissions. When the matcher skips a dense file, the notice must identify the known omissions as a lower bound and count the unparsed file. All four cases must pass. This is a response-contract check, not a latency benchmark or a daemon interoperability test.

## Reading the receipt

`text_bytes` counts UTF-8 bytes of the actual MCP text blocks, not Rust execution metrics or tokens. `request_bytes` and `response_bytes` count the exact JSON-RPC request and matching response frames including newlines (notifications are excluded). `rpc_ms` is wall-clock time from writing the request through receipt of the response line. `runner_ms` is Code Mode's reported integer-ms duration; it does not include the deferred front's readiness wait. Raw samples and median/min/max are included. The benchmark has exact correctness checks but no fixed latency gate: compare like-for-like builds and machines and account for the observed spread. These measurements exclude model tokenization, tool schemas, model turns, and any external client overhead.

## Observed baseline: published 12.2.0, Linux x86-64

[Raw receipt](baseline-12.2.0-linux.json) from the published binary, SHA-256 `90638440da9c4853fc918e2696318d724057de14c9479b4d209696a2d495c86f`. This is not a build of the review checkout. The fixtures contained 7,469 source bytes (12 files) and 1,699,181 source bytes (2,206 files). All variants remained inline, with no truncation.

| Case | MCP text bytes | Request / response wire bytes | Host calls | Path occurrences |
|---|---:|---:|---:|---:|
| F11 unprojected symbol bundle | 1,370 | 182 / 2,049 | 1 | — |
| F11 unprojected file windows | 441 | 291 / 1,051 | 2 | — |
| F11 projected bundle | 173 | 331 / 775 | 1 | — |
| F11 projected windows | 173 | 326 / 775 | 2 | — |
| F13 location and preview rows | 1,393 | 170 / 2,067 | 1 | 3 |
| F13 raw multiline context records | 6,432 | 181 / 7,220 | 1 | 24 |
| F13 grouped context projection | 3,010 | 446 / 3,705 | 1 | 3 |
| F13 per-file counts | 187 | 174 / 789 | 1 | 3 |

The windows used 67.8% fewer text bytes than the full bundle. The two projections returned byte-identical source text, 87.4% smaller than the full bundle. F13 grouping reduced multiline text by 53.2%; count mode reduced text by 86.6% versus location and preview rows, while returning less information. These are payload savings, not measured reductions in model tokens or execution time.

| F12 compact-map timing | Samples | Median ms | Min–max ms |
|---|---:|---:|---:|
| Cold first successful map RPC | 5 | 1,046.178 | 1,031.207–1,560.112 |
| Cold launch to successful map | 5 | 1,101.816 | 1,090.955–1,616.042 |
| Fresh launch to ready status | 5 | 1,111.176 | 902.112–1,219.791 |
| Ready-first map RPC | 5 | 5.707 | 4.816–8.539 |
| Warm map RPC | 35 | 2.476 | 1.512–8.769 |

Every cold first RPC returned a map without a retry. The cold Code Mode runner's own duration was 3–8 ms (median 4 ms); all 45 map responses had the same 2,577 text bytes. Most of the cold delay on this fixture was outside the Code Mode runner and is consistent with startup/index readiness. The timing does not isolate any specific startup stage, and does not justify changing the `repo_map` renderer. The run was on a shared Linux machine, not a controlled timing environment.
