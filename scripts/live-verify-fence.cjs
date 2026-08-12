#!/usr/bin/env node
// Live end-to-end verification of the T028 root-generation fence.
//
// Slice 1's oracles all run in-process against a slice-local model. This drives
// the REAL binary over the REAL MCP transport and observes the REAL index, which
// is the only way to answer "does it work" rather than "does the model agree
// with itself". CLAUDE.md: verified means seen, not read.
//
// The property under test is the defect the Slice 0 T014 oracle names: a
// generation captured before a root split must not authorize a reindex of root A
// into root B. Observably: after retargeting the live index from root A to root
// B, an edit made under root A must never appear in the index serving root B.
//
// Usage: node scripts/live-verify-fence.cjs <path-to-symforge-binary>
// Exit 0 = property held and was actually observed. Non-zero = it did not, or
// the run could not observe it (which is NOT a pass).

const { spawn } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const BINARY = process.argv[2];
if (!BINARY || !fs.existsSync(BINARY)) {
  console.error(`no binary at ${BINARY}`);
  process.exit(2);
}

const workspace = fs.mkdtempSync(path.join(os.tmpdir(), "symforge-live-"));
const rootA = path.join(workspace, "root-a");
const rootB = path.join(workspace, "root-b");

function seed(root, marker) {
  fs.mkdirSync(path.join(root, "src"), { recursive: true });
  fs.writeFileSync(
    path.join(root, "src", "lib.rs"),
    `pub fn ${marker}_original() -> u32 { 1 }\n`,
  );
}
seed(rootA, "alpha");
seed(rootB, "beta");

let child;
let endpoint;

async function rpc(method, params, id) {
  const response = await fetch(endpoint, {
    method: "POST",
    headers: { "content-type": "application/json", accept: "application/json, text/event-stream" },
    body: JSON.stringify({ jsonrpc: "2.0", id, method, params }),
  });
  const text = await response.text();
  // The transport may answer as SSE; take the last data: line either way.
  const line = text.split("\n").filter((l) => l.startsWith("data:")).pop();
  return JSON.parse(line ? line.slice(5).trim() : text);
}

async function callTool(name, args, id) {
  return rpc("tools/call", { name, arguments: args }, id);
}

function textOf(payload) {
  return payload?.result?.content?.map((c) => c.text).join("\n") ?? JSON.stringify(payload);
}

// Whether the index actually HOLDS the symbol.
//
// Not a substring test. `search_symbols` answers a miss with
// "No symbols matching 'X'. Try: search_text(query=\"X\") ..." -- which contains
// the query term twice. An earlier version of this harness tested
// `JSON.stringify(response).includes(symbol)` and reported a cross-root leak
// that did not exist: it was firing on the server saying it found nothing. The
// observable is the match line, so that is what gets read.
function indexHolds(payload, symbol) {
  const text = textOf(payload);
  if (/^No symbols matching/mu.test(text)) return false;
  const listed = new RegExp(`^\\s*\\d+:\\s+\\w+\\s+${symbol}\\b`, "mu").test(text);
  const counted = /^\s*(\d+) matches? in \d+ files?/mu.exec(text);
  return listed && counted !== null && Number(counted[1]) > 0;
}

function fail(reason) {
  console.error(`LIVE-VERIFY FAIL: ${reason}`);
  if (child) child.kill();
  process.exit(1);
}

async function main() {
  child = spawn(BINARY, ["serve", "--listen", "127.0.0.1:0"], {
    stdio: ["ignore", "pipe", "pipe"],
  });

  const bound = await new Promise((resolve, reject) => {
    let buffer = "";
    const timer = setTimeout(() => reject(new Error("serve never reported a bound address")), 60000);
    const onData = (chunk) => {
      buffer += chunk.toString("utf8");
      const match = buffer.match(/https?:\/\/127\.0\.0\.1:(\d+)(\/mcp)?/);
      if (match) {
        clearTimeout(timer);
        resolve(`http://127.0.0.1:${match[1]}/mcp`);
      }
    };
    child.stdout.on("data", onData);
    child.stderr.on("data", onData);
    child.on("exit", (code) => {
      clearTimeout(timer);
      reject(new Error(`serve exited early with code ${code}: ${buffer.slice(-2000)}`));
    });
  });
  endpoint = bound;
  console.log(`serve bound at ${endpoint}`);

  const init = await rpc("initialize", {
    protocolVersion: "2025-06-18",
    capabilities: {},
    clientInfo: { name: "live-verify-fence", version: "1" },
  }, 1);
  if (!init.result) fail(`initialize failed: ${JSON.stringify(init).slice(0, 400)}`);

  // Index root A and observe its symbol is actually there.
  await callTool("index_folder", { path: rootA }, 2);
  const inA = await callTool("search_symbols", { query: "alpha_original" }, 3);
  const sawAlpha = indexHolds(inA, "alpha_original");
  if (!sawAlpha) fail("precondition: root A's own symbol was not indexed, so nothing below is observable");
  console.log("observed: root A indexed, alpha_original present");

  // Retarget the live index to root B.
  await callTool("index_folder", { path: rootB }, 4);
  const inB = await callTool("search_symbols", { query: "beta_original" }, 5);
  if (!indexHolds(inB, "beta_original")) {
    fail("precondition: root B's own symbol was not indexed after retarget");
  }
  console.log("observed: retargeted to root B, beta_original present");

  // Now mutate root A -- the root the index is NO LONGER serving.
  fs.writeFileSync(
    path.join(rootA, "src", "lib.rs"),
    "pub fn alpha_original() -> u32 { 1 }\npub fn alpha_smuggled() -> u32 { 2 }\n",
  );
  console.log("mutated root A after the split; waiting for any watcher reaction");
  await new Promise((r) => setTimeout(r, 8000));

  const smuggled = await callTool("search_symbols", { query: "alpha_smuggled" }, 6);
  const leaked = indexHolds(smuggled, "alpha_smuggled");

  // And root B must still be intact and serving.
  const stillB = await callTool("search_symbols", { query: "beta_original" }, 7);
  const bIntact = indexHolds(stillB, "beta_original");

  child.kill();

  if (leaked) fail("a post-split edit under root A appeared in the index serving root B");
  if (!bIntact) fail("root B stopped serving its own symbol, so the negative above proves nothing");

  console.log("LIVE-VERIFY PASS: root A's post-split edit did not reach root B, and root B is still serving");
  process.exit(0);
}

main().catch((error) => {
  if (child) child.kill();
  fail(error.message);
});
