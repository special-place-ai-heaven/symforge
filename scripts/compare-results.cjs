#!/usr/bin/env node
/**
 * Phase 2 in-repo compare-results — computes H3/H4/H5 on battery JSON.
 * Formulas: docs/v8-gap-closure-plan.md §5.1 + A-012 serve-only H3 scope.
 *
 * Usage:
 *   node scripts/compare-results.cjs <candidate.json> [--baseline <baseline.json>] [--report <out.md>]
 *
 * Exit 0 when Phase 2 minimum gates (H3 + H4) PASS; H5 reported but non-blocking for exit code.
 */
const fs = require("fs");
const path = require("path");

const REPO_ROOT = path.resolve(__dirname, "..");

function loadJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

function isSmallFileTaskId(id) {
  return id.includes("_small") || id.endsWith("/small");
}

function normalizeResults(raw) {
  const rows = (raw.rows || []).map((row) => {
    const S = row.S ?? 0;
    const M = row.M ?? 0;
    const decision = row.decision || "serve";
    const equivalence = row.equivalence || "PENDING_REVIEW";
    const acceptedServe = decision === "serve" && equivalence === "EQUIVALENT";
    return {
      ...row,
      S,
      M,
      sGteM: S >= M,
      acceptedServe,
      goldenId: row.goldenId || row.id,
      chain: row.chain || "single",
      mcpCalls: row.mcpCalls ?? 1,
      eligibleH6: row.eligibleH6 ?? true,
    };
  });

  const session_net_accepted = rows
    .filter((r) => r.acceptedServe)
    .reduce((sum, r) => sum + (r.M - r.S), 0);
  const session_net_all36 = rows.reduce((sum, r) => sum + (r.M - r.S), 0);

  return {
    ...raw,
    rows,
    session_net_accepted,
    session_net_all36,
    rowCount: rows.length,
    skippedRows: raw.skippedRows || [],
  };
}

function h3ScopeRows(rows) {
  const serveAccepted = rows.filter((r) => r.decision === "serve" && r.acceptedServe);
  const small = serveAccepted.filter((r) => isSmallFileTaskId(r.id));
  return small.length > 0 ? small : serveAccepted;
}

function computeGates(results) {
  const h3Rows = h3ScopeRows(results.rows);
  const h3Violations = h3Rows.filter((r) => r.sGteM);
  const h5Violations = results.rows.filter((r) => r.chain === "single" && r.mcpCalls > 1);

  const gates = {
    H1: "NOT_CLAIMED",
    H2: "NOT_CLAIMED",
    H3: h3Violations.length === 0 ? "PASS" : "FAIL",
    H4: results.session_net_accepted >= 0 ? "PASS" : "FAIL",
    H5: h5Violations.length === 0 ? "PASS" : "FAIL",
    H6: "NOT_CLAIMED",
    H7: "NOT_CLAIMED",
    H8: "NOT_CLAIMED",
  };

  const diagnostics = [];
  if (h3Violations.length) {
    diagnostics.push(
      `H3 violations: ${h3Violations.map((r) => `${r.id}(S=${r.S},M=${r.M})`).join(", ")}`
    );
  }
  if (results.session_net_accepted < 0) {
    diagnostics.push(`H4 session_net_accepted=${results.session_net_accepted}`);
  }
  if (h5Violations.length) {
    diagnostics.push(`H5 violations: ${h5Violations.map((r) => `${r.id}:mcpCalls=${r.mcpCalls}`).join(", ")}`);
  }
  if (results.skippedRows.length) {
    diagnostics.push(`Skipped rows: ${results.skippedRows.join(", ")}`);
  }
  if (!diagnostics.length) {
    diagnostics.push("All computed Phase 2 gates passed on measured rows.");
  }

  return {
    gates,
    h3_scope_row_count: h3Rows.length,
    h3_small_serve_s_gte_m_count: h3Violations.length,
    h5_single_chain_violations: h5Violations.map((r) => `${r.id}: mcpCalls=${r.mcpCalls}`),
    session_net_accepted: results.session_net_accepted,
    session_net_all36: results.session_net_all36,
    diagnostics: diagnostics.join(" "),
  };
}

function formatReportMarkdown({ results, computed, candidatePath, baselinePath, command }) {
  const g = computed.gates;
  return `# Phase 2 compact-surface gate report

**Report ID:** phase2-gate-${new Date().toISOString().slice(0, 10)}
**Surface:** ${results.surface || "compact"}
**Baseline commit:** \`${results.baselineCommit || "unknown"}\`
**Candidate results:** \`${candidatePath}\`
**Baseline results:** \`${baselinePath || "(self)"}\`
**Compare command:** \`${command}\`
**H3 policy:** [docs/research/A-012-bypass-policy.md](docs/research/A-012-bypass-policy.md)

## Gate statuses

| Gate | Status |
|------|--------|
| H1 | ${g.H1} |
| H2 | ${g.H2} |
| H3 | ${g.H3} |
| H4 | ${g.H4} |
| H5 | ${g.H5} |
| H6 | ${g.H6} |
| H7 | ${g.H7} |
| H8 | ${g.H8} |

## Computed metrics

- \`session_net_accepted\`: ${computed.session_net_accepted}
- \`session_net_all36\`: ${computed.session_net_all36}
- H3 scope rows: ${computed.h3_scope_row_count}
- H3 sGteM violations: ${computed.h3_small_serve_s_gte_m_count}
- H5 single-chain violations: ${computed.h5_single_chain_violations.length}
- Measured rows: ${results.rowCount}
- Skipped rows: ${results.skippedRows.length}

## Diagnostics

${computed.diagnostics}

## H3 scope note (A-012)

H3 evaluates **accepted serve** rows only (bypass/degrade/cache_hit excluded). When no \`*_small\` task ids are present, all accepted serve rows in the golden corpus are used.

## H5 note

Compact surface uses one external \`symforge\` MCP call per task. Multi-hop rows (\`chain=multi\`) execute legacy tools in-process but report \`mcpCalls=1\`.
`;
}

function parseArgs(argv) {
  const args = { candidate: null, baseline: null, report: null };
  for (let i = 2; i < argv.length; i++) {
    if (argv[i] === "--baseline" && argv[i + 1]) {
      args.baseline = argv[++i];
    } else if (argv[i] === "--report" && argv[i + 1]) {
      args.report = argv[++i];
    } else if (!args.candidate) {
      args.candidate = argv[i];
    }
  }
  return args;
}

function main() {
  const args = parseArgs(process.argv);
  if (!args.candidate) {
    console.error("Usage: node scripts/compare-results.cjs <candidate.json> [--baseline path] [--report out.md]");
    process.exit(2);
  }
  const candidatePath = path.resolve(args.candidate);
  const results = normalizeResults(loadJson(candidatePath));
  const computed = computeGates(results);
  const command = `node scripts/compare-results.cjs ${path.relative(REPO_ROOT, candidatePath)}${args.baseline ? ` --baseline ${args.baseline}` : ""}`;

  const payload = {
    surface: results.surface || "compact",
    baselineCommit: results.baselineCommit,
    candidate: path.relative(REPO_ROOT, candidatePath),
    ...computed,
  };

  console.log(JSON.stringify(payload, null, 2));

  if (args.report) {
    const md = formatReportMarkdown({
      results,
      computed,
      candidatePath: path.relative(REPO_ROOT, candidatePath),
      baselinePath: args.baseline,
      command,
    });
    fs.writeFileSync(path.resolve(args.report), md);
    console.error(`Wrote ${args.report}`);
  }

  const phase2MinPass = computed.gates.H3 === "PASS" && computed.gates.H4 === "PASS";
  process.exit(phase2MinPass ? 0 : 1);
}

main();
