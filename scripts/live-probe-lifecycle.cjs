#!/usr/bin/env node
// Does the unevidenced-lifecycle defect produce a USER-VISIBLE symptom?
//
// grok 4.6 found `derive_native_lifecycle` returning (Active, Evidence::None)
// for a unit that declares no status and sits under no archive path, and proved
// it by reading the code and running a unit test. Nobody has checked what a real
// `search_knowledge` call actually prints to a user.
//
// This indexes a knowledge unit with NO declared status, then asks the running
// binary for it and shows the authority line verbatim. If `lifecycle=active`
// appears, the fabrication reaches users and the fix is a shipping fix, not a
// tidy-up. If it does not, the defect is real in code but not user-visible, and
// that changes its priority -- say so rather than assuming.

const { spawn } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const BINARY = process.argv[2];
const workspace = fs.mkdtempSync(path.join(os.tmpdir(), "symforge-lifecycle-"));

fs.mkdirSync(path.join(workspace, "docs"), { recursive: true });
fs.mkdirSync(path.join(workspace, "src"), { recursive: true });
fs.writeFileSync(path.join(workspace, "src", "lib.rs"), "pub fn ready() -> u32 { 1 }\n");

// No `status:` line anywhere. Nothing here declares a lifecycle.
fs.writeFileSync(
  path.join(workspace, "docs", "undeclared.md"),
  "# Current implementation\n\ncode_path = \"src/lib.rs\"\n\nThe widget subsystem resolves requests through the ready path.\n",
);
// A declared one, for contrast.
fs.writeFileSync(
  path.join(workspace, "docs", "declared.md"),
  "# Current implementation\n\nstatus: active\ncode_path = \"src/lib.rs\"\n\nThe declared widget path is explicitly marked active.\n",
);

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
const textOf = (p) => p?.result?.content?.map((c) => c.text).join("\n") ?? JSON.stringify(p);

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

  await rpc("initialize", {
    protocolVersion: "2025-06-18", capabilities: {},
    clientInfo: { name: "lifecycle-probe", version: "1" },
  }, 1);

  console.log(textOf(await callTool("index_folder", { path: workspace }, 2)));

  const search = await callTool("search_knowledge", { query: "widget" }, 3);
  const text = textOf(search);
  console.log("\n===== search_knowledge('widget') =====");
  console.log(text.slice(0, 3000));

  const lifecycleLines = text.split("\n").filter((l) => /lifecycle/i.test(l));
  console.log("\n===== lifecycle lines =====");
  console.log(lifecycleLines.length ? lifecycleLines.join("\n") : "(no line mentions lifecycle)");

  const claimsActive = /lifecycle\s*=\s*active/i.test(text);
  console.log(`\nVERDICT: user-visible 'lifecycle=active' present: ${claimsActive}`);
  console.log("Note: the undeclared unit declares NO status. Any active claim for it is unevidenced.");

  child.kill();
}

main().catch((e) => { if (child) child.kill(); console.error(`probe error: ${e.message}`); process.exit(1); });
