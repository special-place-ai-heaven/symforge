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
 *   FAIL    snapshot regression on an exact-output tool, a required `must_contain` anchor is absent,
 *           or a directly called tool errored / went empty. Legacy reads relayed through the A-019
 *           compatibility path carry raw renderer text and no typed `isError`; oracle cases therefore
 *           require either the compact `Trust:` / expanded `Source authority:` success envelope or an
 *           exact case-declared empty-result prefix.
 *
 * Council guardrail (Hotz): this stays ONE script + data (cases.jsonl + *.snap).
 * No config system, no plugin seam. If it grows one, it has become the bug.
 *
 * KNOWN LIMITATIONS (honest, so a future session does not rediscover them):
 *  - The macro-body plain-call recovery in src/parsing/xref.rs uses a quote-parity
 *    string-literal guard, not a real lexer (precision-favoring). A call name after
 *    an odd number of earlier `"` chars in the file could be skipped. Bounded; see
 *    the `ponytail:` comment at that guard.
 *  - grep is a completeness TRIPWIRE, not a judge: it over-matches strings/comments
 *    vs symbol-aware tools, so an oracle diff is REVIEW (human looks), never FAIL.
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
// Binary: `--bin <path>` (CI/Linux, where it's target/release/symforge with no
// extension), else a bare *.exe arg (Windows convenience), else the local debug
// build. Resolved to ABSOLUTE — spawn runs with cwd=FIXTURE, so a relative --bin
// would resolve against the wrong dir. Cross-platform for ubuntu CI.
const BIN_ARG =
  (ARGV.includes("--bin") && ARGV[ARGV.indexOf("--bin") + 1]) ||
  ARGV.find((a) => a.endsWith(".exe")) ||
  path.join(REPO_ROOT, process.platform === "win32" ? "target/debug/symforge.exe" : "target/debug/symforge");
const BIN = path.resolve(REPO_ROOT, BIN_ARG);

// Legacy tools reached via the compact `symforge` facade's deterministic,
// source-mutation-safe measurement relay (_probe_legacy_tool/_probe_legacy_args)
// so we test the TOOL, not the NL router. The compatibility relay preserves raw
// renderer text and deliberately does not fabricate a semantic result status.
// batch_rename is full-surface-only but the relay permits only its dry-run case.
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

function normalizeOracleText(text) {
  return process.platform === "win32" ? text.replace(/\\/g, "/") : text;
}

function grepHitAnchor(hit) {
  const match = normalizeOracleText(hit).match(/^(.*?):(\d+):/);
  return match ? `${match[1]}:${match[2]}` : null;
}

function toolOutputAnchors(text) {
  const anchors = new Set();
  let groupedPath = null;
  for (const rawLine of normalizeOracleText(text).split("\n")) {
    const line = rawLine.trim();
    const fileHeader = line.match(/^([A-Za-z0-9_.-]+(?:\/[A-Za-z0-9_.-]+)*\.[A-Za-z0-9]+)$/);
    if (fileHeader) groupedPath = fileHeader[1];

    for (const match of line.matchAll(/(?:^|[`(\s])([A-Za-z0-9_.-]+(?:\/[A-Za-z0-9_.-]+)*\.[A-Za-z0-9]+):(\d+)(?=$|[`),:\s])/g)) {
      anchors.add(`${match[1]}:${match[2]}`);
    }
    const groupedLine = line.match(/^(?:>\s*)?(\d+):/);
    if (groupedPath && groupedLine) anchors.add(`${groupedPath}:${groupedLine[1]}`);
    const lineThenPath = line.match(/^(\d+):.*\(([A-Za-z0-9_.-]+(?:\/[A-Za-z0-9_.-]+)*\.[A-Za-z0-9]+)\)/);
    if (lineThenPath) anchors.add(`${lineThenPath[2]}:${lineThenPath[1]}`);
  }
  return anchors;
}

function judgeOracleResult(text, c, hits, grepMissing) {
  if (grepMissing) {
    return { verdict: "REVIEW", reason: "rg not on PATH — grep oracle skipped", text };
  }

  const envelopeLines = text.split("\n").slice(0, 6);
  const hasSuccessEnvelope = envelopeLines.some(
    (line) => line.startsWith("Trust: ") || line.startsWith("Source authority: ")
  );
  const hasDeclaredEmptyResult =
    typeof c.allow_empty === "string" && text.startsWith(c.allow_empty);
  if (!hasSuccessEnvelope && !hasDeclaredEmptyResult) {
    return {
      verdict: "FAIL",
      reason: "successful search terminal absent (expected envelope or declared empty result)",
      text,
      oracle: hits,
    };
  }
  if (hasDeclaredEmptyResult) {
    return {
      verdict: "PASS",
      reason: `declared empty result; grep saw ${hits.length} raw hit(s)`,
      text,
      oracle: hits,
    };
  }

  const anchors = [...new Set(hits.map(grepHitAnchor).filter(Boolean))];
  if (hits.length && !anchors.length) {
    return {
      verdict: "REVIEW",
      reason: `could not derive path:line anchors from ${hits.length} grep hit(s)`,
      text,
      oracle: hits,
    };
  }
  const observedAnchors = toolOutputAnchors(text);
  const missingAnchors = anchors.filter((anchor) => !observedAnchors.has(anchor));
  if (missingAnchors.length) {
    const sample = missingAnchors.slice(0, 3).join(", ");
    const more = missingAnchors.length > 3 ? ` (+${missingAnchors.length - 3} more)` : "";
    return {
      verdict: "REVIEW",
      reason: `tool omitted ${missingAnchors.length}/${anchors.length} grep path:line anchor(s): ${sample}${more}`,
      text,
      oracle: hits,
    };
  }

  return {
    verdict: "PASS",
    reason: `tool contains all ${anchors.length} grep path:line anchor(s)`,
    text,
    oracle: hits,
  };
}

function runOracleComparatorSelfTest() {
  const c = { allow_empty: "No references found" };
  const hits = ["src/a.rs:2:fn alpha() {}", "src/b.rs:5:fn beta() {}"];
  const complete = judgeOracleResult(
    "Trust: exact | current index\nsrc/a.rs\n  > 2: alpha\nsrc/b.rs\n  5: beta",
    {},
    hits,
    false
  );
  const omitted = judgeOracleResult(
    "Trust: exact | current index\nsrc/a.rs\n  > 2: alpha",
    {},
    hits,
    false
  );
  const declaredEmpty = judgeOracleResult("No references found for alpha", c, hits, false);
  if (complete.verdict !== "PASS" || omitted.verdict !== "REVIEW" || declaredEmpty.verdict !== "PASS") {
    throw new Error("grep path:line oracle comparator self-test failed");
  }
}

// Redact ONLY fields tied to LIVE index-generation state; everything else stays
// byte-exact so a real regression still FAILs (proven by the over-mask check).
//   - publication=/content=/generation=: monotonic per-publish counters.
//   - budget-limited (was: full): token-budget truncation race on the envelope.
//   - source=/content_hash=/link_id=: identity hashes that fold in the source
//     ROOT BINDING (the absolute repo path -> differs Windows `E:\` vs Linux CI
//     `/home/runner/...`) and, for links pointing INTO the snapshot files
//     themselves (these fixtures self-reference), content-generation with no
//     golden fixpoint. bytes=, the path:line target, counts, coverage,
//     resolution, bridge_index and the whole outline stay byte-exact and carry
//     the regression signal. (Snapshots are NOT rewritten to placeholders: the
//     stored value is redacted here on BOTH sides at compare time, and leaving
//     the file bytes unchanged preserves the self-referential byte-offset
//     fixpoint.)
function normalize(text) {
  return text
    .replace(/publication=\d+/g, "publication=<gen>")
    .replace(/content=\d+/g, "content=<gen>")
    .replace(/generation=\d+/g, "generation=<gen>")
    .replace(/source=[0-9a-f]+/g, "source=<src>")
    .replace(/content_hash=[0-9a-f]+/g, "content_hash=<hash>")
    .replace(/link_id=[0-9a-f]+/g, "link_id=<id>")
    // Truncation footer's pre-truncation size estimate: computed over raw output
    // that embeds the live counters above, so it tips (e.g. ~1010 vs ~1011) with
    // their digit width. The fixed "budget is 1000 tokens" claim stays byte-exact.
    .replace(/Original output is ~\d+ tokens/g, "Original output is ~<n> tokens")
    .replace(/budget-limited \(was: full\)/g, "full");
}

function readSnapshot(name) {
  const p = path.join(SNAP_DIR, name);
  return fs.existsSync(p) ? fs.readFileSync(p, "utf8") : null;
}
function writeSnapshot(name, text) {
  fs.writeFileSync(path.join(SNAP_DIR, name), text);
}

async function startSession(READINESS_PROBE) {
  const proc = spawn(BIN, [], {
    cwd: FIXTURE,
    // stderr INHERITED, not dropped: the daemon's own warnings are the evidence that
    // explains a readiness/cold-start failure. Safe — tracing writes to stderr
    // (src/observability.rs), and the JSON-RPC stream this harness parses is stdout-only.
    stdio: ["pipe", "pipe", "inherit"],
    // symforge_edit REQUIRES the compact surface; read tools use the raw A-019 relay.
    // Pin the root to the fixture (else find_project_root walks up to the parent
    // symforge repo and indexes 620 files, making every oracle mismatch). NO_DAEMON=1
    // keeps the index in-process so the pin holds.
    env: {
      ...process.env,
      // "warn", not "off": a silenced index-publish warning is precisely what made the
      // source-binding bug cost two days. Measured cost on a healthy run: one line.
      RUST_LOG: "warn",
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
  // Compact auto-indexes SYMFORGE_WORKSPACE_ROOT on startup (async), and the
  // PLACEHOLDER index it serves meanwhile can ALREADY report files: a targeted read
  // admits files into the EmptyBootstrap index before the initial load has bound a
  // source root. So `index_files > 0` opens the gate on an index with no source
  // identity — measured on this fixture, 4 of 12 cold starts reported
  // index_files>0 / index_state=Ready / load_source=EmptyBootstrap / generation=0 at
  // poll 0, with the search probe hitting (the targeted read admitted the very file
  // it needed). Gate on the per-call trust evidence in `_meta` instead: the real load
  // must have PUBLISHED (load_source != EmptyBootstrap) and the published state must
  // be Ready — then confirm a known symbol actually answers.
  // Gate on `_meta`, not the status body's `index_ready:`. Since the cold-start fix
  // `is_ready()` delegates to `index_state()`, so it is no longer DISHONEST here — but
  // it is still the weaker signal: it is a rendered string, it does not carry
  // `load_source`/`index_files`, and it is not what the daemon proxy forwards. The
  // `_meta` evidence carries all three from whichever process actually served.
  // ponytail: fixed 80-poll x 250ms ceiling (~20s of sleep); raise if a bigger fixture needs it.
  const EVIDENCE_KEY = "symforge/project_evidence";
  // Must match `IndexLoadSource::label()` (src/live_index/store.rs) exactly. That
  // method is the ONE spelling shared by this gate, the `health` runtime line, and
  // the `_meta` evidence. It used to be `format!("{:?}", ..)` here and snake_case in
  // `health`, i.e. two spellings of one value compared against each other.
  const EMPTY_BOOTSTRAP = "empty_bootstrap";
  let ready = false;
  let evidence = null;
  for (let i = 0; i < 80 && !ready; i++) {
    const status = await callTool("status", {});
    evidence = (status.result && status.result._meta && status.result._meta[EVIDENCE_KEY]) || null;
    const sourceBound =
      !!evidence &&
      evidence.index_state === "Ready" &&
      evidence.load_source !== EMPTY_BOOTSTRAP &&
      Number(evidence.index_files) > 0;
    if (sourceBound) {
      if (!READINESS_PROBE) {
        ready = true; // no known symbol to probe; a source-bound Ready generation is the signal
      } else {
        // Confirm the index actually answers before we trust it. Empty/error → wait.
        // `limit: 50` is search_symbols' own default, so the answer set is unchanged —
        // but the canonical-JSON fingerprint differs from every snapshot case, which
        // pass `{ query }` alone. Feature 032's repeat notice fires on the 3rd
        // identical (tool, arguments) serve in a session; a probe that repeated a
        // case's exact arguments could make that case the 3rd serve and append the
        // notice into a release-gate snapshot nondeterministically.
        const probe = await callTool("search_symbols", { query: READINESS_PROBE, limit: 50 });
        if (!probe.isError && probe.text.includes(READINESS_PROBE)) ready = true;
      }
    }
    if (!ready) await new Promise((r) => setTimeout(r, 250));
  }
  if (!ready) {
    // LOUD and specific. Never fall through to a snapshot comparison against a
    // half-loaded index: that renders as a content regression and sends the next
    // session hunting the wrong bug. Exit 2 = the harness could not run (same code as
    // a missing binary); exit 1 stays reserved for a REAL regression.
    console.error(
      "\n  HARNESS ABORT — the index never reached a source-bound Ready generation " +
        "(80 polls x 250ms, ~20s of sleep).\n" +
        `  last ${EVIDENCE_KEY}: ${JSON.stringify(evidence)}\n` +
        `  Wanted index_state="Ready" with load_source!="${EMPTY_BOOTSTRAP}" and index_files>0.\n` +
        `  load_source="${EMPTY_BOOTSTRAP}" => the real load never published (placeholder index).\n` +
        '  {"bound":false} => no workspace bound; check SYMFORGE_WORKSPACE_ROOT / the fixture path.\n' +
        "  null/absent evidence => the _meta key was NOT written at all. The server always writes\n" +
        "    it (unbound emits {\"bound\":false}, never omission), so this means the _meta contract\n" +
        "    changed; fix the gate, not the snapshots.\n"
    );
    proc.kill();
    process.exit(2);
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
  if (missing.length) {
    return { verdict: "FAIL", reason: `expected substrings absent: ${missing.join(", ")}`, text };
  }

  if (c.judge === "oracle") {
    const { hits, grepMissing } = grepOracle(c.oracle_grep);
    return judgeOracleResult(text, c, hits, grepMissing);
  }

  if (c.judge === "snapshot" || c.judge === "write_snapshot") {
    // For write cases, snapshot the RESULTING FILE (proves the write landed clean);
    // for read cases, snapshot the tool's text output. File already restored above.
    // normalize() redacts live index-generation counters before the compare AND
    // before --update writes, so committed goldens store the placeholder form.
    const captured = normalize(isWrite ? writtenContent : text);

    const snap = readSnapshot(c.snapshot);
    if (snap === null) {
      if (UPDATE) {
        writeSnapshot(c.snapshot, captured);
        return { verdict: "PASS", reason: `snapshot written (${c.snapshot})`, text: captured };
      }
      return { verdict: "REVIEW", reason: `no snapshot yet — run --update after eyeballing`, text: captured };
    }
    const snapN = normalize(snap);
    if (snapN === captured) return { verdict: "PASS", reason: "byte-identical to snapshot", text: captured };
    return { verdict: "FAIL", reason: `differs from snapshot ${c.snapshot}`, text: captured, snap: snapN };
  }

  return { verdict: "FAIL", reason: `unknown judge '${c.judge}'`, text };
}

// First N differing lines, not just one: a formatter change shifts a whole block and
// the single first line rarely says which. The trailing count carries the real signal:
// "5 of 137 differing" reads wholesale rewrite, a bare 1-line diff reads one value moved.
// ponytail: N=5 is the knee — raise it only if a real failure needed more.
const DIFF_LINES = 5;
function firstDiff(a, b) {
  const al = (a || "").split("\n");
  const bl = (b || "").split("\n");
  const shown = [];
  let differing = 0;
  for (let i = 0; i < Math.max(al.length, bl.length); i++) {
    if (al[i] === bl[i]) continue;
    differing++;
    if (shown.length < DIFF_LINES) {
      shown.push(`  L${i + 1}\n   expected: ${JSON.stringify(bl[i])}\n   actual:   ${JSON.stringify(al[i])}`);
    }
  }
  if (!differing) return "";
  const more =
    differing > shown.length ? `\n  ... ${shown.length} of ${differing} differing line(s) shown` : "";
  return shown.join("\n") + more;
}

(async () => {
  if (ARGV.includes("--self-test-oracle")) {
    runOracleComparatorSelfTest();
    console.log("verify-tools oracle comparator self-test: PASS");
    return;
  }
  if (!fs.existsSync(BIN)) {
    console.error(`No binary at ${BIN}. Build it: cargo build`);
    process.exit(2);
  }
  const cases = loadCases();
  // Readiness probe: reuse the first search_symbols case's query — a symbol we know
  // exists in this fixture — so the poll confirms the index actually answers. The
  // probe call itself carries an extra argument (see startSession) so its request
  // fingerprint never equals that case's.
  const probe = (cases.find((c) => c.tool === "search_symbols") || {}).args?.query || "";
  const session = await startSession(probe);
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
  if (tally.FAIL) console.log("  FAIL = a real regression: exact output/must-contain changed, a direct tool errored, or output went empty.\n");
  // The one number the council named: REVIEW + FAIL. Drive it down. Exit nonzero only on FAIL.
  process.exit(tally.FAIL > 0 ? 1 : 0);
})();
