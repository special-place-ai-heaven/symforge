#!/usr/bin/env node
/**
 * Phase 2 compact-surface golden battery — one external symforge call per row.
 * Populates STEL extension fields from trust-envelope ledger metadata.
 *
 * Usage:
 *   cargo build -p symforge
 *   node scripts/phase2-compact-battery.cjs [symforge-bin] [output.json]
 *
 * Requires cloned phase0 corpora for full 36-row coverage; skips missing corpora
 * with entries in skippedRows (deterministic clean-checkout behavior).
 */
const { spawn } = require("child_process");
const readline = require("readline");
const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");

const REPO_ROOT = path.resolve(__dirname, "..");
const BIN =
  process.argv[2] ||
  path.join(
    REPO_ROOT,
    "target/debug",
    process.platform === "win32" ? "symforge.exe" : "symforge"
  );
const OUT =
  process.argv[3] || path.join(REPO_ROOT, "docs/research/results-v8-phase2-candidate.json");
const GOLDEN = path.join(REPO_ROOT, "docs/fixtures/routes.golden.jsonl");

const SMALL_FILE = 200;
const WINDOW = 50 * 80;

function tokensFromBytes(n) {
  return Math.ceil(n / 4);
}

function competentManualChars(raw) {
  if (raw < SMALL_FILE) return raw;
  return Math.min(raw, WINDOW);
}

function mTokens(raw) {
  return tokensFromBytes(competentManualChars(raw));
}

function sTokens(text) {
  return tokensFromBytes(Buffer.byteLength(text, "utf8"));
}

function loadGoldenRows() {
  return fs
    .readFileSync(GOLDEN, "utf8")
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line));
}

function baselineCommit() {
  try {
    return execSync("git rev-parse HEAD", { cwd: REPO_ROOT, encoding: "utf8" }).trim();
  } catch {
    return "unknown";
  }
}

function corpusForRow(row) {
  if (row.id === "cfg-if/multi_search_symbol") {
    return path.join(REPO_ROOT, "tests/fixtures/stel_multi_hop/cfg-if-rust");
  }
  if (row.id === "records/multi_context_refs") {
    return path.join(REPO_ROOT, "tests/fixtures/stel_multi_hop/records-python");
  }
  if (row.id === "is-plain/multi_files_content") {
    return path.join(REPO_ROOT, "tests/fixtures/stel_multi_hop/is-plain-obj-ts");
  }
  if (row.id.startsWith("cfg-if/")) {
    return path.join(REPO_ROOT, "tests/fixtures/phase0-corpus/cfg-if-rust");
  }
  if (row.id.startsWith("records/")) {
    return path.join(REPO_ROOT, "tests/fixtures/phase0-corpus/records-python");
  }
  if (row.id.startsWith("is-plain/")) {
    return path.join(REPO_ROOT, "tests/fixtures/phase0-corpus/is-plain-obj-ts");
  }
  if (row.id.startsWith("compression/")) {
    return path.join(REPO_ROOT, "tests/fixtures/compression_ratio/rust");
  }
  throw new Error(`no corpus mapping for ${row.id}`);
}

function markerForRow(row) {
  if (row.id.startsWith("cfg-if/")) return "src/lib.rs";
  if (row.id.startsWith("records/")) return "records.py";
  if (row.id.startsWith("is-plain/")) {
    return row.id === "is-plain/multi_files_content" ? "test.js" : "index.js";
  }
  if (row.id.startsWith("compression/")) return "service.rs";
  return null;
}

function estManualCharsFromFile(corpusDir, marker) {
  const filePath = path.join(corpusDir, marker);
  if (!fs.existsSync(filePath)) return 4000;
  return fs.statSync(filePath).size;
}

/** Competent-manual baseline per phase0 battery (not raw small-file bytes alone). */
function estManualCharsForRow(row, corpusDir, marker) {
  const fileChars = estManualCharsFromFile(corpusDir, marker);
  let taskFloor = row.id.startsWith("records/") ? 6000 : 4000;
  if (Array.isArray(row.must_call) && row.must_call.includes("explore")) {
    taskFloor = Math.max(taskFloor, 8000);
  }
  return Math.max(fileChars, taskFloor);
}

function parseSymforgeOutput(text) {
  const decisionLine = text.match(/^decision: (\w+)/m);
  const predictedLine = text.match(/^predicted: (\d+)/m);
  const ledgerLine = text.match(/^ledger: (\{.+})$/m);
  let ledger = null;
  if (ledgerLine) {
    try {
      ledger = JSON.parse(ledgerLine[1]);
    } catch {
      ledger = null;
    }
  }
  const decision = (ledger && ledger.decision) || (decisionLine && decisionLine[1]) || "serve";
  const chainFailed = text.includes("Multi-hop chain failed:");
  return {
    decision,
    predicted: predictedLine ? Number(predictedLine[1]) : 0,
    ledger,
    chainFailed,
  };
}

function equivalenceForRow(row, parsed, text) {
  if (parsed.decision === "bypass") return "BYPASS";
  if (parsed.decision === "cache_hit") return "EQUIVALENT";
  if (parsed.decision === "degrade") return parsed.chainFailed ? "SYMFORGE-LESS" : "EQUIVALENT";
  if (parsed.chainFailed) return "SYMFORGE-LESS";
  if (row.expected_decision === "bypass") return "BYPASS";
  if (text.includes("── stel ──") && !parsed.chainFailed) return "EQUIVALENT";
  return "PENDING_REVIEW";
}

function stelBlockFromLedger(ledger, parsed) {
  if (!ledger) return null;
  const tools =
    ledger.route_tool && ledger.route_tool.includes("+")
      ? ledger.route_tool.split("+")
      : ledger.route_tool
        ? [ledger.route_tool]
        : [];
  return {
    plan_id: ledger.plan_id || "",
    decision: ledger.decision || parsed.decision,
    tools_called: tools,
    predicted_tokens: parsed.predicted || ledger.output_tokens || 0,
    actual_tokens: ledger.output_tokens || 0,
    net_vs_manual: ledger.predicted_net ?? 0,
    route_confidence: ledger.route_confidence || "inferred",
  };
}

async function runCorpusBatch(corpusDir, rows) {
  const results = [];
  let id = 1;
  const pending = new Map();

  const proc = spawn(BIN, [], {
    cwd: corpusDir,
    stdio: ["pipe", "pipe", "ignore"],
    env: {
      ...process.env,
      RUST_LOG: "off",
      SYMFORGE_SURFACE: "compact",
      // The trust envelope is COMPACT by default and drops the per-call
      // economics lines (`decision:`, `predicted:`, `ledger:`) this battery
      // regex-parses. Force the FULL block so the economics measurement is real.
      SYMFORGE_STEL_FULL: "1",
      SYMFORGE_NO_DAEMON: "1",
    },
  });

  readline.createInterface({ input: proc.stdout }).on("line", (line) => {
    if (!line.trim()) return;
    let msg;
    try {
      msg = JSON.parse(line);
    } catch {
      return;
    }
    if (msg.id != null && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      if (msg.error) reject(new Error(JSON.stringify(msg.error)));
      else resolve(msg.result);
    }
  });

  function request(method, params) {
    return new Promise((resolve, reject) => {
      const myId = id++;
      pending.set(myId, { resolve, reject });
      proc.stdin.write(JSON.stringify({ jsonrpc: "2.0", id: myId, method, params }) + "\n");
      setTimeout(() => {
        if (pending.has(myId)) {
          pending.delete(myId);
          reject(new Error(`timeout ${method}`));
        }
      }, 180000);
    });
  }

  async function callTool(name, args) {
    const result = await request("tools/call", { name, arguments: args });
    const text = (result.content || [])
      .filter((c) => c.type === "text")
      .map((c) => c.text)
      .join("\n");
    return text;
  }

  await request("initialize", {
    protocolVersion: "2024-11-05",
    capabilities: {},
    clientInfo: { name: "phase2-compact-battery", version: "1.0" },
  });
  proc.stdin.write(
    JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized", params: {} }) + "\n"
  );

  await callTool("index_folder", { path: corpusDir });

  for (const row of rows) {
    const marker = markerForRow(row);
    const manualChars = estManualCharsForRow(row, corpusDir, marker);
    const text = await callTool("symforge", { query: row.query });
    const parsed = parseSymforgeOutput(text);
    const S = sTokens(text);
    const M = mTokens(manualChars);
    const equivalence = equivalenceForRow(row, parsed, text);
    const decision = parsed.decision;
    const acceptedServe = decision === "serve" && equivalence === "EQUIVALENT";
    results.push({
      id: row.id,
      corpus: path.basename(corpusDir),
      S,
      M,
      sGteM: S >= M,
      responseBytes: Buffer.byteLength(text, "utf8"),
      acceptedServe,
      equivalence,
      goldenId: row.id,
      decision,
      chain: row.chain || "single",
      mcpCalls: 1,
      eligibleH6: row.eligible_h6 !== false,
      stel: stelBlockFromLedger(parsed.ledger, parsed),
    });
  }

  proc.kill();
  return results;
}

(async () => {
  const goldenRows = loadGoldenRows();
  const skippedRows = [];
  const byCorpus = new Map();

  for (const row of goldenRows) {
    const corpusDir = corpusForRow(row);
    const marker = markerForRow(row);
    if (!marker || !fs.existsSync(path.join(corpusDir, marker))) {
      skippedRows.push(row.id);
      continue;
    }
    if (!byCorpus.has(corpusDir)) byCorpus.set(corpusDir, []);
    byCorpus.get(corpusDir).push(row);
  }

  const allRows = [];
  for (const [corpusDir, rows] of byCorpus.entries()) {
    console.error(`Battery corpus ${corpusDir} (${rows.length} rows)`);
    const batchRows = await runCorpusBatch(corpusDir, rows);
    allRows.push(...batchRows);
  }

  allRows.sort((a, b) => a.id.localeCompare(b.id));

  let session_net_accepted = 0;
  let session_net_all36 = 0;
  for (const row of allRows) {
    session_net_all36 += row.M - row.S;
    if (row.acceptedServe) session_net_accepted += row.M - row.S;
  }

  const output = {
    measuredAt: new Date().toISOString(),
    symforgeBin: BIN,
    surface: "compact",
    method: "ceil(bytes/4); M=competent_manual_window; STEL ledger metadata",
    baselineCommit: baselineCommit(),
    rows: allRows,
    session_net_accepted,
    session_net_all36,
    rowCount: allRows.length,
    skippedRows,
  };

  fs.mkdirSync(path.dirname(OUT), { recursive: true });
  fs.writeFileSync(OUT, JSON.stringify(output, null, 2));
  console.error(`Wrote ${OUT} (${allRows.length} rows, skipped ${skippedRows.length})`);
})().catch((err) => {
  console.error(err);
  process.exit(1);
});
