# Phase 0 MCP battery corpora

Small cloned repos for in-repo Phase 0 tool batteries (`scripts/phase0-mcp-battery.cjs`).
Not shipped as product fixtures — clone on demand for evidence gathering.

## Clone commands

```powershell
$root = "tests/fixtures/phase0-corpus"
git clone --depth 1 https://github.com/rust-lang/cfg-if.git "$root/cfg-if-rust"
git clone --depth 1 https://github.com/kennethreitz/records.git "$root/records-python"
git clone --depth 1 https://github.com/sindresorhus/is-plain-obj.git "$root/is-plain-obj-ts"
```

Also uses existing `tests/fixtures/compression_ratio/rust` (in-repo).

## Run battery

```powershell
cargo build -p symforge
node scripts/phase0-mcp-battery.cjs target/debug/symforge.exe docs/research/A-001-tool-battery-run1.json
node scripts/phase0-mcp-battery.cjs target/debug/symforge.exe docs/research/A-001-tool-battery-run2.json
```

## L0 A/B (A-019)

```powershell
node scripts/phase0-l0-ab-battery.cjs target/debug/symforge.exe docs/research/A-019-l0-ab-results.json
```

Golden route seed (36 rows): `node scripts/seed-routes-golden.cjs`
