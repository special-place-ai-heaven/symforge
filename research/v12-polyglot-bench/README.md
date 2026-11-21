# SymForge 12.0.0 Polyglot & RTK Benchmark Suite

Two-stage empirical benchmark comparing **Native (`rg` + file read)**, **RTK `1.2.1` CLI**, **SymForge `12.0.0` Full MCP (`39` tools)**, **SymForge `12.0.0` Code Mode MCP (`4` tools)**, and **SymForge `12.0.0` CLI + RTK Hybrid** across **6 real-world open-source repositories** (`12,389` indexed files):

| Language | Repository | Pinned Commit SHA | Indexed Files |
|---|---|---|---:|
| **Rust** | `tokio-rs/tokio` | `8bdf1e9dfdaa85808bdbe6ac9b0173100fde9951` | 890 |
| **C#** | `App-vNext/Polly` | `ad7d87bf1c1cee65f4a90f1afb192094111df6eb` | 986 |
| **TypeScript** | `TanStack/query` | `0827cf2cb22e2c81ef210271c25fc50e9f1e7328` | 5,594 |
| **Python** | `fastapi/fastapi` | `05e6df7e906c3decba377c37548019973f40bf15` | 3,003 |
| **Java** | `google/gson` | `e5f02ab6c24567d3ca5200afc4076337746b3349` | 315 |
| **Go** | `cli/cli` | `de06af2c2ea536d5722fda35dc2001565f9b9f64` | 1,601 |

## Stage 1 — Deterministic Live Subprocess & `tiktoken` (`o200k_base`) Benchmark ($0, Zero Credentials)

Runs all **30 auto-graded tasks** (`T1_symbol_slice`, `T2_caller_chain`, `T3_subsystem_orient`, `T4_edit_prep`, `T5_literal_git_hybrid`) against the real binaries (`symforge`, `rtk`, `rg`, `git`) and measures exact `o200k_base` tokens on both live JSON-RPC `tools/list` schemas and returned tool payloads:

```bash
py research/v12-polyglot-bench/stage1_deterministic.py --corpus-root C:/AI_STUFF/symforge-bench-corpus
py research/v12-polyglot-bench/render_charts.py
```

## Stage 2 — Smol-LLM Multi-Turn Agent Harness (`--dry-run` & Opt-In `--live`)

Isolates every `(repo, task, arm)` run in a clean detached `git worktree` and grades outcomes with the identical deterministic code oracle (`corpus.grade_output`):

```bash
# Offline deterministic verification (zero network, zero API keys):
py research/v12-polyglot-bench/stage2_smol_agent.py --dry-run --limit-tasks 6

# Opt-in live smol-LLM run from your own terminal (reads OPENROUTER_API_KEY):
py research/v12-polyglot-bench/stage2_smol_agent.py --live --model google/gemini-2.5-flash-lite --limit-tasks 30
```

## Unit Tests

```bash
py -m unittest discover -s research/v12-polyglot-bench/tests -p "test_*.py"
```
