#!/usr/bin/env node
// ledger-oracle.mjs - oracles for docs/research/fathom-embed-ledger-plan-aligned.
// Zero dependencies. Node 18+. Run from the repository root.
//
// Every mode performs all of its assertions first and prints its success
// marker ("LEDGER-ORACLE ... GREEN", "... CAUGHT", "... ADDITIVE") only when
// every one of them holds. Any failure prints "LEDGER-ORACLE RED: <why>" and
// exits nonzero. Cargo output is parsed, never echoed wholesale.
//
// Modes:
//   tests    --tests N --name T [--name T ...] [--echo TEXT] -- <cargo args>
//            exit 0, zero failures, EXACTLY N passing tests, every named test ok.
//   mutant   --patch FILE --name T [--name T ...] -- <cargo args>
//            apply FILE (a git patch removing one guard), run cargo, require every
//            named test to report FAILED, restore the touched files byte-for-byte.
//            Touched files must be committed and clean vs HEAD.
//   baseline --out FILE -- <cargo args>          record passed/failed names (not a gate)
//   baseline-head --baseline FILE --expect REF   the baseline was recorded at REF
//   suite    --baseline FILE -- <cargo args>     every baseline pass still passes; no new failure
//   run      --label L [--require TEXT ...] -- <program> <args>
//   api      --base REF --path P ... [--allow NAME ...] [--frozen NAME ...]
//            [--extend NAME=V1,V2 ...]
//            ADDITIVE ONLY: no public item line removed or changed since REF; an
//            added public item must be an --allow name or sit behind a test-only
//            cfg (__test-internals or test); re-exported names form a superset;
//            --frozen enums/structs are byte-identical; --extend NAME=V,...
//            allows only those new unit variants appended to NAME (original
//            variants stay, in order).
//   atoms    --base REF --contract FILE [--allow-prefix ATOM ...] [--require-atom ATOM ...]
//            every atom in migration_v10.introduced_v11_atoms at REF is still
//            present; every new atom equals or extends an --allow-prefix.
//   deps     --base REF [--allow-lock name@version ...]
//            no Cargo.lock package (other than the root crate) outside REF's set
//            except named --allow-lock rows; dependency and patch tables of
//            Cargo.toml unchanged. An --allow-lock row already at REF is RED.
//   doc-has  --file F --section HEADING --token T [--token T ...]
//   release  --since TAG --cr LABEL ... [--together yes] [--head-is-tag yes]
//            [--remote origin] [--handoff FILE [--handoff-section HEADING] --token T ...]
//            the lowest fetched v* tag newer than TAG containing every commit
//            whose subject names each LABEL; its CHANGELOG section names each
//            LABEL; with --together, no tag carries only part of those commits.
//   restore  put back files left mutated by an interrupted mutant run
//   self-test  prove this script's own RED paths (uses a temporary git repo)

import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync, mkdirSync, unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const DEFAULT_TIMEOUT_SECS = 9000; // below the 10800 s gate-check timeout, so restore always runs
const TAIL_LINES = 40;

class Red extends Error {}
const red = (message) => { throw new Red(message); };

// ---------- argument parsing ----------

export function parseArgs(argv) {
  const sep = argv.indexOf("--");
  const head = sep === -1 ? argv : argv.slice(0, sep);
  const rest = sep === -1 ? [] : argv.slice(sep + 1);
  const opts = {};
  for (let i = 1; i < head.length; i += 2) {
    const key = head[i];
    const value = head[i + 1];
    if (!key.startsWith("--") || value === undefined) red("bad option list near " + key);
    (opts[key.slice(2)] ||= []).push(value);
  }
  return { mode: head[0], opts, rest };
}

const one = (opts, key) => {
  const values = opts[key] || [];
  if (values.length !== 1) red("expected exactly one --" + key);
  return values[0];
};

// ---------- processes ----------

function git(args, cwd, allowFail = false) {
  const r = spawnSync("git", args, { cwd, encoding: "utf8", maxBuffer: 256 * 1024 * 1024 });
  if (r.error) red("could not start git: " + r.error.message);
  if (!allowFail && r.status !== 0) red("git " + args.join(" ") + " exited " + r.status + ": " + (r.stderr || "").trim());
  return r;
}

function killTree(pid) {
  if (process.platform === "win32") {
    spawnSync(join(process.env.SystemRoot || "C:\\Windows", "System32", "taskkill.exe"),
      ["/PID", String(pid), "/T", "/F"], { stdio: "ignore" });
  } else {
    try { process.kill(-pid, "SIGKILL"); } catch { /* already gone */ }
  }
}

export function runCommand(program, args, cwd, timeoutSecs) {
  return new Promise((done) => {
    const lines = [];
    const child = spawn(program, args, {
      cwd, stdio: ["ignore", "pipe", "pipe"], windowsHide: true,
      detached: process.platform !== "win32",
    });
    const pending = { stdout: "", stderr: "" };
    const feed = (stream) => (chunk) => {
      const text = pending[stream] + chunk.toString("utf8");
      const parts = text.split(/\r?\n/);
      pending[stream] = parts.pop();
      lines.push(...parts);
    };
    child.stdout.on("data", feed("stdout"));
    child.stderr.on("data", feed("stderr"));
    let timedOut = false;
    const timer = setTimeout(() => { timedOut = true; killTree(child.pid); }, timeoutSecs * 1000);
    child.on("error", (error) => { clearTimeout(timer); done({ code: null, lines, error: error.message, timedOut }); });
    child.on("close", (code) => {
      clearTimeout(timer);
      for (const rest of [pending.stdout, pending.stderr]) if (rest) lines.push(rest);
      done({ code, lines, timedOut });
    });
  });
}

const tail = (lines) => lines.slice(-TAIL_LINES).join("\n");

// ---------- libtest parsing ----------

export function parseLibtest(lines) {
  let binary = "?";
  const tests = [];
  const results = [];
  const errors = [];
  for (const raw of lines) {
    const line = raw.replace(/\r$/, "");
    let m;
    if ((m = line.match(/^\s*Running (.+?) \(/))) { binary = m[1].replace(/\\/g, "/"); continue; }
    if ((m = line.match(/^test (\S+) \.\.\. (ok|FAILED|ignored)\b/))) {
      tests.push({ binary, name: m[1], status: m[2], key: binary + " :: " + m[1] });
      continue;
    }
    if ((m = line.match(/^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out/))) {
      results.push({ binary, passed: +m[2], failed: +m[3], ignored: +m[4], filtered: +m[6] });
      continue;
    }
    if (/^error(\[E\d+\])?: /.test(line)) errors.push(line);
  }
  return { tests, results, errors };
}

const matchesName = (test, name) => test.name === name || test.name.endsWith("::" + name);
const sum = (rows, key) => rows.reduce((total, row) => total + row[key], 0);

export function evaluateTests(run, expected, names) {
  if (run.error) red("could not start cargo: " + run.error);
  if (run.timedOut) red("cargo timed out");
  const parsed = parseLibtest(run.lines);
  if (!parsed.results.length) red("no test binary reported a result (build error or wrong target)\n" + (parsed.errors.slice(0, 10).join("\n") || tail(run.lines)));
  const failed = parsed.tests.filter((t) => t.status === "FAILED").map((t) => t.key);
  if (run.code !== 0 || sum(parsed.results, "failed") > 0) red("cargo exited " + run.code + "; failing tests: " + (failed.join(", ") || "(none parsed)") + "\n" + tail(run.lines));
  const passed = sum(parsed.results, "passed");
  const okNames = parsed.tests.filter((t) => t.status === "ok").map((t) => t.name);
  if (passed !== expected) red("expected exactly " + expected + " passing test(s), observed " + passed + ": " + okNames.join(", "));
  for (const name of names) {
    const hits = parsed.tests.filter((t) => matchesName(t, name));
    if (hits.length !== 1) red("named test " + name + " matched " + hits.length + " reported tests (need exactly 1)");
    if (hits[0].status !== "ok") red("named test " + name + " reported " + hits[0].status);
  }
  return parsed;
}

export function evaluateMutant(run, names) {
  if (run.error) red("could not start cargo: " + run.error);
  if (run.timedOut) red("cargo timed out under the mutation; a behavioural test must fail by its own deadline, not hang");
  const parsed = parseLibtest(run.lines);
  for (const name of names) {
    const hits = parsed.tests.filter((t) => matchesName(t, name));
    if (hits.length !== 1) red("under the mutation, named test " + name + " matched " + hits.length + " reported tests (a build error is not a caught mutant)\n" + (parsed.errors.slice(0, 10).join("\n") || tail(run.lines)));
    if (hits[0].status !== "FAILED") red("under the mutation, named test " + name + " reported " + hits[0].status + " instead of FAILED");
  }
  if (run.code === 0) red("cargo exited 0 under the mutation");
  return parsed;
}

export function evaluateSuite(run, baseline) {
  if (run.error) red("could not start cargo: " + run.error);
  if (run.timedOut) red("cargo timed out");
  const parsed = parseLibtest(run.lines);
  if (!parsed.results.length) red("no test binary reported a result\n" + (parsed.errors.slice(0, 10).join("\n") || tail(run.lines)));
  const passed = new Set(parsed.tests.filter((t) => t.status === "ok").map((t) => t.key));
  const failed = parsed.tests.filter((t) => t.status === "FAILED").map((t) => t.key);
  const baselineFailed = new Set(baseline.failed);
  const missing = baseline.passed.filter((key) => !passed.has(key));
  if (missing.length) red(missing.length + " test(s) that passed at the baseline did not pass now: " + missing.slice(0, 20).join(", "));
  const fresh = failed.filter((key) => !baselineFailed.has(key));
  if (fresh.length) red("new failing test(s): " + fresh.join(", "));
  if (run.code !== 0 && failed.length === 0) red("cargo exited " + run.code + " with no failing test parsed\n" + tail(run.lines));
  return { passed: passed.size, preexisting: failed.filter((key) => baselineFailed.has(key)) };
}

// ---------- mutation ----------

function sentinelPath(cwd) {
  return resolve(cwd, git(["rev-parse", "--git-path", "ledger-oracle-mutation.json"], cwd).stdout.trim());
}

function refuseIfMutated(cwd) {
  if (existsSync(sentinelPath(cwd))) red("a previous mutant run did not restore its files; run: node <this script> restore");
}

function restoreFrom(cwd, record) {
  for (const file of record.files) writeFileSync(resolve(cwd, file.path), Buffer.from(file.data, "base64"));
  for (const file of record.files) {
    if (!readFileSync(resolve(cwd, file.path)).equals(Buffer.from(file.data, "base64"))) return "bytes differ after restore: " + file.path;
    if (git(["diff", "--quiet", "HEAD", "--", file.path], cwd, true).status !== 0) return "not clean vs HEAD after restore: " + file.path;
  }
  return null;
}

export async function mutantCore({ cwd, patch, names, program, args, timeoutSecs }) {
  const patchPath = resolve(cwd, patch);
  if (!existsSync(patchPath)) red("patch not found: " + patch);
  refuseIfMutated(cwd);
  const paths = git(["apply", "--numstat", patchPath], cwd).stdout.split(/\r?\n/).filter(Boolean).map((l) => l.split("\t")[2]);
  if (!paths.length) red("patch touches no file: " + patch);
  for (const path of paths) {
    if (git(["ls-files", "--error-unmatch", "--", path], cwd, true).status !== 0) red("patch touches an untracked file: " + path);
    if (git(["diff", "--quiet", "HEAD", "--", path], cwd, true).status !== 0) red("commit the fix first; not clean vs HEAD: " + path);
  }
  if (git(["apply", "--check", patchPath], cwd, true).status !== 0) red("patch does not apply cleanly (the fix moved, or the mutation is already applied): " + patch);
  const record = { patch, files: paths.map((path) => ({ path, data: readFileSync(resolve(cwd, path)).toString("base64") })) };
  const sentinel = sentinelPath(cwd);
  writeFileSync(sentinel, JSON.stringify(record));
  const onSignal = () => { restoreFrom(cwd, record); try { unlinkSync(sentinel); } catch {} process.exit(130); };
  process.once("SIGINT", onSignal);
  process.once("SIGTERM", onSignal);
  let run;
  try {
    git(["apply", patchPath], cwd);
    run = await runCommand(program, args, cwd, timeoutSecs);
  } finally {
    const problem = restoreFrom(cwd, record);
    process.removeListener("SIGINT", onSignal);
    process.removeListener("SIGTERM", onSignal);
    if (problem) red("RESTORE FAILED, sentinel kept at " + sentinel + ": " + problem);
    unlinkSync(sentinel);
  }
  return evaluateMutant(run, names);
}

// ---------- additive-only public API ----------

const PUB_ITEM = /^\s*pub\s+(?:(?:unsafe|async|const|extern(?:\s+"[^"]*")?)\s+)*(?:fn|struct|enum|trait|type|const|static|mod|union)\s+([A-Za-z_][A-Za-z0-9_]*)/;
const PUB_FIELD = /^\s*pub\s+([a-z_][A-Za-z0-9_]*)\s*:/;
const TEST_DOOR = /__test-internals|cfg\((?:all\()?test\b/;

const pubName = (body) => { const m = body.match(PUB_ITEM) || body.match(PUB_FIELD); return m ? m[1] : null; };
const indentOf = (line) => line.match(/^\s*/)[0].length;

function attributesAboveHaveTestDoor(lines, index) {
  for (let i = index - 1; i >= 0; i--) {
    const t = lines[i].trim();
    if (t.startsWith("#[")) { if (TEST_DOOR.test(t)) return true; }
    else if (!t.startsWith("//")) return false;
  }
  return false;
}

// An item is test-only when it, or any item that encloses it, carries a
// test-only cfg attribute.
export function gatedByTestDoor(lines, at) {
  if (attributesAboveHaveTestDoor(lines, at)) return true;
  const indent = indentOf(lines[at]);
  if (indent === 0) return false;
  for (let i = at - 1; i >= 0; i--) {
    if (!lines[i].trim() || lines[i].trim().startsWith("//") || lines[i].trim().startsWith("#[")) continue;
    if (indentOf(lines[i]) < indent) return gatedByTestDoor(lines, i);
  }
  return false;
}

// The name of the struct, enum, trait or impl target that directly encloses
// line `at`, so fields and methods of an approved new item are approved with it.
export function enclosingItemName(lines, at) {
  const indent = indentOf(lines[at]);
  if (indent === 0) return null;
  for (let i = at - 1; i >= 0; i--) {
    const t = lines[i].trim();
    if (!t || t.startsWith("//") || t.startsWith("#[")) continue;
    if (indentOf(lines[i]) >= indent) continue;
    const m = t.match(/^(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum|trait|union)\s+([A-Za-z_][A-Za-z0-9_]*)/)
      || t.match(/^impl(?:<[^>]*>)?\s+(?:[A-Za-z_][A-Za-z0-9_:<>, ]*\s+for\s+)?([A-Za-z_][A-Za-z0-9_]*)/);
    return m ? m[1] : null;
  }
  return null;
}

export function reexportNames(text) {
  const names = new Set();
  for (const m of text.matchAll(/(^|\n)\s*pub\s+use\s+([^;]+);/g)) {
    const body = m[2].replace(/\s+/g, " ").trim();
    const brace = body.indexOf("{");
    const items = brace === -1 ? [body] : body.slice(brace + 1, body.lastIndexOf("}")).split(",");
    for (const item of items.map((s) => s.trim()).filter(Boolean)) {
      const alias = item.match(/\sas\s+([A-Za-z_][A-Za-z0-9_]*)$/);
      names.add(alias ? alias[1] : item.split("::").pop());
    }
  }
  return names;
}

export function publicChanges(diffText, headText, allow, extendNames = new Set()) {
  const headLines = headText.split(/\r?\n/);
  const added = [];
  const removed = [];
  let headLine = 0;
  for (const line of diffText.split(/\r?\n/)) {
    const hunk = line.match(/^@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
    if (hunk) { headLine = Number(hunk[1]); continue; }
    if (line.startsWith("+++ ") || line.startsWith("--- ")) continue;
    if (line.startsWith("+")) {
      const name = pubName(line.slice(1));
      if (name) added.push({ name, text: line.slice(1).trim(), at: headLine - 1 });
      headLine++;
    } else if (line.startsWith("-")) {
      const name = pubName(line.slice(1));
      if (name) removed.push({ name, text: line.slice(1).trim() });
    }
  }
  const violations = [];
  for (const gone of removed) {
    const moved = added.findIndex((a) => a.text === gone.text);
    if (moved !== -1) { added.splice(moved, 1); continue; }
    if (gone.name === "ALL") {
      const allAdded = added.findIndex((a) => a.name === "ALL" && extendNames.has(enclosingItemName(headLines, a.at)));
      if (allAdded !== -1) { added.splice(allAdded, 1); continue; }
    }
    violations.push("removed or changed: " + gone.text);
  }
  for (const item of added) {
    if (allow.has(item.name) || allow.has(enclosingItemName(headLines, item.at)) || gatedByTestDoor(headLines, item.at)) continue;
    violations.push("added outside the approved atoms: " + item.text);
  }
  return violations;
}

export function extractItem(text, name) {
  const m = new RegExp("pub\\s+(?:enum|struct|trait)\\s+" + name + "\\b").exec(text);
  if (!m) return null;
  const start = m.index;
  const before = text.slice(0, start).split("\n");
  before.pop();
  const attrs = [];
  for (let i = before.length - 1; i >= 0; i--) {
    const t = before[i].trim();
    if (t.startsWith("#[")) attrs.unshift(t);
    else if (!t.startsWith("///")) break;
  }
  const brace = text.indexOf("{", start);
  const semi = text.indexOf(";", start);
  if (brace === -1 || (semi !== -1 && semi < brace)) return attrs.join("\n") + "\n" + text.slice(start, semi + 1);
  let depth = 0;
  for (let i = brace; i < text.length; i++) {
    if (text[i] === "{") depth++;
    else if (text[i] === "}" && --depth === 0) return attrs.join("\n") + "\n" + text.slice(start, i + 1).replace(/\r\n/g, "\n");
  }
  return null;
}

export function enumUnitVariants(itemText) {
  const open = itemText.indexOf("{");
  const close = itemText.lastIndexOf("}");
  if (open === -1 || close === -1 || close <= open) return [];
  const names = [];
  for (const line of itemText.slice(open + 1, close).split(/\r?\n/)) {
    const m = line.match(/^\s*([A-Z][A-Za-z0-9]*)\s*,?\s*$/);
    if (m) names.push(m[1]);
  }
  return names;
}

export function enumExtendViolation(before, after, allowedNew) {
  const baseVars = enumUnitVariants(before);
  const headVars = enumUnitVariants(after);
  if (!baseVars.length) return "base item is not a unit enum";
  const expected = baseVars.concat(allowedNew);
  if (headVars.join(",") !== expected.join(",")) {
    return "variants must be " + expected.join(",") + " (got " + (headVars.join(",") || "<none>") + ")";
  }
  return null;
}

function apiCheck(cwd, opts) {
  const base = one(opts, "base");
  git(["rev-parse", "--verify", base + "^{commit}"], cwd);
  const allow = new Set(opts.allow || []);
  const extendNames = new Set((opts.extend || []).map((spec) => spec.split("=")[0]));
  const violations = [];
  for (const path of opts.path || []) {
    const baseText = git(["show", base + ":" + path], cwd).stdout;
    const headText = existsSync(resolve(cwd, path)) ? readFileSync(resolve(cwd, path), "utf8") : "";
    for (const v of publicChanges(git(["diff", "--no-color", "-U0", base, "--", path], cwd).stdout, headText, allow, extendNames)) violations.push(path + " " + v);
    const headNames = reexportNames(headText);
    for (const name of reexportNames(baseText)) if (!headNames.has(name)) violations.push(path + " re-export removed: " + name);
    const baseNames = reexportNames(baseText);
    for (const name of headNames) if (!baseNames.has(name) && !allow.has(name)) violations.push(path + " re-export added outside the approved atoms: " + name);
  }
  for (const name of opts.frozen || []) {
    let found = false;
    for (const path of opts.path || []) {
      const baseText = git(["show", base + ":" + path], cwd, true);
      if (baseText.status !== 0) continue;
      const before = extractItem(baseText.stdout, name);
      if (!before) continue;
      found = true;
      const after = existsSync(resolve(cwd, path)) ? extractItem(readFileSync(resolve(cwd, path), "utf8"), name) : null;
      if (after !== before.replace(/\r\n/g, "\n")) violations.push("frozen public item changed: " + name + " in " + path);
    }
    if (!found) red("frozen item " + name + " not found at " + base + " in the --path files");
  }
  for (const spec of opts.extend || []) {
    const eq = spec.indexOf("=");
    if (eq < 1 || eq === spec.length - 1) red("bad --extend (want NAME=V1,V2): " + spec);
    const name = spec.slice(0, eq);
    const allowed = spec.slice(eq + 1).split(",").filter(Boolean);
    if (!allowed.length) red("bad --extend (empty variant list): " + spec);
    let found = false;
    for (const path of opts.path || []) {
      const baseText = git(["show", base + ":" + path], cwd, true);
      if (baseText.status !== 0) continue;
      const before = extractItem(baseText.stdout, name);
      if (!before) continue;
      found = true;
      const after = existsSync(resolve(cwd, path)) ? extractItem(readFileSync(resolve(cwd, path), "utf8"), name) : null;
      if (!after) {
        violations.push("extended public item missing: " + name + " in " + path);
        continue;
      }
      const reason = enumExtendViolation(before, after, allowed);
      if (reason) violations.push("extended public item " + name + " in " + path + ": " + reason);
    }
    if (!found) red("extend item " + name + " not found at " + base + " in the --path files");
  }
  if (violations.length) red("public API is not additive-only:\n  " + violations.join("\n  "));
  return base;
}

export function atomChanges(baseAtoms, headAtoms, allowPrefixes) {
  const head = new Set(headAtoms);
  const base = new Set(baseAtoms);
  return {
    missing: baseAtoms.filter((a) => !head.has(a)),
    added: headAtoms.filter((a) => !base.has(a)),
    unapproved: headAtoms.filter((a) => !base.has(a) && !allowPrefixes.some((p) => a === p || a.startsWith(p + "::"))),
  };
}

export const requiredAtomsAbsent = (atoms, required) => required.filter((atom) => !atoms.includes(atom));

const atomsOf = (text) => {
  const atoms = JSON.parse(text)?.migration_v10?.introduced_v11_atoms;
  if (!Array.isArray(atoms) || !atoms.length) red("contract has no migration_v10.introduced_v11_atoms array");
  return atoms;
};

// ---------- dependencies ----------

export function lockPackages(text, rootName) {
  const out = new Set();
  for (const m of text.replace(/\r\n/g, "\n").matchAll(/\[\[package\]\]\nname = "([^"]+)"\nversion = "([^"]+)"/g)) {
    if (m[1] !== rootName) out.add(m[1] + "@" + m[2]);
  }
  return out;
}

export function dependencyTables(tomlText) {
  const kept = [];
  let keep = false;
  for (const line of tomlText.replace(/\r\n/g, "\n").split("\n")) {
    if (/^\s*\[/.test(line)) keep = /^\s*\[(?:target\..+\.)?(?:dependencies|dev-dependencies|build-dependencies)(?:\..+)?\]\s*$|^\s*\[patch\..+\]\s*$/.test(line);
    if (keep) kept.push(line.trimEnd());
  }
  return kept.join("\n");
}

function depsCheck(cwd, opts) {
  const base = one(opts, "base");
  const allowLock = new Set(opts["allow-lock"] || []);
  const baseToml = git(["show", base + ":Cargo.toml"], cwd).stdout;
  const headToml = readFileSync(resolve(cwd, "Cargo.toml"), "utf8");
  const root = (baseToml.match(/^\[package\][\s\S]*?^name\s*=\s*"([^"]+)"/m) || red("no [package] name in Cargo.toml"))[1];
  const violations = [];
  if (dependencyTables(baseToml) !== dependencyTables(headToml)) violations.push("Cargo.toml dependency or patch tables changed");
  const baseLock = lockPackages(git(["show", base + ":Cargo.lock"], cwd).stdout, root);
  if (!baseLock.size) red("no packages parsed from Cargo.lock at " + base);
  for (const pkg of lockPackages(readFileSync(resolve(cwd, "Cargo.lock"), "utf8"), root)) {
    if (!baseLock.has(pkg) && !allowLock.has(pkg)) violations.push("Cargo.lock package not present at " + base + ": " + pkg);
  }
  for (const pkg of allowLock) if (baseLock.has(pkg)) red("--allow-lock " + pkg + " is already present at " + base);
  if (violations.length) red("dependency set changed:\n  " + violations.join("\n  "));
  return { base, packages: baseLock.size };
}

// ---------- docs and release ----------

export function sectionOf(text, heading) {
  const lines = text.split(/\r?\n/);
  const at = lines.findIndex((l) => l.startsWith(heading));
  if (at === -1) return null;
  const level = (heading.match(/^#+/) || ["##"])[0].length;
  const end = lines.findIndex((l, i) => i > at && new RegExp("^#{1," + level + "} ").test(l));
  return lines.slice(at, end === -1 ? lines.length : end).join("\n");
}

export const labelPattern = (label) => new RegExp("(?<![A-Za-z0-9-])" + label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") + "(?![0-9])");

export function requireLabelsInChangelog(changelog, tag, labels) {
  const section = sectionOf(changelog, "## [" + tag.slice(1) + "]");
  if (!section) red("CHANGELOG.md at " + tag + " has no ## [" + tag.slice(1) + "] section");
  for (const label of labels) if (!labelPattern(label).test(section)) red("CHANGELOG section for " + tag + " does not name " + label);
}

// candidates ascend by version; contains(commit, tag) answers ancestry.
export function pickReleaseTag(candidates, commits, contains, together) {
  if (together) {
    for (const tag of candidates) {
      const carried = commits.filter((c) => contains(c, tag)).length;
      if (carried && carried !== commits.length) red("tag " + tag + " carries only part of the delivered change set; the change requests must ship in one release");
    }
  }
  return candidates.find((tag) => commits.every((c) => contains(c, tag))) || null;
}

const semver = (tag) => tag.slice(1).split(".").map(Number);
const newer = (a, b) => { const x = semver(a), y = semver(b); for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] > y[i]; return false; };

function releaseCheck(cwd, opts) {
  const since = one(opts, "since");
  const remote = (opts.remote || ["origin"])[0];
  const labels = opts.cr || red("name at least one --cr");
  const remoteMain = git(["ls-remote", remote, "refs/heads/main"], cwd).stdout.split("\t")[0];
  const localMain = git(["rev-parse", "refs/remotes/" + remote + "/main"], cwd).stdout.trim();
  if (!remoteMain || remoteMain !== localMain) red("refs/remotes/" + remote + "/main is stale; run git fetch --tags " + remote);
  const log = git(["log", "--format=%H%x09%s", since + ".." + localMain], cwd).stdout.split(/\r?\n/).filter(Boolean).map((l) => l.split("\t"));
  const commits = [...new Set(labels.flatMap((label) => {
    const hits = log.filter(([, subject]) => labelPattern(label).test(subject));
    if (!hits.length) red("no commit on " + remote + "/main since " + since + " has a subject naming " + label);
    return hits.map(([sha]) => sha);
  }))];
  const tags = new Map();
  for (const line of git(["ls-remote", "--tags", remote], cwd).stdout.split(/\r?\n/).filter(Boolean)) {
    const [sha, ref] = line.split("\t");
    const m = ref.match(/^refs\/tags\/(v\d+\.\d+\.\d+)(\^\{\})?$/);
    if (m && (m[2] || !tags.has(m[1]))) tags.set(m[1], sha);
  }
  const candidates = [...tags.keys()].filter((t) => newer(t, since)).sort((a, b) => (newer(a, b) ? 1 : -1))
    .filter((tag) => { const local = git(["rev-parse", "--verify", "refs/tags/" + tag + "^{commit}"], cwd, true); return local.status === 0 && local.stdout.trim() === tags.get(tag); });
  const contains = (commit, tag) => git(["merge-base", "--is-ancestor", commit, tag], cwd, true).status === 0;
  const tag = pickReleaseTag(candidates, commits, contains, (opts.together || [])[0] === "yes");
  if (!tag) red("no fetched release tag newer than " + since + " on " + remote + " contains every delivered change request");
  requireLabelsInChangelog(git(["show", tag + ":CHANGELOG.md"], cwd).stdout, tag, labels);
  if ((opts["head-is-tag"] || [])[0] === "yes" && git(["rev-parse", "HEAD"], cwd).stdout.trim() !== tags.get(tag)) red("HEAD is not the release tag " + tag);
  if (opts.handoff) {
    requireHandoff(readFileSync(resolve(cwd, one(opts, "handoff")), "utf8"), (opts["handoff-section"] || [null])[0], [tag, ...(opts.token || [])]);
  }
  return { tag, labels };
}

// The handoff record must carry every token inside the named section, so older
// campaigns in the same file cannot satisfy it.
export function requireHandoff(text, heading, tokens) {
  const scope = heading ? sectionOf(text, heading) : text;
  if (scope === null) red("handoff record has no section starting with " + heading);
  for (const token of tokens) if (!scope.includes(token)) red("handoff record lacks: " + token);
}

// ---------- self-test ----------

async function selfTest() {
  let checks = 0;
  const expectRed = (fn, why, reason = "") => { try { fn(); } catch (e) { if (e instanceof Red && e.message.includes(reason)) { checks++; return; } throw e; } throw new Error("self-test: expected RED for " + why); };
  const expectRedAsync = async (fn, why, reason = "") => { try { await fn(); } catch (e) { if (e instanceof Red && e.message.includes(reason)) { checks++; return; } throw e; } throw new Error("self-test: expected RED for " + why); };
  const ok = (fn) => { fn(); checks++; };
  const need = (cond, what) => { if (!cond) throw new Error("self-test: " + what); };
  const run = (text, code) => ({ code, lines: text.split("\n"), timedOut: false });
  const lib = "     Running tests/embed_bound_index.rs (target/debug/deps/embed_bound_index-1.exe)\n";

  expectRed(() => evaluateTests(run(lib + "running 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out", 0), 1, ["a"]), "a filter that matched nothing");
  const one1 = run(lib + "running 1 test\ntest a ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out", 0);
  ok(() => evaluateTests(one1, 1, ["a"]));
  expectRed(() => evaluateTests(one1, 1, ["b"]), "the wrong test name");
  expectRed(() => evaluateTests(one1, 2, ["a"]), "a count mismatch");
  expectRed(() => evaluateTests(run(lib + "test a ... ok\ntest a_extra ... ok\ntest result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out", 0), 1, ["a"]), "extra matched tests");
  expectRed(() => evaluateTests(run(lib + "test a ... ignored\ntest b ... ok\ntest result: ok. 1 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out", 0), 1, ["a"]), "an ignored named test");
  const failedRun = run(lib + "test a ... FAILED\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out", 101);
  expectRed(() => evaluateTests(failedRun, 0, []), "a failing run");
  ok(() => evaluateMutant(failedRun, ["a"]));
  expectRed(() => evaluateMutant(one1, ["a"]), "a surviving mutant");
  expectRed(() => evaluateMutant(run(lib + "test a ... ok\ntest b ... FAILED\ntest result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out", 101), ["a"]), "a mutant caught only by another test");
  expectRed(() => evaluateMutant(run("error[E0425]: nope", 101), ["a"]), "a build error posing as a caught mutant");
  expectRed(() => evaluateMutant(run(lib + "test a ... FAILED", 0), ["a"]), "a FAILED line with a zero exit");

  const base = { passed: ["B :: t::a", "B :: t::b"], failed: ["B :: t::old"] };
  const B = "     Running B (x)\n";
  ok(() => need(evaluateSuite(run(B + "test t::a ... ok\ntest t::b ... ok\ntest t::old ... FAILED\ntest result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out", 101), base).preexisting.length === 1, "preexisting count"));
  expectRed(() => evaluateSuite(run(B + "test t::a ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out", 0), base), "a vanished baseline test");
  expectRed(() => evaluateSuite(run(B + "test t::a ... ok\ntest t::b ... ok\ntest t::new ... FAILED\ntest result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out", 101), base), "a new failing test");

  const head = [
    "pub struct Keep;",
    "#[cfg(feature = \"__test-internals\")]",
    "pub fn hold_for_test() {}",
    "#[cfg(feature = \"__test-internals\")]",
    "impl Keep {",
    "    pub fn wait_for_test(&self) {}",
    "}",
    "impl Keep {",
    "    pub fn index_census(&self) {}",
    "    pub fn stray(&self) {}",
    "}",
  ].join("\n");
  const diff = "--- a/x.rs\n+++ b/x.rs\n@@ -1,0 +2,10 @@\n" + head.split("\n").slice(1).map((l) => "+" + l).join("\n");
  const found = publicChanges(diff, head, new Set(["index_census"]));
  ok(() => need(found.length === 1 && found[0].includes("pub fn stray"), "additive scan: " + JSON.stringify(found)));
  const newType = ["pub struct KnowledgeSearchResult {", "    pub total: u64,", "    pub hits: Vec<u8>,", "}", "pub struct SymbolSearchRequest {", "    pub extra: bool,", "}"].join("\n");
  const newTypeDiff = "@@ -0,0 +1,4 @@\n+pub struct KnowledgeSearchResult {\n+    pub total: u64,\n+    pub hits: Vec<u8>,\n+}\n@@ -1,0 +6 @@\n+    pub extra: bool,";
  const fieldFindings = publicChanges(newTypeDiff, newType, new Set(["KnowledgeSearchResult"]));
  ok(() => need(fieldFindings.length === 1 && fieldFindings[0].includes("pub extra"), "fields of approved new items pass, fields added to existing items fail: " + JSON.stringify(fieldFindings)));
  ok(() => need(enclosingItemName(["impl<T> Display for IndexCensus {", "    pub fn x() {}"], 1) === "IndexCensus", "impl target name"));
  ok(() => need(publicChanges("@@ -3 +3 @@\n-    pub const ALL: [Self; 7] = [\n+    pub const ALL: [Self; 8] = [", "a\nb\n    pub const ALL: [Self; 8] = [", new Set()).some((v) => v.startsWith("removed or changed")), "changed signature flagged"));
  ok(() => need(publicChanges("@@ -9 +2 @@\n-pub fn moved() {}\n+pub fn moved() {}", "x\npub fn moved() {}", new Set()).length === 0, "moved identical line tolerated"));
  ok(() => need(publicChanges("@@ -0,0 +1 @@\n+pub(crate) fn inner() {}", "pub(crate) fn inner() {}", new Set()).length === 0, "crate-private ignored"));
  ok(() => need([...reexportNames("pub use a::b::{C as D, E};\npub use x::Y;")].sort().join(",") === "D,E,Y", "reexport names"));
  const enumText = "/// doc\n#[derive(Debug)]\npub enum Phase {\n    A,\n}\n";
  ok(() => need(extractItem(enumText, "Phase") !== extractItem(enumText.replace("    A,\n", "    A,\n    B,\n"), "Phase"), "variant change seen"));
  const atoms = atomChanges(["e::A", "e::H"], ["e::A", "e::H", "e::IndexCensus", "e::H::index_census", "e::Other"], ["e::IndexCensus", "e::H::index_census"]);
  ok(() => need(atoms.unapproved.join() === "e::Other" && atoms.missing.length === 0, "atom additions"));
  ok(() => need(atomChanges(["e::A", "e::B"], ["e::A"], []).missing.join() === "e::B", "atom removal"));
  ok(() => need(requiredAtomsAbsent(["e::A", "e::IndexCensus"], ["e::IndexCensus", "e::CensusFile"]).join() === "e::CensusFile", "required atom absent"));
  const lockA = "[[package]]\nname = \"symforge\"\nversion = \"11.2.0\"\n\n[[package]]\nname = \"anyhow\"\nversion = \"1.0.1\"\n";
  ok(() => need(lockPackages(lockA, "symforge").size === 1, "lock parse excludes root"));
  ok(() => need(lockPackages(lockA + "\n[[package]]\nname = \"newdep\"\nversion = \"0.1.0\"\n", "symforge").has("newdep@0.1.0"), "new lock package"));
  ok(() => need(dependencyTables("[package]\nversion = \"1\"\n[dependencies]\na = \"1\"\n[features]\nx = []") === dependencyTables("[package]\nversion = \"2\"\n[dependencies]\na = \"1\"\n[features]\nx = [\"y\"]"), "package version bump ignored"));
  ok(() => need(dependencyTables("[dependencies]\na = \"1\"") !== dependencyTables("[dependencies]\na = \"1\"\nb = \"2\""), "added dependency seen"));
  ok(() => need(labelPattern("CR-1").test("feat(embed): census (CR-1, CR-2)") && !labelPattern("CR-1").test("CR-10") && !labelPattern("F-1").test("XF-1"), "label pattern"));
  const changelog = "# Changelog\n## [11.3.0](x)\n* embed: CR-0 CR-1\n## [11.2.0](y)\n* CR-2 only here";
  ok(() => requireLabelsInChangelog(changelog, "v11.3.0", ["CR-0", "CR-1"]));
  expectRed(() => requireLabelsInChangelog(changelog, "v11.3.0", ["CR-2"]), "a label named only in an older section");
  const contains = (c, t) => ({ "v11.2.1": ["c0"], "v11.3.0": ["c0", "c1"] }[t] || []).includes(c);
  ok(() => need(pickReleaseTag(["v11.2.1", "v11.3.0"], ["c0", "c1"], contains, false) === "v11.3.0", "pick lowest complete tag"));
  expectRed(() => pickReleaseTag(["v11.2.1", "v11.3.0"], ["c0", "c1"], contains, true), "a partial release before the full one", "only part");
  ok(() => need(pickReleaseTag(["v11.2.1"], ["c0", "c1"], contains, false) === null, "no complete tag"));
  const todo = "# Fathom Embed Alignment\n## Review\nv11.3.0 released\n# Older campaign\nSYMFORGE_REV Cargo.lock";
  expectRed(() => requireHandoff(todo, "# Fathom Embed Alignment", ["v11.3.0", "SYMFORGE_REV"]), "a token present only in an older campaign", "lacks: SYMFORGE_REV");
  ok(() => requireHandoff(todo.replace("released", "released; bump SYMFORGE_REV"), "# Fathom Embed Alignment", ["v11.3.0", "SYMFORGE_REV"]));

  const repo = mkdtempSync(join(tmpdir(), "ledger-oracle-"));
  try {
    git(["init", "-q"], repo);
    git(["config", "user.email", "t@example.invalid"], repo);
    git(["config", "user.name", "t"], repo);
    git(["config", "core.autocrlf", "false"], repo);
    writeFileSync(join(repo, "guard.rs"), "fn guard() -> bool {\n    true\n}\n");
    writeFileSync(join(repo, "api.rs"), "pub use inner::{Keep, Other};\npub struct Keep;\n");
    git(["add", "."], repo);
    git(["commit", "-qm", "base"], repo);
    writeFileSync(join(repo, "guard.rs"), "fn guard() -> bool {\n    false\n}\n");
    writeFileSync(join(repo, "m.patch"), git(["diff"], repo).stdout);
    git(["checkout", "--", "guard.rs"], repo);
    const original = readFileSync(join(repo, "guard.rs"));
    const node = process.execPath;
    const catches = ["-e", "const f=require('fs').readFileSync('guard.rs','utf8');const bad=f.includes('false');console.log('test t::a ... '+(bad?'FAILED':'ok'));process.exit(bad?101:0)"];
    await mutantCore({ cwd: repo, patch: "m.patch", names: ["a"], program: node, args: catches, timeoutSecs: 60 });
    checks++;
    need(readFileSync(join(repo, "guard.rs")).equals(original), "mutant restore");
    await expectRedAsync(() => mutantCore({ cwd: repo, patch: "m.patch", names: ["a"], program: node, args: ["-e", "console.log('test t::a ... ok')"], timeoutSecs: 60 }), "a surviving mutant run");
    need(readFileSync(join(repo, "guard.rs")).equals(original), "surviving mutant restore");
    writeFileSync(join(repo, "guard.rs"), "fn guard() -> bool {\n    true\n}\n// edited\n");
    await expectRedAsync(() => mutantCore({ cwd: repo, patch: "m.patch", names: ["a"], program: node, args: catches, timeoutSecs: 60 }), "a dirty file under mutation", "commit the fix first");
    git(["checkout", "--", "guard.rs"], repo);
    ok(() => apiCheck(repo, { base: ["HEAD"], path: ["api.rs"], frozen: ["Keep"] }));
    writeFileSync(join(repo, "api.rs"), "pub use inner::{Keep, Other, IndexCensus};\npub struct Keep;\n");
    ok(() => apiCheck(repo, { base: ["HEAD"], path: ["api.rs"], allow: ["IndexCensus"] }));
    expectRed(() => apiCheck(repo, { base: ["HEAD"], path: ["api.rs"] }), "an unapproved re-export", "re-export added");
    writeFileSync(join(repo, "api.rs"), "pub use inner::{Keep};\npub struct Keep;\n");
    expectRed(() => apiCheck(repo, { base: ["HEAD"], path: ["api.rs"] }), "a removed re-export", "re-export removed: Other");
    writeFileSync(join(repo, "api.rs"), "pub use inner::{Keep, Other};\n#[non_exhaustive]\npub struct Keep;\n");
    expectRed(() => apiCheck(repo, { base: ["HEAD"], path: ["api.rs"], frozen: ["Keep"] }), "a changed frozen item", "frozen public item changed");
    writeFileSync(join(repo, "kind.rs"), "pub enum Kind {\n    One,\n    Two,\n}\n");
    git(["add", "kind.rs"], repo);
    git(["commit", "-qm", "kind"], repo);
    writeFileSync(join(repo, "kind.rs"), "pub enum Kind {\n    One,\n    Two,\n    Three,\n}\n");
    ok(() => apiCheck(repo, { base: ["HEAD"], path: ["kind.rs"], extend: ["Kind=Three"] }));
    writeFileSync(join(repo, "kind.rs"), "pub enum Kind {\n    One,\n    Three,\n    Two,\n}\n");
    expectRed(() => apiCheck(repo, { base: ["HEAD"], path: ["kind.rs"], extend: ["Kind=Three"] }), "a reordered extend enum", "variants must be");
    writeFileSync(join(repo, "kind.rs"), "pub enum Kind {\n    One,\n    Two,\n    Four,\n}\n");
    expectRed(() => apiCheck(repo, { base: ["HEAD"], path: ["kind.rs"], extend: ["Kind=Three"] }), "a wrong extend variant", "variants must be");
    git(["checkout", "--", "api.rs", "kind.rs"], repo);
    writeFileSync(join(repo, "Cargo.toml"), "[package]\nname = \"symforge\"\nversion = \"1\"\n[dependencies]\nanyhow = \"1\"\n");
    writeFileSync(join(repo, "Cargo.lock"), lockA);
    git(["add", "Cargo.toml", "Cargo.lock"], repo);
    git(["commit", "-qm", "lock"], repo);
    writeFileSync(join(repo, "Cargo.lock"), lockA + "\n[[package]]\nname = \"reqwest\"\nversion = \"0.13.5\"\n");
    ok(() => depsCheck(repo, { base: ["HEAD"], "allow-lock": ["reqwest@0.13.5"] }));
    expectRed(() => depsCheck(repo, { base: ["HEAD"] }), "an unapproved lock bump", "reqwest@0.13.5");
    expectRed(() => depsCheck(repo, { base: ["HEAD"], "allow-lock": ["anyhow@1.0.1"] }), "allow-lock of a base package", "already present");
    git(["checkout", "--", "Cargo.lock"], repo);
  } finally {
    rmSync(repo, { recursive: true, force: true });
  }
  return checks;
}

// ---------- main ----------

async function main() {
  const { mode, opts, rest } = parseArgs(process.argv.slice(2));
  const cwd = process.cwd();
  const timeoutSecs = Number((opts["timeout-secs"] || [DEFAULT_TIMEOUT_SECS])[0]);
  if (!Number.isInteger(timeoutSecs) || timeoutSecs < 1) red("--timeout-secs must be a positive integer");
  const needCargo = () => { if (!rest.length) red("pass cargo arguments after --"); refuseIfMutated(cwd); };
  switch (mode) {
    case "tests": {
      needCargo();
      const expected = Number(one(opts, "tests"));
      const names = opts.name || [];
      if (!Number.isInteger(expected) || expected < 1 || names.length === 0) red("tests mode needs --tests N >= 1 and at least one --name");
      const r = await runCommand("cargo", rest, cwd, timeoutSecs);
      evaluateTests(r, expected, names);
      for (const text of opts.echo || []) for (const line of r.lines.filter((l) => l.includes(text)).slice(0, 20)) console.log(line);
      console.log("LEDGER-ORACLE TESTS GREEN tests=" + expected + "; names=" + names.join(","));
      break;
    }
    case "mutant": {
      needCargo();
      const names = opts.name || red("mutant mode needs --name");
      await mutantCore({ cwd, patch: one(opts, "patch"), names, program: "cargo", args: rest, timeoutSecs });
      console.log("LEDGER-ORACLE MUTANT CAUGHT patch=" + basename(one(opts, "patch")) + "; names=" + names.join(","));
      break;
    }
    case "baseline": {
      needCargo();
      const out = resolve(cwd, one(opts, "out"));
      if (existsSync(out)) red("refusing to overwrite an existing baseline: " + out);
      if (git(["status", "--porcelain", "--untracked-files=no"], cwd).stdout.trim()) red("record the baseline on a clean checkout");
      const r = await runCommand("cargo", rest, cwd, timeoutSecs);
      if (r.error || r.timedOut) red("cargo did not complete: " + (r.error || "timed out"));
      const parsed = parseLibtest(r.lines);
      if (!parsed.results.length) red("no test binary reported a result\n" + tail(r.lines));
      const passed = parsed.tests.filter((t) => t.status === "ok").map((t) => t.key).sort();
      const failed = parsed.tests.filter((t) => t.status === "FAILED").map((t) => t.key).sort();
      if (!passed.length) red("baseline has zero passing tests");
      mkdirSync(dirname(out), { recursive: true });
      const headSha = git(["rev-parse", "HEAD"], cwd).stdout.trim();
      writeFileSync(out, JSON.stringify({ schema: 1, head: headSha, args: rest, passed, failed }, null, 1) + "\n");
      for (const key of failed) console.log("BASELINE FAILURE " + key);
      console.log("LEDGER-ORACLE BASELINE RECORDED head=" + headSha + " passed=" + passed.length + " failed=" + failed.length);
      break;
    }
    case "baseline-head": {
      const baseline = JSON.parse(readFileSync(resolve(cwd, one(opts, "baseline")), "utf8"));
      const expected = git(["rev-parse", "--verify", one(opts, "expect") + "^{commit}"], cwd).stdout.trim();
      if (baseline.head !== expected) red("baseline head " + baseline.head + " is not " + one(opts, "expect") + " (" + expected + ")");
      if (!Array.isArray(baseline.passed) || !baseline.passed.length) red("baseline records no passing test");
      console.log("LEDGER-ORACLE BASELINE GREEN at=" + one(opts, "expect") + " passed=" + baseline.passed.length + " failed=" + baseline.failed.length + ";");
      break;
    }
    case "suite": {
      needCargo();
      const baseline = JSON.parse(readFileSync(resolve(cwd, one(opts, "baseline")), "utf8"));
      if (JSON.stringify(baseline.args) !== JSON.stringify(rest)) red("baseline was recorded for a different cargo command: " + baseline.args.join(" "));
      if (git(["merge-base", "--is-ancestor", baseline.head, "HEAD"], cwd, true).status !== 0) red("baseline head " + baseline.head + " is not an ancestor of HEAD");
      const result = evaluateSuite(await runCommand("cargo", rest, cwd, timeoutSecs), baseline);
      for (const key of result.preexisting) console.log("PRE-EXISTING FAILURE (also failing at baseline) " + key);
      console.log("LEDGER-ORACLE SUITE GREEN passed=" + result.passed + " baseline_passed=" + baseline.passed.length + " preexisting_failures=" + result.preexisting.length + ";");
      break;
    }
    case "run": {
      if (!rest.length) red("pass the command after --");
      refuseIfMutated(cwd);
      const label = one(opts, "label");
      const r = await runCommand(rest[0], rest.slice(1), cwd, timeoutSecs);
      if (r.error || r.timedOut) red(label + " did not complete: " + (r.error || "timed out"));
      if (r.code !== 0) red(label + " exited " + r.code + "\n" + tail(r.lines));
      for (const text of opts.require || []) if (!r.lines.some((l) => l.includes(text))) red(label + " output lacks: " + text);
      console.log("LEDGER-ORACLE RUN GREEN label=" + label + ";");
      break;
    }
    case "api":
      console.log("LEDGER-ORACLE API ADDITIVE base=" + apiCheck(cwd, opts) + ";");
      break;
    case "atoms": {
      const base = one(opts, "base");
      const contract = one(opts, "contract");
      const change = atomChanges(atomsOf(git(["show", base + ":" + contract], cwd).stdout), atomsOf(readFileSync(resolve(cwd, contract), "utf8")), opts["allow-prefix"] || []);
      if (change.missing.length) red("contract atoms removed since " + base + ": " + change.missing.join(", "));
      if (change.unapproved.length) red("contract atoms added outside the approved list: " + change.unapproved.join(", "));
      const absent = requiredAtomsAbsent(atomsOf(readFileSync(resolve(cwd, contract), "utf8")), opts["require-atom"] || []);
      if (absent.length) red("required new atoms not recorded in the contract: " + absent.join(", "));
      console.log("LEDGER-ORACLE ATOMS ADDITIVE base=" + base + " added=" + change.added.length + ";");
      break;
    }
    case "deps": {
      const { base, packages } = depsCheck(cwd, opts);
      console.log("LEDGER-ORACLE DEPS UNCHANGED base=" + base + " packages=" + packages + ";");
      break;
    }
    case "doc-has": {
      const file = one(opts, "file");
      const section = sectionOf(readFileSync(resolve(cwd, file), "utf8"), one(opts, "section"));
      if (section === null) red(file + " has no section starting with " + one(opts, "section"));
      for (const token of opts.token || red("doc-has needs --token")) if (!section.includes(token)) red(file + " section lacks: " + token);
      console.log("LEDGER-ORACLE DOC GREEN file=" + file + ";");
      break;
    }
    case "release": {
      const { tag, labels } = releaseCheck(cwd, opts);
      console.log("LEDGER-ORACLE RELEASE GREEN tag=" + tag + " crs=" + labels.join(",") + ";");
      break;
    }
    case "restore": {
      const sentinel = sentinelPath(cwd);
      if (!existsSync(sentinel)) red("nothing to restore");
      const problem = restoreFrom(cwd, JSON.parse(readFileSync(sentinel, "utf8")));
      if (problem) red(problem);
      unlinkSync(sentinel);
      console.log("LEDGER-ORACLE RESTORED");
      break;
    }
    case "self-test":
      console.log("LEDGER-ORACLE SELF-TEST GREEN checks=" + (await selfTest()) + ";");
      break;
    default:
      red("unknown mode " + mode + "; read the header of this file");
  }
}

if (import.meta.url === pathToFileURL(process.argv[1] || "").href) {
  main().catch((error) => {
    console.log("LEDGER-ORACLE RED: " + (error instanceof Red ? error.message : error.stack));
    process.exit(error instanceof Red ? 1 : 2);
  });
}
