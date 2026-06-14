#!/usr/bin/env node
/**
 * T2.1 rg-baseline inventory for A-029 T2 tasks (8.1 index-recall audit).
 * Docs/evidence only — writes JSON under docs/research/rg-hits/.
 *
 * Usage:
 *   node scripts/a029-t21-rg-inventory.cjs [symforge-bin]
 *
 * With symforge-bin: indexes each corpus and captures cited paths (measurement only).
 */
const { spawn } = require("child_process");
const readline = require("readline");
const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");

const REPO_ROOT = path.resolve(__dirname, "..");
const TASKS = path.join(REPO_ROOT, "tests/fixtures/a029-t2/tasks.jsonl");
const FIXTURE_ROOT = path.join(REPO_ROOT, "tests/fixtures/a029-t2");
const OUT_DIR = path.join(REPO_ROOT, "docs/research/rg-hits");
const BIN = process.argv[2]
  ? path.resolve(process.argv[2])
  : path.join(
      REPO_ROOT,
      "target/debug",
      process.platform === "win32" ? "symforge.exe" : "symforge"
    );
const RUN_SYMFORGE = fs.existsSync(BIN);

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

function corpusSha(corpusDirPath) {
  try {
    return execSync("git rev-parse HEAD", { cwd: corpusDirPath, encoding: "utf8" }).trim();
  } catch {
    return "unknown";
  }
}

function baselineReferencePaths(corpusDirPath, symbol, repo) {
  const glob = repo === "django" ? "*.py" : "*.rs";
  try {
    const pattern = `\\b${symbol}\\b`;
    const out = execSync(
      `rg -l --glob '${glob}' --glob '!target/**' --glob '!tests/fixtures/**' '${pattern}' .`,
      { cwd: corpusDirPath, encoding: "utf8", maxBuffer: 20 * 1024 * 1024 }
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

function categorizePath(p) {
  const norm = p.replace(/\\/g, "/");
  if (/\.md$/i.test(norm)) return "markdown";
  if (/(^|\/)benches?\//i.test(norm) || /_bench\.rs$/i.test(norm)) return "bench";
  if (/\/tests?\//i.test(norm) || /\/test_/i.test(norm) || /_test\.(rs|py)$/i.test(norm))
    return "test";
  if (/\/docs?\//i.test(norm)) return "docs";
  if (/\/examples?\//i.test(norm)) return "example";
  if (/\.toml$/i.test(norm) || /\.yaml$/i.test(norm) || /\.yml$/i.test(norm)) return "config";
  return "source";
}

function bucketCounts(paths) {
  const counts = {};
  for (const p of paths) {
    const b = categorizePath(p);
    counts[b] = (counts[b] || 0) + 1;
  }
  return counts;
}

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

function normalizeSet(paths) {
  return new Set(paths.map((p) => p.toLowerCase()));
}

function diffSets(baseline, cited) {
  const citedNorm = normalizeSet(cited);
  const missed = baseline.filter((p) => !citedNorm.has(p.toLowerCase()));
  const matched = baseline.filter((p) => citedNorm.has(p.toLowerCase()));
  return { missed, matched };
}

async function captureSymforge(corpusDirPath, query) {
  return new Promise((resolve, reject) => {
    let id = 1;
    const pending = new Map();
    let stdoutBuf = "";

    const proc = spawn(BIN, [], {
      cwd: corpusDirPath,
      stdio: ["pipe", "pipe", "ignore"],
      env: {
        ...process.env,
        RUST_LOG: "off",
        SYMFORGE_SURFACE: "compact",
        SYMFORGE_NO_DAEMON: "1",
      },
    });

    readline.createInterface({ input: proc.stdout }).on("line", (line) => {
      if (!line.trim()) return;
      let msg;
      try {
        msg = JSON.parse(line);
      } catch {
        stdoutBuf += line + "\n";
        return;
      }
      if (msg.id != null && pending.has(msg.id)) {
        const { resolve: res, reject: rej } = pending.get(msg.id);
        pending.delete(msg.id);
        if (msg.error) rej(new Error(JSON.stringify(msg.error)));
        else res(msg.result);
      }
    });

    proc.stdout.on("data", (chunk) => {
      stdoutBuf += chunk.toString();
    });

    function request(method, params) {
      return new Promise((res, rej) => {
        const myId = id++;
        pending.set(myId, { resolve: res, reject: rej });
        proc.stdin.write(JSON.stringify({ jsonrpc: "2.0", id: myId, method, params }) + "\n");
        setTimeout(() => {
          if (pending.has(myId)) {
            pending.delete(myId);
            rej(new Error(`timeout ${method}`));
          }
        }, 600000);
      });
    }

    async function run() {
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
        clientInfo: { name: "a029-t21-inventory", version: "1.0" },
      });
      proc.stdin.write(
        JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized", params: {} }) + "\n"
      );
      await callTool("index_folder", { path: corpusDirPath });
      const text = await callTool("symforge", { query });
      proc.stdin.end();
      resolve(text);
    }

    run().catch(reject);
    proc.on("error", reject);
  });
}

function outPathForTask(taskId) {
  const parts = taskId.split("/");
  const dir = path.join(OUT_DIR, ...parts.slice(0, -1));
  fs.mkdirSync(dir, { recursive: true });
  return path.join(dir, `${parts[parts.length - 1]}.json`);
}

async function main() {
  const tasks = loadTasks();
  const measuredAt = new Date().toISOString();
  const symforgeCommit = execSync("git rev-parse HEAD", { cwd: REPO_ROOT, encoding: "utf8" }).trim();

  fs.mkdirSync(OUT_DIR, { recursive: true });

  const summary = [];

  for (const task of tasks) {
    const dir = corpusDir(task.repo);
    if (!fs.existsSync(dir)) {
      console.error(`SKIP ${task.id}: missing corpus ${dir}`);
      continue;
    }

    const baselinePaths = baselineReferencePaths(dir, task.symbol, task.repo);
    let citedPaths = [];
    let symforgeOutput = null;

    if (RUN_SYMFORGE) {
      console.error(`Symforge measure ${task.id} ...`);
      try {
        symforgeOutput = await captureSymforge(dir, task.query);
        citedPaths = extractCitedPaths(symforgeOutput);
      } catch (e) {
        console.error(`  symforge failed: ${e.message}`);
      }
    }

    const { missed, matched } = diffSets(baselinePaths, citedPaths);
    const recall = baselinePaths.length ? matched.length / baselinePaths.length : 0;

    const payload = {
      task_id: task.id,
      repo: task.repo,
      query: task.query,
      symbol: task.symbol,
      min_baseline_recall: task.min_baseline_recall,
      measuredAt,
      symforge_baseline_commit: symforgeCommit,
      corpus_sha: corpusSha(dir),
      method: {
        rg: "rg -l --glob '<lang>' --glob '!target/**' word-boundary symbol",
        symforge: RUN_SYMFORGE ? "compact symforge find_references (measurement only)" : "skipped (binary missing)",
      },
      baseline_paths_count: baselinePaths.length,
      cited_paths_count: citedPaths.length,
      matched_paths_count: matched.length,
      missed_paths_count: missed.length,
      baseline_recall: Math.round(recall * 10000) / 10000,
      baseline_bucket_counts: bucketCounts(baselinePaths),
      missed_bucket_counts: bucketCounts(missed),
      matched_bucket_counts: bucketCounts(matched),
      top_missed_prefixes: topPrefixes(missed, 15),
      baseline_paths: baselinePaths,
      cited_paths: citedPaths,
      matched_paths: matched,
      missed_paths: missed,
    };

    const outFile = outPathForTask(task.id);
    fs.writeFileSync(outFile, JSON.stringify(payload, null, 2) + "\n");
    console.error(`Wrote ${outFile} (baseline=${baselinePaths.length} recall=${(recall * 100).toFixed(1)}%)`);

    summary.push({
      task_id: task.id,
      baseline_paths: baselinePaths.length,
      cited_paths: citedPaths.length,
      matched_paths: matched.length,
      baseline_recall: payload.baseline_recall,
      missed_bucket_counts: payload.missed_bucket_counts,
      artifact: path.relative(REPO_ROOT, outFile),
    });
  }

  const summaryPath = path.join(REPO_ROOT, "docs/research/rg-hits/summary.json");
  fs.writeFileSync(
    summaryPath,
    JSON.stringify({ measuredAt, symforgeCommit, run_symforge: RUN_SYMFORGE, tasks: summary }, null, 2) + "\n"
  );
  console.error(`Wrote ${summaryPath}`);
}

function topPrefixes(paths, n) {
  const counts = {};
  for (const p of paths) {
    const parts = p.replace(/\\/g, "/").split("/");
    const prefix = parts.length > 1 ? parts.slice(0, 2).join("/") : parts[0];
    counts[prefix] = (counts[prefix] || 0) + 1;
  }
  return Object.entries(counts)
    .sort((a, b) => b[1] - a[1])
    .slice(0, n)
    .map(([prefix, count]) => ({ prefix, count }));
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
