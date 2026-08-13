#!/usr/bin/env node
// Diagnostic probe: dump what the live server ACTUALLY returns, so the fence
// verification can assert on observed structure instead of a substring match.
//
// The first harness flagged a leak by testing `JSON.stringify(response)
// .includes("alpha_smuggled")`. Search responses commonly echo the query term,
// so that check cannot distinguish "the symbol is in the index" from "the server
// repeated my query back to me". This prints the payloads so the difference is
// visible before anything is claimed.

const { spawn } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const BINARY = process.argv[2];
const workspace = fs.mkdtempSync(path.join(os.tmpdir(), "symforge-probe-"));
const rootA = path.join(workspace, "root-a");
const rootB = path.join(workspace, "root-b");

for (const [root, marker] of [[rootA, "alpha"], [rootB, "beta"]]) {
  fs.mkdirSync(path.join(root, "src"), { recursive: true });
  fs.writeFileSync(path.join(root, "src", "lib.rs"), `pub fn ${marker}_original() -> u32 { 1 }\n`);
}

let child;
let endpoint;

async function rpc(method, params, id) {
  const response = await fetch(endpoint, {
    method: "POST",
    headers: { "content-type": "application/json", accept: "application/json, text/event-stream" },
    body: JSON.stringify({ jsonrpc: "2.0", id, method, params }),
  });
  const text = await response.text();
  const line = text.split("\n").filter((l) => l.startsWith("data:")).pop();
  return JSON.parse(line ? line.slice(5).trim() : text);
}

const callTool = (name, args, id) => rpc("tools/call", { name, arguments: args }, id);

function show(label, payload) {
  const text = payload?.result?.content?.map((c) => c.text).join("\n") ?? JSON.stringify(payload);
  console.log(`\n===== ${label} =====`);
  console.log(text.slice(0, 1800));
}

async function main() {
  child = spawn(BINARY, ["serve", "--listen", "127.0.0.1:0"], { stdio: ["ignore", "pipe", "pipe"] });
  endpoint = await new Promise((resolve, reject) => {
    let buffer = "";
    const timer = setTimeout(() => reject(new Error("no bound address")), 60000);
    const onData = (chunk) => {
      buffer += chunk.toString("utf8");
      const m = buffer.match(/https?:\/\/127\.0\.0\.1:(\d+)/);
      if (m) { clearTimeout(timer); resolve(`http://127.0.0.1:${m[1]}/mcp`); }
    };
    child.stdout.on("data", onData);
    child.stderr.on("data", onData);
    child.on("exit", (c) => { clearTimeout(timer); reject(new Error(`serve exited ${c}: ${buffer.slice(-1500)}`)); });
  });
  console.log(`serve at ${endpoint}`);
  console.log(`root A = ${rootA}`);
  console.log(`root B = ${rootB}`);

  await rpc("initialize", {
    protocolVersion: "2025-06-18", capabilities: {},
    clientInfo: { name: "probe", version: "1" },
  }, 1);

  show("index_folder A", await callTool("index_folder", { path: rootA }, 2));
  show("search alpha_original (A active)", await callTool("search_symbols", { query: "alpha_original" }, 3));
  show("index_folder B (retarget)", await callTool("index_folder", { path: rootB }, 4));
  show("search beta_original (B active)", await callTool("search_symbols", { query: "beta_original" }, 5));
  show("search alpha_original AFTER retarget", await callTool("search_symbols", { query: "alpha_original" }, 6));

  fs.writeFileSync(
    path.join(rootA, "src", "lib.rs"),
    "pub fn alpha_original() -> u32 { 1 }\npub fn alpha_smuggled() -> u32 { 2 }\n",
  );
  console.log("\n--- mutated root A after the split, waiting 8s ---");
  await new Promise((r) => setTimeout(r, 8000));

  show("search alpha_smuggled AFTER mutating the non-served root", await callTool("search_symbols", { query: "alpha_smuggled" }, 7));
  show("health", await callTool("health_compact", {}, 8));

  child.kill();
}

main().catch((e) => { if (child) child.kill(); console.error(`probe error: ${e.message}`); process.exit(1); });
