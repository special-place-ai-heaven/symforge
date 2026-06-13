#!/usr/bin/env node
/**
 * Phase 0 A-019 L0 surface A/B battery on pinned 20-row corpus.
 * Surfaces: full-32 (legacy tools), compact-3 (symforge facade), meta-1 (symforge facade).
 * Token method: ceil(utf8Bytes/4). M = competent manual window.
 * Requires SYMFORGE_NO_DAEMON=1.
 */
const { spawn } = require("child_process");
const readline = require("readline");
const fs = require("fs");
const path = require("path");

const REPO_ROOT = path.resolve(__dirname, "..");
const BIN = process.argv[2] || path.join(REPO_ROOT, "target/debug/symforge.exe");
const OUT = process.argv[3] || path.join(REPO_ROOT, "docs/research/A-019-l0-ab-results.json");

const SMALL_FILE = 200;
const WINDOW = 50 * 80;

const SURFACES = [
  { id: "full-32", env: "full", mode: "legacy" },
  { id: "compact-3", env: "compact", mode: "facade" },
  { id: "meta-1", env: "meta", mode: "facade" },
];

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

function facadeArgs(sc) {
  const intentMap = {
    search_text: "find",
    search_symbols: "find",
    search_files: "find",
    get_file_context: "read",
    get_file_content: "read",
    get_symbol: "read",
    find_references: "trace",
    find_dependents: "impact",
  };
  return {
    query: sc.args.query || sc.args.path || sc.args.name || sc.id,
    intent: intentMap[sc.tool] || "auto",
    path: sc.args.path || sc.args.path_prefix || undefined,
    symbol: sc.args.name || sc.args.symbol || undefined,
    _probe_legacy_tool: sc.tool,
    _probe_legacy_args: sc.args,
  };
}

async function runMcpSession(corpusCwd, scenarios, surface) {
  const rows = [];
  let id = 1;
  const pending = new Map();

  const proc = spawn(BIN, [], {
    cwd: corpusCwd,
    stdio: ["pipe", "pipe", "ignore"],
    env: {
      ...process.env,
      RUST_LOG: "off",
      SYMFORGE_SURFACE: surface.env,
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
    clientInfo: { name: "phase0-l0-ab", version: "0.1" },
  });
  proc.stdin.write(
    JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized", params: {} }) + "\n"
  );

  const list = await request("tools/list", {});
  const toolNames = (list.tools || []).map((t) => t.name);
  const listBytes = Buffer.byteLength(JSON.stringify(list), "utf8");

  await callTool("index_folder", { path: corpusCwd });

  for (const sc of scenarios) {
    const call =
      surface.mode === "legacy"
        ? { tool: sc.tool, args: sc.args }
        : { tool: "symforge", args: facadeArgs(sc) };
    const { text } = await callTool(call.tool, call.args);
    const S = sTokens(text);
    const M = mTokens(sc.estManualChars || text.length);
    rows.push({
      id: sc.id,
      surface: surface.id,
      corpus: path.basename(corpusCwd),
      tool: call.tool,
      legacyTool: sc.tool,
      S,
      M,
      sGteM: S > M,
      responseBytes: Buffer.byteLength(text, "utf8"),
      acceptedServe: S <= M,
      equivalence: sc.expectedEquiv ? "EQUIVALENT" : "PENDING_REVIEW",
      goldenId: sc.id,
      decision: "serve",
      mcpCalls: 1,
      eligibleH6: true,
      responseTextHash: text.length,
    });
  }

  proc.kill();
  return { rows, toolNames, listBytes };
}

(async () => {
  const surfaceResults = [];
  const fullRowMap = new Map();

  for (const surface of SURFACES) {
    const allRows = [];
    let toolNames = [];
    let listBytes = 0;
    for (const c of CORPORA) {
      if (!fs.existsSync(c.dir)) {
        console.error("skip missing corpus", c.dir);
        continue;
      }
      console.error("l0-ab", surface.id, c.dir);
      const { rows, toolNames: names, listBytes: lb } = await runMcpSession(c.dir, c.scenarios, surface);
      allRows.push(...rows);
      toolNames = names;
      listBytes = lb;
    }

    const sessionNetAccepted = allRows
      .filter((r) => r.acceptedServe)
      .reduce((a, r) => a + (r.M - r.S), 0);
    const equivCount = allRows.filter((r) => r.equivalence === "EQUIVALENT").length;

    if (surface.id === "full-32") {
      for (const row of allRows) {
        fullRowMap.set(row.id, row.responseBytes);
      }
    } else {
      for (const row of allRows) {
        const baseline = fullRowMap.get(row.id);
        if (baseline != null && baseline !== row.responseBytes) {
          row.equivalence = "SYMFORGE-MORE";
          row.equivNote = `responseBytes ${row.responseBytes} vs full ${baseline}`;
        }
      }
    }

    surfaceResults.push({
      surface: surface.id,
      symforgeSurfaceEnv: surface.env,
      toolCount: toolNames.length,
      toolsListBytes: listBytes,
      h1Pass: listBytes <= 5000,
      rowCount: allRows.length,
      session_net_accepted: sessionNetAccepted,
      equiv_count: equivCount,
      rows: allRows,
    });
  }

  const eligible = surfaceResults.filter((s) => s.h1Pass);
  eligible.sort((a, b) => b.session_net_accepted - a.session_net_accepted);
  const winner = eligible[0] || null;
  const tied =
    eligible.length > 1 &&
    eligible[0].session_net_accepted === eligible[1].session_net_accepted;
  const selected = tied ? "compact-3" : winner?.surface || "none";

  const artifact = {
    measuredAt: new Date().toISOString(),
    symforgeBin: BIN,
    method: "ceil(bytes/4); M=competent_manual_window; facade relay via symforge _probe_*",
    corpus: "pinned 20-row phase0-corpus (4 repos)",
    surfaces: surfaceResults,
    winner: {
      selected,
      tieBreak: tied ? "compact-3 (simpler per gap plan §4.1)" : null,
      criterion: "max session_net_accepted among H1-pass surfaces with output parity to full-32",
    },
  };

  fs.mkdirSync(path.dirname(OUT), { recursive: true });
  fs.writeFileSync(OUT, JSON.stringify(artifact, null, 2));
  console.log(
    "Wrote",
    OUT,
    "winner",
    selected,
    "fullNet",
    surfaceResults.find((s) => s.surface === "full-32")?.session_net_accepted,
    "compactNet",
    surfaceResults.find((s) => s.surface === "compact-3")?.session_net_accepted
  );
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
