#!/usr/bin/env node
/** Validate docs/fixtures/routes.golden.jsonl per Phase 0 contract. */
const fs = require("fs");
const path = require("path");

const REPO_ROOT = path.resolve(__dirname, "..");
const GOLDEN = path.join(REPO_ROOT, "docs/fixtures/routes.golden.jsonl");
// TR-13 (010 FR-015): `expected_equiv` removed — it was write-only dead data
// (golden replay grades route SHAPE + L2 decision only, never equivalence), so
// requiring it implied a measurement that never ran. Equivalence is an offline
// bench signal (a029-t2), not something this route corpus grades.
const REQUIRED = [
  "id",
  "query",
  "must_call",
  "must_not_call",
  "expected_decision",
  "chain",
  "eligible_h6",
  "notes",
];

const raw = fs.readFileSync(GOLDEN, "utf8").trim();
const lines = raw ? raw.split("\n") : [];
const errors = [];
const ids = new Set();

if (lines.length !== 36) {
  errors.push(`line count ${lines.length} !== 36`);
}

lines.forEach((line, i) => {
  let obj;
  try {
    obj = JSON.parse(line);
  } catch (e) {
    errors.push(`line ${i + 1}: invalid JSON (${e.message})`);
    return;
  }
  for (const f of REQUIRED) {
    if (!(f in obj)) errors.push(`line ${i + 1} (${obj.id || "?"}): missing ${f}`);
  }
  if (obj.id) {
    if (ids.has(obj.id)) errors.push(`duplicate id ${obj.id}`);
    ids.add(obj.id);
  }
});

const pff = lines
  .map((l) => JSON.parse(l))
  .filter((r) => r.expected_decision === "bypass" && r.eligible_h6 === false);
if (pff.length < 4) {
  errors.push(`P-FF rows ${pff.length} < 4`);
}

const reviewed = lines
  .map((l) => JSON.parse(l))
  .filter((r) => /reviewed/i.test(r.notes || ""));
if (reviewed.length < 10) {
  errors.push(`human-reviewed notes ${reviewed.length} < 10 minimum`);
}

if (errors.length) {
  console.error("FAIL");
  errors.forEach((e) => console.error(" -", e));
  process.exit(1);
}

console.log("PASS", lines.length, "rows,", pff.length, "P-FF,", reviewed.length, "reviewed notes");
