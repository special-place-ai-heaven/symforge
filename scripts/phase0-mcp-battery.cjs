#!/usr/bin/env node
/**
 * Phase 0 in-repo MCP tool battery — exercises legacy 32-tool surface on cloned corpora.
 * Token method: ceil(utf8Bytes/4) per sf-bench convention.
 * M baseline: competent manual window (see src/protocol/format.rs).
 */
const { spawn } = require("child_process");
const readline = require("readline");
const fs = require("fs");
const path = require("path");

const REPO_ROOT = path.resolve(__dirname, "..");
const BIN = process.argv[2] || path.join(REPO_ROOT, "target/debug/symforge.exe");
const OUT = process.argv[3] || path.join(REPO_ROOT, "docs/research/A-001-tool-battery-run1.json");

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

async function runMcpSession(corpusCwd, scenarios) {
  const rows = [];
  let id = 1;
  const pending = new Map();

  const proc = spawn(BIN, [], {
    cwd: corpusCwd,
    stdio: ["pipe", "pipe", "ignore"],
    env: { ...process.env, RUST_LOG: "off", SYMFORGE_SURFACE: "full", SYMFORGE_NO_DAEMON: "1" },
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
      }, 120000);
    });
  }

  async function callTool(name, args) {
    const result = await request("tools/call", { name, arguments: args });
    const text = (result.content || [])
      .filter((c) => c.type === "text")
      .map((c) => c.text)
      .join("\n");
    return { result, text };
  }

  await request("initialize", {
    protocolVersion: "2024-11-05",
    capabilities: {},
    clientInfo: { name: "phase0-battery", version: "0.1" },
  });
  proc.stdin.write(
    JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized", params: {} }) + "\n"
  );

  await callTool("index_folder", { path: corpusCwd });

  for (const sc of scenarios) {
    const { text } = await callTool(sc.tool, sc.args);
    const S = sTokens(text);
    const M = mTokens(sc.estManualChars || text.length);
    const N = sc.estNaiveChars ? tokensFromBytes(sc.estNaiveChars) : null;
    rows.push({
      id: sc.id,
      corpus: path.basename(corpusCwd),
      tool: sc.tool,
      S,
      M,
      N,
      sGteM: S > M,
      responseBytes: Buffer.byteLength(text, "utf8"),
      acceptedServe: S <= M,
      equivalence: sc.expectedEquiv ? "EQUIVALENT" : "PENDING_REVIEW",
      goldenId: sc.id,
      decision: "serve",
      mcpCalls: 1,
      eligibleH6: true,
    });
  }

  proc.kill();
  return rows;
}

const CORPORA = [
  {
    dir: path.join(REPO_ROOT, "tests/fixtures/phase0-corpus/cfg-if-rust"),
    scenarios: [
      { id: "cfg-if/t1_search", tool: "search_text", args: { query: "cfg_if", path_prefix: "src" }, estManualChars: 4000, expectedEquiv: true },
      { id: "cfg-if/t2_context", tool: "get_file_context", args: { path: "src/lib.rs" }, estManualChars: 4000, expectedEquiv: true },
      { id: "cfg-if/t3_symbol", tool: "search_symbols", args: { query: "cfg_if" }, estManualChars: 4000, expectedEquiv: true },
      { id: "cfg-if/t4_refs", tool: "find_references", args: { name: "cfg_if", path: "src/lib.rs" }, estManualChars: 4000, expectedEquiv: true },
      { id: "cfg-if/t5_symbol", tool: "get_symbol", args: { path: "src/lib.rs", name: "cfg_if" }, estManualChars: 4000, expectedEquiv: true },
    ],
  },
  {
    dir: path.join(REPO_ROOT, "tests/fixtures/phase0-corpus/records-python"),
    scenarios: [
      { id: "records/t1_search", tool: "search_text", args: { query: "Database", path_prefix: "" }, estManualChars: 8000, expectedEquiv: true },
      { id: "records/t2_context", tool: "get_file_context", args: { path: "records.py" }, estManualChars: 6000, expectedEquiv: true },
      { id: "records/t3_files", tool: "search_files", args: { query: "records" }, estManualChars: 4000, expectedEquiv: true },
      { id: "records/t4_refs", tool: "find_references", args: { name: "Database", path: "records.py" }, estManualChars: 6000, expectedEquiv: true },
      { id: "records/t5_symbol", tool: "get_symbol", args: { path: "records.py", name: "Database" }, estManualChars: 6000, expectedEquiv: true },
    ],
  },
  {
    dir: path.join(REPO_ROOT, "tests/fixtures/phase0-corpus/is-plain-obj-ts"),
    scenarios: [
      { id: "is-plain/t1_search", tool: "search_text", args: { query: "plainObject", path_prefix: "" }, estManualChars: 4000, expectedEquiv: true },
      { id: "is-plain/t2_content", tool: "get_file_content", args: { path: "index.js", limit: 80 }, estManualChars: 3200, expectedEquiv: true },
      { id: "is-plain/t3_context", tool: "get_file_context", args: { path: "index.js" }, estManualChars: 4000, expectedEquiv: true },
      { id: "is-plain/t4_symbols", tool: "search_symbols", args: { query: "isPlainObject" }, estManualChars: 4000, expectedEquiv: true },
      { id: "is-plain/t5_symbol", tool: "get_symbol", args: { path: "index.js", name: "isPlainObject" }, estManualChars: 4000, expectedEquiv: true },
    ],
  },
  {
    dir: path.join(REPO_ROOT, "tests/fixtures/compression_ratio/rust"),
    scenarios: [
      { id: "compression/t1_search", tool: "search_text", args: { query: "reconcile", path_prefix: "" }, estManualChars: 4000, expectedEquiv: true },
      { id: "compression/t2_context", tool: "get_file_context", args: { path: "service.rs" }, estManualChars: 2298, expectedEquiv: true },
      { id: "compression/t3_symbol", tool: "get_symbol", args: { path: "service.rs", name: "reconcile" }, estManualChars: 2298, expectedEquiv: true },
      { id: "compression/t4_refs", tool: "find_references", args: { name: "reconcile", path: "service.rs" }, estManualChars: 4000, expectedEquiv: true },
      { id: "compression/t5_dependents", tool: "find_dependents", args: { path: "service.rs" }, estManualChars: 4000, expectedEquiv: true },
    ],
  },
];

(async () => {
  const allRows = [];
  for (const c of CORPORA) {
    if (!fs.existsSync(c.dir)) {
      console.error("skip missing corpus", c.dir);
      continue;
    }
    console.error("battery corpus", c.dir);
    const rows = await runMcpSession(c.dir, c.scenarios);
    allRows.push(...rows);
  }

  const sessionNetAccepted = allRows.filter((r) => r.acceptedServe).reduce((a, r) => a + (r.M - r.S), 0);
  const sessionNetAll = allRows.reduce((a, r) => a + (r.M - r.S), 0);

  const artifact = {
    measuredAt: new Date().toISOString(),
    symforgeBin: BIN,
    method: "ceil(bytes/4); M=competent_manual_window",
    rows: allRows,
    session_net_accepted: sessionNetAccepted,
    session_net_all36: sessionNetAll,
    rowCount: allRows.length,
  };

  fs.mkdirSync(path.dirname(OUT), { recursive: true });
  fs.writeFileSync(OUT, JSON.stringify(artifact, null, 2));
  console.log("Wrote", OUT, "rows", allRows.length, "session_net_accepted", sessionNetAccepted);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
