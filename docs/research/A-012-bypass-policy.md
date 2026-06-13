# A-012 — Bypass policy and H3 scoring scope

**Tasks:** T034–T035, T036 (P-FF / H6 rules)  
**Status:** **INTERIM POLICY SELECTED**

## Policy choice (T034)

**Selected path:** `serve-only-h3-scope` (interim until two-hop harness lands)

Rationale: sf-bench two-hop bypass completion harness (`lib/bypass-hop.js` per gap plan G-012) is not available locally (B-SFBENCH). H3 gate counts **accepted serve** small-file rows only; bypass rows excluded from H3 numerator per gap plan §12A bypass note.

## Bypass Policy Record

```yaml
policy: serve-only-h3-scope
affected_rows:
  - cfg-if/pff_whole_lib
  - records/pff_whole_module
  - is-plain/pff_whole_index
  - compression/pff_whole_service
completion_check: null
h3_scope: "H3 evaluates sGteM on accepted serve rows (EQUIVALENT ∧ S≤M); bypass rows excluded"
h6_eligibility_rule: "eligible_h6=false rows excluded from H6 numerator and denominator"
contradictions: []
```

**Explicit limitation:** Bypass **completion** is not claimed until two-hop harness validates host Read completion.

## P-FF and eligible H6 rules (T036)

Document for golden-file README (target: `<sf-bench>/routes.golden.jsonl` README — blocked until B-SFBENCH clears):

### Policy P-FF (full-file review)

- **Decision:** `bypass`
- **Equivalence class:** `BYPASS`
- **eligible_h6:** `false`
- **Use case:** Whole-file review tasks where SymForge correctly declines to serve truncated context
- **Expected rows:** 4 in golden corpus (enforcement may wait for §12B)

### eligible_h6 rules

| Row type | eligible_h6 | H6 impact |
|----------|-------------|-----------|
| Normal serve/bypass economics rows | `true` | Included in H6 eligible denominator |
| P-FF full-file bypass | `false` | Excluded from H6 numerator and denominator |
| BYPASS (non-P-FF) | per row spec | Scored in bypass ledger; H3 serve-only scope applies |

## sf-bench surface link (T035)

**Blocked:** Cannot link bypass-hop evidence or live compare-results H3 scope until sf-bench restored.

When unblocked:

- If two-hop lands → switch policy to `two-hop-completion` and link completion check artifact.
- Else → confirm compare-results `--preflight` H3 column matches serve-only scope above.

## Verdicts

| ID | Verdict |
|----|---------|
| A-012 | **OPEN** (interim scope documented; completion not validated) |
| A-032 | **OPEN** (P-FF rules documented; 4 corpus rows not validated) |

**§12A A-012 item:** satisfies **"H3 scoped to serve-only until implemented"** documentation requirement.
