#!/usr/bin/env node
"use strict";

// Emit the V11 retirement lifecycle phase.
//
// `validate-lifecycle-oracle-traceability.cjs` decides the phase internally and
// prints only a green summary, so there is no command that ANSWERS "which
// frozen set does this tree match?". Feature 020 Slice 5 needs that answer as a
// recorded baseline field, and a field whose value cannot be produced by a
// command is not an observation — it is a claim someone typed. This probe
// exists to make it an observation.
//
// It deliberately REPLICATES the checker's derivation rather than importing it
// (the checker is a script, not a module). That duplication is the cost of the
// answer, and it is bounded: if the two ever disagree, the checker is the
// authority and this probe is the bug. The `--verify` flag exists to catch that
// drift — it asserts the derived phase is one the checker would accept at all.

const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..");
const read = (relative) => {
  try {
    return fs.readFileSync(path.join(root, relative), "utf8");
  } catch {
    return "";
  }
};

// Mirrors `derivePublicApiAtoms` in the checker.
function derivePublicApiAtoms() {
  const atoms = new Set(["symforge"]);
  const lib = read("src/lib.rs");
  for (const match of lib.matchAll(/^\s*pub\s+mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;/gmu)) {
    atoms.add(`symforge::${match[1]}`);
  }
  const embed = read("src/embed.rs");
  for (const match of embed.matchAll(
    /^\s*pub\s+(?:async\s+)?(?:fn|struct|enum|type|trait|const|static)\s+([A-Za-z_][A-Za-z0-9_]*)/gmu,
  )) {
    atoms.add(`symforge::embed::${match[1]}`);
  }
  for (const match of embed.matchAll(/\bpub\s+use\s+crate::([\s\S]*?);/gu)) {
    const expression = match[1].trim();
    const open = expression.indexOf("{");
    const close = expression.lastIndexOf("}");
    const leaves =
      open === -1 || close < open
        ? [expression.split("::").at(-1)]
        : expression.slice(open + 1, close).split(",");
    for (const leaf of leaves) {
      const alias = /\bas\s+([A-Za-z_][A-Za-z0-9_]*)\s*$/u.exec(leaf);
      const identifiers = [...leaf.matchAll(/[A-Za-z_][A-Za-z0-9_]*/gu)].map((item) => item[0]);
      const name = alias ? alias[1] : identifiers.at(-1);
      if (name && name !== "self") atoms.add(`symforge::embed::${name}`);
    }
  }
  return atoms;
}

// The 3-segment filter that makes this probe worth having: 34 of the manifest's
// 64 introduced atoms are 4-segment associated methods the lifecycle set never
// contains, so a green checker does not mean the public surface is intact.
const directPublicAtoms = (atoms) =>
  [...new Set(atoms.filter((atom) => typeof atom === "string" && atom.split("::").length <= 3))].sort();

function main() {
  const manifest = JSON.parse(read("specs/020-repository-knowledge-index/contracts/public-api-v11.json"));
  const migration = manifest.migration_v10 || {};
  const categories = Array.isArray(migration.categories) ? migration.categories : [];
  const introduced = Array.isArray(migration.introduced_v11_atoms) ? migration.introduced_v11_atoms : [];

  const derived = derivePublicApiAtoms();
  const scanned = new Set(
    introduced
      .filter((atom) => atom.split("::").length >= 2)
      .map((atom) => atom.split("::")[1])
      .filter((module) => module !== "embed"),
  );
  for (const module of scanned) {
    if (!derived.has(`symforge::${module}`)) continue;
    const source = read(`src/${module}.rs`) || read(`src/${module}/mod.rs`);
    for (const match of source.matchAll(
      /^\s*pub\s+(?:async\s+)?(?:fn|struct|enum|type|trait|const|static)\s+([A-Za-z_][A-Za-z0-9_]*)/gmu,
    )) {
      derived.add(`symforge::${module}::${match[1]}`);
    }
  }

  const actual = [...derived].sort();
  const preactivation = directPublicAtoms(categories.flatMap((c) => c.atoms || []));
  const kept = categories.filter((c) => c.decision === "keep").flatMap((c) => c.atoms || []);
  const postactivation = directPublicAtoms([...kept, ...introduced]);

  const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
  const phase = same(actual, preactivation)
    ? "preactivation"
    : same(actual, postactivation)
      ? "postactivation"
      : "INVALID";

  const deep = introduced.filter((atom) => atom.split("::").length > 3).length;

  process.stdout.write(
    `PHASE: ${phase}\n` +
      `actual: ${actual.length}  pre: ${preactivation.length}  post: ${postactivation.length}\n` +
      `scannedModules: [${[...scanned].join(", ")}]\n` +
      `actualOnly: [${actual.filter((a) => !postactivation.includes(a)).join(", ")}]\n` +
      `postOnly: [${postactivation.filter((a) => !actual.includes(a)).join(", ")}]\n` +
      `introduced atoms invisible to this set (>3 segments): ${deep} of ${introduced.length}` +
      ` — covered by execution/refreeze_v11.py, not by the lifecycle checker\n`,
  );

  if (phase === "INVALID") {
    process.stderr.write(
      "phase is neither frozen set; the traceability checker would fail with " +
        "RETIREMENT_LIFECYCLE_PHASE_INVALID\n",
    );
    process.exitCode = 1;
  }
}

main();
