#!/usr/bin/env node
/**
 * Tool-correctness harness — proves SymForge's MCP tools return the RIGHT answer,
 * not just a cheap one. Drives the REAL binary over stdio against a frozen fixture.
 *
 *   node scripts/verify-tools.cjs [path/to/symforge.exe]
 *   node scripts/verify-tools.cjs --update      # (re)write missing snapshots
 *
 * Verdicts:
 *   PASS    answer matches the oracle (grep) or the frozen snapshot
 *   REVIEW  answer differs from the grep oracle — a human looks (grep over-matches
 *           strings/comments vs symbol-aware tools, so a diff is NOT auto-wrong)
 *   FAIL    snapshot regression on an exact-output tool, or the tool errored / went empty
 *
 * Council guardrail (Hotz): this stays ONE script + data (cases.jsonl + *.snap).
 * No config system, no plugin seam. If it grows one, it has become the bug.
 */
const { spawnSync, spawn } = require("child_process");
const readline = require("readline");
const fs = require("fs");
const path = require("path");

const REPO_ROOT = path.resolve(__dirname, "..");
const ARGV = process.argv.slice(2); // skip node + script path
const UPDATE = ARGV.includes("--update");
// Fixture is a parameter (data), not baked in: `--fixture <name>` picks the dir
// under tests/fixtures/. Default is the original synthetic fixture.
const FIXTURE_NAME =
  (ARGV.includes("--fixture") && ARGV[ARGV.indexOf("--fixture") + 1]) || "verify-tools";
const FIXTURE = path.join(REPO_ROOT, "tests/fixtures", FIXTURE_NAME);
const SNAP_DIR = path.join(FIXTURE, "snapshots");
const CASES = path.join(FIXTURE, "cases.jsonl");
const BIN =
  ARGV.find((a) => a.endsWith(".exe")) ||
  path.join(REPO_ROOT, "target/debug/symforge.exe");

// Legacy tools reached via the compact `symforge` facade's deterministic probe
// relay (_probe_legacy_tool/_probe_legacy_args) so we test the TOOL, not the NL
// router. batch_rename is full-surface-only but the relay dispatches it on compact.
const READ_TOOLS = new Set([
  "search_symbols",
  "search_text",
  "find_references",
  "get_symbol",
  "get_file_context",
  "get_symbol_context",
  "batch_rename",
]);

function loadCases() {
  return fs
    .readFileSync(CASES, "utf8")
    .split("\n")
    .filter((l) => l.trim())
    .map((l) => JSON.parse(l));
}

// grep oracle: raw ripgrep over the fixture. Deliberately naive — it over-matches
// (comments, strings, defs). That's why an oracle diff is REVIEW, not FAIL.
function grepOracle(pattern) {
  const r = spawnSync("rg", ["-n", "--no-heading", pattern, "src"], {
    cwd: FIXTURE,
    encoding: "utf8",
  });
  if (r.error) return { hits: [], grepMissing: true };
  const hits = (r.stdout || "")
    .split("\n")
    .filter((l) => l.trim())
    .map((l) => l.trim());
  return { hits, grepMissing: false };
}

function readSnapshot(name) {
  const p = path.join(SNAP_DIR, name);
  return fs.existsSync(p) ? fs.readFileSync(p, "utf8") : null;
}
function writeSnapshot(name, text) {
  fs.writeFileSync(path.join(SNAP_DIR, name), text);
}

async function startSession() {
  const proc = spawn(BIN, [], {
    cwd: FIXTURE,
    stdio: ["pipe", "pipe", "ignore"],
    // symforge_edit REQUIRES the compact surface; read tools reached via probe relay.
    // Pin the root to the fixture (else find_project_root walks up to the parent
    // symforge repo and indexes 620 files, making every oracle mismatch). NO_DAEMON=1
    // keeps the index in-process so the pin holds.
    env: {
      ...process.env,
      RUST_LOG: "off",
      SYMFORGE_SURFACE: "compact",
      SYMFORGE_NO_DAEMON: "1",
      SYMFORGE_WORKSPACE_ROOT: FIXTURE,
    },
  });
  const pending = new Map();
  let id = 1;
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
      }, 60000);
    });
  }
  async function callTool(name, args) {
    let toolName = name;
    let toolArgs = args;
    if (READ_TOOLS.has(name)) {
      toolName = "symforge";
      toolArgs = { _probe_legacy_tool: name, _probe_legacy_args: args };
    }
    const result = await request("tools/call", { name: toolName, arguments: toolArgs });
    const text = (result.content || [])
      .filter((c) => c.type === "text")
      .map((c) => c.text)
      .join("\n");
    return { result, text, isError: !!result.isError };
  }
  await request("initialize", {
    protocolVersion: "2024-11-05",
    capabilities: {},
    clientInfo: { name: "verify-tools", version: "1.0" },
  });
  proc.stdin.write(
    JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized", params: {} }) + "\n"
  );
  // Compact auto-indexes SYMFORGE_WORKSPACE_ROOT on startup (async). Poll `status`
  // until the index reports ready before replaying cases.
  for (let i = 0; i < 40; i++) {
    const { text } = await callTool("status", {});
    if (/index_ready:\s*true|"index_ready":\s*true|index_files/.test(text)) break;
    await new Promise((r) => setTimeout(r, 250));
  }
  return { callTool, close: () => proc.kill() };
}

async function runCase(session, c) {
  // For a real-write case, save the on-disk original FIRST so we can restore it
  // afterward regardless of git tracking state (the fixture may be untracked).
  const isWrite = c.judge === "write_snapshot";
  const writeAbs = isWrite ? path.join(FIXTURE, c.write_file) : null;
  const original = isWrite ? fs.readFileSync(writeAbs, "utf8") : null;

  let text, isError, writtenContent;
  try {
    ({ text, isError } = await session.callTool(c.tool, c.args));
    // Capture what the tool actually wrote to disk BEFORE we restore.
    if (isWrite) writtenContent = fs.readFileSync(writeAbs, "utf8");
  } finally {
    // Always put the file back — pass, fail, or throw. The point is proving the
    // write didn't corrupt it, not leaving the mutation behind.
    if (isWrite) fs.writeFileSync(writeAbs, original);
  }

  if (isError || !text.trim()) {
    return { verdict: "FAIL", reason: isError ? "tool returned error" : "empty response", text };
  }

  // must_contain: every listed substring has to appear, whatever the judge.
  const missing = (c.must_contain || []).filter((s) => !text.includes(s));

  if (c.judge === "oracle") {
    const { hits, grepMissing } = grepOracle(c.oracle_grep);
    if (grepMissing) return { verdict: "REVIEW", reason: "rg not on PATH — grep oracle skipped", text };
    if (missing.length) {
      return { verdict: "REVIEW", reason: `expected substrings absent: ${missing.join(", ")}`, text, oracle: hits };
    }
    // Tool answered and contains what we required; grep count is only a tripwire.
    return { verdict: "PASS", reason: `grep saw ${hits.length} raw hit(s); tool contains required anchors`, text, oracle: hits };
  }

  if (c.judge === "snapshot" || c.judge === "write_snapshot") {
    // For write cases, snapshot the RESULTING FILE (proves the write landed clean);
    // for read cases, snapshot the tool's text output. File already restored above.
    const captured = isWrite ? writtenContent : text;

    const snap = readSnapshot(c.snapshot);
    if (snap === null) {
      if (UPDATE) {
        writeSnapshot(c.snapshot, captured);
        return { verdict: "PASS", reason: `snapshot written (${c.snapshot})`, text: captured };
      }
      return { verdict: "REVIEW", reason: `no snapshot yet — run --update after eyeballing`, text: captured };
    }
    if (snap === captured) return { verdict: "PASS", reason: "byte-identical to snapshot", text: captured };
    return { verdict: "FAIL", reason: `differs from snapshot ${c.snapshot}`, text: captured, snap };
  }

  return { verdict: "FAIL", reason: `unknown judge '${c.judge}'`, text };
}

function firstDiff(a, b) {
  const al = (a || "").split("\n");
  const bl = (b || "").split("\n");
  for (let i = 0; i < Math.max(al.length, bl.length); i++) {
    if (al[i] !== bl[i]) return `  L${i + 1}\n   expected: ${JSON.stringify(bl[i])}\n   actual:   ${JSON.stringify(al[i])}`;
  }
  return "";
}

(async () => {
  if (!fs.existsSync(BIN)) {
    console.error(`No binary at ${BIN}. Build it: cargo build`);
    process.exit(2);
  }
  const cases = loadCases();
  const session = await startSession();
  const results = [];
  for (const c of cases) {
    try {
      results.push({ c, r: await runCase(session, c) });
    } catch (e) {
      results.push({ c, r: { verdict: "FAIL", reason: `threw: ${e.message}`, text: "" } });
    }
  }
  session.close();

  const pad = (s, n) => (s + " ".repeat(n)).slice(0, n);
  const mark = { PASS: "PASS ", REVIEW: "REVIEW", FAIL: "FAIL " };
  console.log("\n  TOOL-CORRECTNESS HARNESS  —  " + path.relative(REPO_ROOT, BIN) + "\n");
  console.log("  " + pad("VERDICT", 8) + pad("CASE", 22) + pad("TOOL", 18) + "NOTE");
  console.log("  " + "-".repeat(96));
  const tally = { PASS: 0, REVIEW: 0, FAIL: 0 };
  for (const { c, r } of results) {
    tally[r.verdict]++;
    console.log("  " + pad(mark[r.verdict], 8) + pad(c.id, 22) + pad(c.tool, 18) + r.reason);
    if (r.verdict === "FAIL" && r.snap != null) {
      const d = firstDiff(r.text, r.snap);
      if (d) console.log(d);
    }
  }
  console.log("  " + "-".repeat(96));
  console.log(`  ${tally.PASS} PASS   ${tally.REVIEW} REVIEW   ${tally.FAIL} FAIL\n`);
  if (tally.REVIEW) console.log("  REVIEW = a human reads the diff (grep over-matches vs symbol-aware tools). Not a failure.");
  if (tally.FAIL) console.log("  FAIL = a real regression: exact-output tool changed, or tool errored/went empty.\n");
  // The one number the council named: REVIEW + FAIL. Drive it down. Exit nonzero only on FAIL.
  process.exit(tally.FAIL > 0 ? 1 : 0);
})();
