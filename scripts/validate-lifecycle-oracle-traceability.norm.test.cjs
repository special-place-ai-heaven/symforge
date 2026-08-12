#!/usr/bin/env node
"use strict";

// Direct tests for the retirement census normalizer.
//
// The census answers one question: does this change alter the code the release
// build compiles? Everything it must SEE (production tokens, literal contents)
// and everything it must IGNORE (test-only items, comments, formatting) is a
// property of `normalizeRetirementClosureSource`, and the closure self-tests
// cannot check it: they mutate a fixture and assert the checker rejects, so any
// appended production text moves the digest whether the normalizer is right or
// wrong. A case like "a #[cfg(test)] struct field must not hide its production
// siblings" passes there for the wrong reason.
//
// So these tests call the normalizer on PAIRS of sources and assert only whether
// the two canonicalize the same. That isolates the property. It is how the
// over-stripping defect -- where a #[cfg(test)] field, enum variant, or match arm
// let the cut run through production siblings and out of the enclosing block --
// is kept fixed.

const fs = require("node:fs");
const path = require("node:path");

const checkerPath = path.resolve(__dirname, "validate-lifecycle-oracle-traceability.cjs");
const source = fs.readFileSync(checkerPath, "utf8");

function pick(name) {
  const match = source.match(new RegExp("function " + name + "[\\s\\S]*?\\n}\\n"));
  if (!match) throw new Error(`cannot extract ${name} from the checker`);
  return match[0];
}

const normalize = new Function(
  [
    "rustCharacterKinds",
    "maskRustCommentsAndLiterals",
    "cfgPredicateIsTestOnly",
    "rustAttributeAt",
    "stripCfgTestItems",
    "canonicalReleaseSource",
    "normalizeRetirementClosureSource",
  ]
    .map(pick)
    .join("") + "return normalizeRetirementClosureSource;",
)();

const BASE = [
  "pub fn keep(a: &str) -> usize {",
  '    let marker = "two  spaces";',
  "    marker.len() + a.len()",
  "}",
  "",
].join("\n");

const lines = (...parts) => parts.join("\n") + "\n";

// [name, left, right, mustCanonicalizeTheSame]
const CASES = [
  // --- invisible: test-only code, comments, formatting ---
  ["identity", BASE, BASE, true],
  ["cfg(test) module appended", BASE, BASE + lines("", "#[cfg(test)]", "mod t { #[test] fn p() {} }"), true],
  ["cfg(test) module with doc comment", BASE, BASE + lines("", "/// Doc.", "#[cfg(test)]", "mod t { fn p() {} }"), true],
  ["cfg(all(test, feature))", BASE, BASE + lines('#[cfg(all(test, feature = "server"))]', "pub(crate) use x::y;"), true],
  ["cfg(all(feature, test)) order swapped", BASE, BASE + lines('#[cfg(all(feature = "server", test))]', "pub(crate) use x::y;"), true],
  ["cfg(any(test, test))", BASE, BASE + lines("#[cfg(any(test, test))]", "fn p() {}"), true],
  ["nested all(any(test,test), feature)", BASE, BASE + lines('#[cfg(all(any(test, test), feature = "s"))]', "fn p() {}"), true],
  ["stacked cfg(test) + cfg(feature)", BASE, BASE + lines("#[cfg(test)]", '#[cfg(feature = "server")]', "pub(crate) use x::y;"), true],
  ["attribute run: derive then cfg(test)", BASE, BASE + lines("#[derive(Debug)]", "#[cfg(test)]", "struct X;"), true],
  ["cfg(test) statement inside a fn", BASE, BASE.replace("    marker.len()", "    #[cfg(test)]\n    probe::fire();\n    marker.len()"), true],
  ["comment reworded", BASE.replace("pub fn keep", "/// One.\npub fn keep"), BASE.replace("pub fn keep", "/// Two.\npub fn keep"), true],
  ["reformatted", BASE, BASE.replace("    marker", "\n\n        marker"), true],
  ["trailing newline removed", BASE, BASE.trimEnd(), true],

  // --- visible: anything the release build compiles ---
  ["production fn added", BASE, BASE + lines("pub fn extra() {}"), false],
  ["production token changed", BASE, BASE.replace("a.len()", "a.len() + 1"), false],
  ["string literal whitespace changed", BASE, BASE.replace("two  spaces", "two spaces"), false],
  ["string literal content changed", BASE, BASE.replace("two  spaces", "two  spacex"), false],
  ["comment turned into code", BASE, BASE.replace("    marker.len()", "    let z = 1; marker.len()"), false],
  ["cfg(test) removed from a real item", BASE + lines("#[cfg(test)]", "fn helper() {}"), BASE + lines("fn helper() {}"), false],
  ["identifier split by comment", "pub fn ab() {}\n", "pub fn a/*x*/b() {}\n", false],
  ["cfg(not(test))", BASE, BASE + lines("#[cfg(not(test))]", "fn p() {}"), false],
  ["cfg(any(test, feature))", BASE, BASE + lines('#[cfg(any(test, feature = "server"))]', "fn p() {}"), false],
  ["cfg(feature) alone", BASE, BASE + lines('#[cfg(feature = "server")]', "fn p() {}"), false],
  ["cfg(all(feature, unix))", BASE, BASE + lines('#[cfg(all(feature = "server", unix))]', "fn p() {}"), false],
  ["unrecognised predicate", BASE, BASE + lines("#[cfg(some_future_thing(test))]", "fn p() {}"), false],
];

// A `#[cfg(test)]` member ends at a comma, or at the enclosing brace when it is
// last -- not at `;` or `{`. A consumer that scans only for those two runs
// through production siblings and out of the block, hiding real code from the
// census. Each pair below differs ONLY in a production sibling.
const MEMBER_SHAPES = [
  [
    "struct field: production sibling",
    (member) => lines("pub struct S {", "    #[cfg(test)]", "    test_field: u8,", `    ${member}: u8,`, "}", "fn keep() {}"),
  ],
  [
    "struct field: cfg(test) member is last",
    (member) => lines("pub struct S {", `    ${member}: u8,`, "    #[cfg(test)]", "    test_field: u8", "}", "fn keep() {}"),
  ],
  [
    "enum variant: production sibling",
    (member) => lines("pub enum E {", "    #[cfg(test)]", "    TestOnly,", `    ${member},`, "}", "fn keep() {}"),
  ],
  [
    "match arm: production sibling",
    (member) => lines("pub fn f(x: u8) -> u8 {", "    match x {", "        #[cfg(test)]", "        0 => 1,", `        _ => ${member},`, "    }", "}", "fn keep() {}"),
  ],
  [
    "block-bodied match arm: production sibling",
    (member) => lines("pub fn f(x: u8) -> u8 {", "    match x {", "        #[cfg(test)]", "        0 => { 1 },", `        _ => ${member},`, "    }", "}", "fn keep() {}"),
  ],
  [
    "impl method: production sibling",
    (member) => lines("impl S {", "    #[cfg(test)]", "    fn test_only(&self) {}", `    fn ${member}(&self) {}`, "}", "fn keep() {}"),
  ],
];

for (const [name, build] of MEMBER_SHAPES) {
  CASES.push([`${name} must be visible`, build("alpha"), build("beta"), false]);
  // And the item that follows the body must survive the cut at all.
  CASES.push([`${name}: following item survives`, build("alpha"), build("alpha").replace("fn keep() {}", "fn keep() { let x = 1; }"), false]);
}

const failures = [];
for (const [name, left, right, sameExpected] of CASES) {
  let same;
  try {
    same = normalize(left) === normalize(right);
  } catch (error) {
    failures.push(`${name}: threw ${error && error.message ? error.message : "unknown"}`);
    continue;
  }
  if (same !== sameExpected) {
    failures.push(
      `${name}: expected the pair to canonicalize ${sameExpected ? "the same" : "differently"}, got ${same ? "the same" : "differently"}`,
    );
  }
}

if (failures.length > 0) {
  for (const failure of failures) process.stderr.write(`ERROR NORMALIZER: ${failure}\n`);
  process.exitCode = 1;
} else {
  process.stdout.write(`retirement census normalizer: OK (${CASES.length} equivalence cases)\n`);
}
