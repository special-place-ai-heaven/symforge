# SymForge 12.0.0 Polyglot & RTK Benchmark (Stage 1 — Deterministic `o200k_base`)

- **Corpus**: 6 real-world open-source repositories (**12,389 indexed files**) across **Rust, C#, TypeScript, Python, Java, and Go**
- **Tasks**: 30 deterministic auto-graded tasks (5 task archetypes × 6 languages)
- **Tokenizer**: `tiktoken` (`o200k_base`) on exact live subprocess outputs & live JSON-RPC `tools/list` schemas
- **Measured MCP Schemas**: `SYMFORGE_SURFACE=full` = **14,187 tokens** (40 tools) vs. `SYMFORGE_SURFACE=code` = **880 tokens** (4 tools, **-93.8%**)

## Per-Language 5-Task Session Token Totals (`Upfront Schema + 5 Tasks`)

| Language | Repository (`Files`) | Native Raw | Native Window | RTK 1.2.1 CLI | SymForge Full MCP | SymForge V12 Code MCP | SymForge CLI + RTK |
|---|---|---:|---:|---:|---:|---:|---:|
| **Rust** | `tokio-rs/tokio` (890) | 81,960 | 14,082 | 50,510 | 23,308 | **3,159** | **1,543** |
| **C#** | `App-vNext/Polly` (986) | 35,621 | 20,560 | 33,234 | 29,832 | **4,821** | **2,492** |
| **TypeScript** | `TanStack/query` (5,594) | 120,129 | 30,709 | 40,684 | 32,452 | **4,154** | **2,244** |
| **Python** | `fastapi/fastapi` (3,003) | 136,517 | 7,558 | 73,335 | 21,499 | **3,270** | **1,806** |
| **Java** | `google/gson` (315) | 77,717 | 27,357 | 73,368 | 77,996 | **5,678** | **3,943** |
| **Go** | `cli/cli` (1,601) | 48,080 | 7,262 | 34,667 | 22,945 | **5,084** | **3,470** |

## Aggregate 6-Repository Totals (30 Tasks Across 6 Sessions)

| Arm | Upfront Schema / Session | 30-Task Payload Tokens | 6-Session Total Tokens | vs. Native Raw | vs. RTK 1.2.1 | Total Turns | Oracle Pass |
|---|---:|---:|---:|---:|---:|---:|---:|
| Native Raw (`rg` + full file) | 0 | 500,024 | **500,024** | -0.0% | +63.5% | 143 | 30/30 |
| Native Window (`rg` + 80L slice) | 0 | 107,528 | **107,528** | -78.5% | -64.8% | 151 | 26/30 |
| RTK 1.2.1 CLI (`rtk rg` + `rtk read`) | 0 | 305,798 | **305,798** | -38.8% | -0.0% | 63 | 30/30 |
| SymForge 12.0 Full MCP (40 tools) | 14,187 | 122,910 | **208,032** | -58.4% | -32.0% | 48 | 30/30 |
| **SymForge 12.0 Code MCP (4 tools)** | 880 | 20,886 | **26,166** | -94.8% | -91.4% | 36 | 30/30 |
| **SymForge 12.0 CLI + RTK Hybrid** | 0 | 15,498 | **15,498** | -96.9% | -94.9% | 36 | 30/30 |
