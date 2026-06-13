#!/usr/bin/env node
/**
 * Seed docs/fixtures/routes.golden.jsonl — 36 Phase 0 golden route rows.
 * Schema: preflight-evidence-contract.md Golden Route Row
 */
const fs = require("fs");
const path = require("path");

const REPO_ROOT = path.resolve(__dirname, "..");
const OUT = path.join(REPO_ROOT, "docs/fixtures/routes.golden.jsonl");

function row(spec) {
  return {
    id: spec.id,
    query: spec.query,
    must_call: spec.must_call,
    must_not_call: spec.must_not_call || [],
    expected_decision: spec.expected_decision || "serve",
    expected_equiv: spec.expected_equiv !== false,
    chain: spec.chain || "single",
    eligible_h6: spec.eligible_h6 !== false,
    notes: spec.notes,
  };
}

const rows = [
  // cfg-if-rust (8)
  row({ id: "cfg-if/t1_search", query: "find cfg_if macro usage", must_call: ["search_text"], notes: "T1 text search; reviewed 2026-06-13" }),
  row({ id: "cfg-if/t2_context", query: "outline src/lib.rs", must_call: ["get_file_context"], notes: "T3-style outline; reviewed" }),
  row({ id: "cfg-if/t3_symbols", query: "locate cfg_if symbol", must_call: ["search_symbols"], notes: "symbol discovery" }),
  row({ id: "cfg-if/t4_refs", query: "who references cfg_if", must_call: ["find_references"], notes: "T2 reference trace; reviewed" }),
  row({ id: "cfg-if/t5_symbol", query: "body of cfg_if in lib.rs", must_call: ["get_symbol"], notes: "single symbol fetch" }),
  row({ id: "cfg-if/t6_map", query: "repo map cfg-if", must_call: ["get_repo_map"], notes: "repository outline" }),
  row({ id: "cfg-if/t7_content", query: "first 80 lines lib.rs", must_call: ["get_file_content"], notes: "bounded content read" }),
  row({ id: "cfg-if/t8_explore", query: "how does cfg_if work", must_call: ["explore"], notes: "guidance discovery" }),

  // records-python (8)
  row({ id: "records/t1_search", query: "find Database class", must_call: ["search_text"], notes: "reviewed" }),
  row({ id: "records/t2_context", query: "outline records.py", must_call: ["get_file_context"], notes: "reviewed" }),
  row({ id: "records/t3_files", query: "files named records", must_call: ["search_files"], notes: "path search" }),
  row({ id: "records/t4_refs", query: "references to Connection", must_call: ["find_references"], notes: "T2 refs" }),
  row({ id: "records/t5_symbol", query: "Database symbol in records.py", must_call: ["get_symbol"], notes: "symbol body" }),
  row({ id: "records/t6_dependents", query: "what depends on records.py", must_call: ["find_dependents"], notes: "reverse deps" }),
  row({ id: "records/t7_content", query: "read records.py header", must_call: ["get_file_content"], notes: "bounded read" }),
  row({ id: "records/t8_explore", query: "how to use records ORM", must_call: ["explore"], notes: "guidance" }),

  // is-plain-obj-ts (8)
  row({ id: "is-plain/t1_search", query: "find plainObject check", must_call: ["search_text"], notes: "reviewed" }),
  row({ id: "is-plain/t2_context", query: "outline index.js", must_call: ["get_file_context"], notes: "reviewed" }),
  row({ id: "is-plain/t3_content", query: "read index.js limit 80", must_call: ["get_file_content"], notes: "small bounded read" }),
  row({ id: "is-plain/t4_symbols", query: "isPlainObject symbol", must_call: ["search_symbols"], notes: "symbol search" }),
  row({ id: "is-plain/t5_symbol", query: "isPlainObject body", must_call: ["get_symbol"], notes: "symbol fetch" }),
  row({ id: "is-plain/t6_refs", query: "references isPlainObject", must_call: ["find_references"], notes: "refs" }),
  row({ id: "is-plain/t7_files", query: "test files for plain object", must_call: ["search_files"], notes: "path lookup" }),
  row({ id: "is-plain/t8_health", query: "index health", must_call: ["health_compact"], must_not_call: ["get_file_content"], notes: "runtime probe; reviewed" }),

  // compression_ratio/rust fixture (5)
  row({ id: "compression/t1_search", query: "find reconcile function", must_call: ["search_text"], notes: "in-repo fixture; reviewed" }),
  row({ id: "compression/t2_context", query: "outline service.rs", must_call: ["get_file_context"], notes: "reviewed" }),
  row({ id: "compression/t3_symbol", query: "reconcile symbol body", must_call: ["get_symbol"], notes: "symbol" }),
  row({ id: "compression/t4_refs", query: "references reconcile", must_call: ["find_references"], notes: "refs on fixture" }),
  row({ id: "compression/t5_dependents", query: "dependents of service.rs", must_call: ["find_dependents"], notes: "reverse deps" }),

  // P-FF full-file bypass (4) — policy P-FF
  row({
    id: "cfg-if/pff_whole_lib",
    query: "review entire lib.rs for security",
    must_call: [],
    expected_decision: "bypass",
    expected_equiv: false,
    eligible_h6: false,
    notes: "P-FF: whole-file review → bypass; reviewed",
  }),
  row({
    id: "records/pff_whole_module",
    query: "audit full records.py line by line",
    must_call: [],
    expected_decision: "bypass",
    expected_equiv: false,
    eligible_h6: false,
    notes: "P-FF bypass row",
  }),
  row({
    id: "is-plain/pff_whole_index",
    query: "read complete index.js for refactor",
    must_call: [],
    expected_decision: "bypass",
    expected_equiv: false,
    eligible_h6: false,
    notes: "P-FF bypass row",
  }),
  row({
    id: "compression/pff_whole_service",
    query: "full file review service.rs",
    must_call: [],
    expected_decision: "bypass",
    expected_equiv: false,
    eligible_h6: false,
    notes: "P-FF bypass row; reviewed",
  }),

  // Multi-chain (3) — H5 diversity
  row({
    id: "cfg-if/multi_search_symbol",
    query: "search then fetch cfg_if body",
    must_call: ["search_symbols", "get_symbol"],
    chain: "multi",
    notes: "multi-hop golden; Phase 2 replay",
  }),
  row({
    id: "records/multi_context_refs",
    query: "outline then find Connection refs",
    must_call: ["get_file_context", "find_references"],
    chain: "multi",
    notes: "multi-hop golden",
  }),
  row({
    id: "is-plain/multi_files_content",
    query: "find test.js then read it",
    must_call: ["search_files", "get_file_content"],
    chain: "multi",
    notes: "multi-hop golden; reviewed",
  }),
];

if (rows.length !== 36) {
  console.error("Expected 36 rows, got", rows.length);
  process.exit(1);
}

const ids = new Set();
for (const r of rows) {
  if (ids.has(r.id)) {
    console.error("duplicate id", r.id);
    process.exit(1);
  }
  ids.add(r.id);
}

fs.mkdirSync(path.dirname(OUT), { recursive: true });
const body = rows.map((r) => JSON.stringify(r)).join("\n") + "\n";
fs.writeFileSync(OUT, body);
console.log("Wrote", OUT, "rows", rows.length);
