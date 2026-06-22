#!/usr/bin/env node
/**
 * A-029 T2 equivalence spike — compact `symforge` on tokio + django reference tasks.
 *
 * Usage:
 *   cargo build -p symforge
 *   node scripts/a029-t2-spike.cjs [symforge-bin] [output.json]
 *
 * Requires cloned corpora under tests/fixtures/a029-t2/{tokio,django}.
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
  process.argv[3] || path.join(REPO_ROOT, "docs/research/a029-t2-results.json");
const TASKS = path.join(REPO_ROOT, "tests/fixtures/a029-t2/tasks.jsonl");
const FIXTURE_ROOT = path.join(REPO_ROOT, "tests/fixtures/a029-t2");

function baselineCommit() {
  try {
    return execSync("git rev-parse HEAD", { cwd: REPO_ROOT, encoding: "utf8" }).trim();
  } catch {
    return "unknown";
  }
}

function loadTasks() {
  return fs
    .readFileSync(TASKS, "utf8")
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line));
}

function corpusDir(repo) {
  return path.join(FIXTURE_ROOT, repo);
}

/** Ripgrep baseline: unique source files referencing symbol (sidecar parity proxy). */
function baselineReferencePaths(corpusDirPath, symbol, repo) {
  const glob = repo === "django" ? "*.py" : "*.rs";
  try {
    const pattern = repo === "django" ? `\\b${symbol}\\b` : `\\b${symbol}\\b`;
    const out = execSync(
      `rg -l --glob '${glob}' --glob '!target/**' --glob '!tests/fixtures/**' '${pattern}' .`,
      { cwd: corpusDirPath, encoding: "utf8", maxBuffer: 10 * 1024 * 1024 }
    );
    return out
      .trim()
      .split("\n")
      .filter(Boolean)
      .map((p) => p.replace(/^\.\//, "").replace(/\\/g, "/"));
  } catch {
    return [];
  }
}

function parseSymforgeOutput(text) {
  const ledgerLine = text.match(/^ledger: (\{.+})$/m);
  let ledger = null;
  if (ledgerLine) {
    try {
      ledger = JSON.parse(ledgerLine[1]);
    } catch {
      ledger = null;
    }
  }
  const decision = (ledger && ledger.decision) || "serve";
  const chainFailed = text.includes("Multi-hop chain failed:");
  const tools =
    ledger && ledger.route_tool
      ? ledger.route_tool.includes("+")
        ? ledger.route_tool.split("+")
        : [ledger.route_tool]
      : [];
  return { decision, ledger, chainFailed, tools_called: tools };
}

/** Extract indexed file paths cited in find_references compact output. */
function extractCitedPaths(text) {
  const paths = new Set();
  for (const line of text.split("\n")) {
    const trimmed = line.trim();
    if (/\.(rs|py)$/.test(trimmed) && !trimmed.startsWith(":")) {
      paths.add(trimmed.replace(/\\/g, "/"));
    }
    const m = trimmed.match(/^([^\s:]+\.(?:rs|py)):/);
    if (m) paths.add(m[1].replace(/\\/g, "/"));
  }
  return [...paths];
}

function computeRecall(baselinePaths, citedPaths) {
  if (baselinePaths.length === 0) return { matched: 0, recall: 0 };
  const baselineSet = new Set(baselinePaths.map((p) => p.toLowerCase()));
  let matched = 0;
  for (const cited of citedPaths) {
    const norm = cited.toLowerCase();
    if (baselineSet.has(norm)) matched += 1;
  }
  return { matched, recall: matched / baselinePaths.length };
}

async function runRepoBatch(corpusDirPath, tasks) {
  const results = [];
  let id = 1;
  const pending = new Map();

  const proc = spawn(BIN, [], {
    cwd: corpusDirPath,
    stdio: ["pipe", "pipe", "ignore"],
    env: {
      ...process.env,
      RUST_LOG: "off",
      SYMFORGE_SURFACE: "compact",
      // The trust envelope is COMPACT by default and drops the per-call
      // economics lines (`decision:`, `predicted:`, `ledger:`) this spike
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
      }, 600000);
    });
  }

  async function callTool(name, args) {
    const result = await request("tools/call", { name, arguments: args });
    return (result.content || [])
      .filter((c) => c.type === "text")
      .map((c) => c.text)
      .join("\n");
  }

  await request("initialize", {
    protocolVersion: "2024-11-05",
    capabilities: {},
    clientInfo: { name: "a029-t2-spike", version: "1.0" },
  });
  proc.stdin.write(
    JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized", params: {} }) + "\n"
  );

  console.error(`Indexing ${corpusDirPath} ...`);
  await callTool("index_folder", { path: corpusDirPath });

  for (const task of tasks) {
    const baselinePaths = baselineReferencePaths(corpusDirPath, task.symbol, task.repo);
    console.error(`Task ${task.id} (baseline files=${baselinePaths.length}) ...`);
    const text = await callTool("symforge", { query: task.query });
    const parsed = parseSymforgeOutput(text);
    const citedPaths = extractCitedPaths(text);
    const { matched, recall } = computeRecall(baselinePaths, citedPaths);
    const equiv =
      !parsed.chainFailed &&
      parsed.decision === "serve" &&
      parsed.tools_called.includes("find_references") &&
      baselinePaths.length > 0 &&
      recall >= task.min_baseline_recall
        ? "EQUIVALENT"
        : parsed.chainFailed || recall < task.min_baseline_recall
          ? "SYMFORGE-LESS"
          : "NOT_EQUIVALENT";

    results.push({
      id: task.id,
      repo: task.repo,
      query: task.query,
      symbol: task.symbol,
      decision: parsed.decision,
      tools_called: parsed.tools_called,
      equivalence: equiv,
      baseline_paths: baselinePaths.length,
      matched_paths: matched,
      baseline_recall: Number(recall.toFixed(4)),
      min_baseline_recall: task.min_baseline_recall,
      chain_failed: parsed.chainFailed,
      diagnostics:
        equiv === "EQUIVALENT"
          ? `recall=${(recall * 100).toFixed(1)}% matched ${matched}/${baselinePaths.length} rg files`
          : `recall=${(recall * 100).toFixed(1)}% need >=${(task.min_baseline_recall * 100).toFixed(0)}%; tools=${parsed.tools_called.join(",") || "none"}`,
    });
  }

  proc.kill();
  return results;
}

function evaluateVerdict(rows) {
  const equiv = rows.filter((r) => r.equivalence === "EQUIVALENT").length;
  let verdict = "KILL";
  let pivot_policy = null;
  if (equiv >= 2) verdict = "PASS";
  else if (rows.length >= 2) {
    verdict = "PIVOT";
    pivot_policy = "P-T2 bypass-only for reference tasks (grep envelope; eligible_h6=false)";
  }
  return { equiv, verdict, pivot_policy };
}

(async () => {
  const tasks = loadTasks();
  const skipped = [];
  const byRepo = new Map();
  for (const task of tasks) {
    const dir = corpusDir(task.repo);
    if (!fs.existsSync(dir)) {
      skipped.push(task.id);
      continue;
    }
    if (!byRepo.has(task.repo)) byRepo.set(task.repo, []);
    byRepo.get(task.repo).push(task);
  }

  if (skipped.length === tasks.length) {
    console.error(
      "No A-029 corpora found. Clone per tests/fixtures/a029-t2/README.md before running spike."
    );
    process.exit(2);
  }

  const allRows = [];
  for (const [repo, repoTasks] of byRepo.entries()) {
    const dir = corpusDir(repo);
    console.error(`A-029 corpus ${repo} (${repoTasks.length} tasks)`);
    const batch = await runRepoBatch(dir, repoTasks);
    allRows.push(...batch);
  }
  allRows.sort((a, b) => a.id.localeCompare(b.id));

  const { equiv, verdict, pivot_policy } = evaluateVerdict(allRows);
  const output = {
    measuredAt: new Date().toISOString(),
    surface: "compact",
    method: "symforge compact + rg baseline file recall",
    baselineCommit: baselineCommit(),
    rows: allRows,
    t2_equiv_pass: equiv,
    t2_tasks_total: allRows.length,
    verdict,
    pivot_policy,
    skippedTasks: skipped,
    notes:
      verdict === "PASS"
        ? "A-029 T2 spike PASS: >=2/4 reference tasks achieved sidecar-parity recall threshold."
        : verdict === "PIVOT"
          ? "A-029 T2 spike PIVOT: register P-T2 bypass-only policy for reference tasks; adjust H6 denominator."
          : "A-029 T2 spike KILL: insufficient evidence; expand T2 program before Phase 2 exit claim.",
  };

  fs.mkdirSync(path.dirname(OUT), { recursive: true });
  fs.writeFileSync(OUT, JSON.stringify(output, null, 2) + "\n");
  console.log(JSON.stringify({ verdict, t2_equiv_pass: equiv, t2_tasks_total: allRows.length }, null, 2));
  console.error(`Wrote ${OUT}`);
  process.exit(verdict === "PASS" ? 0 : 1);
})();
