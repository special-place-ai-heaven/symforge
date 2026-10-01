# Contract: `_meta["symforge/withheld"]`

Normative wire shape for actionable refusal metadata (spec US1 / FR-001). Peers: `symforge/result_status`, `symforge/project_evidence`, `symforge/repeat_notice` in `src/protocol/result_status.rs`.

## Key

`symforge/withheld` — constant to be introduced as `WITHHELD_META_KEY`.

## Payload

```json
{
  "contract_version": 1,
  "path": "config/app.json",
  "policy_version": 7,
  "findings": [
    {
      "id": "wf_01HZX...",
      "rule_id": "context_assignment",
      "line_start": 12,
      "line_end": 12,
      "shape": "16-char mixed alnum+symbol after password =",
      "confidence": "high",
      "actions": [
        { "name": "externalize", "available": true },
        { "name": "encrypt", "available": false, "reason": "sops_not_installed" },
        { "name": "dismiss", "available": true }
      ]
    }
  ]
}
```

## Rules

- Present only on results that withhold for secret admission findings (or path-rule cases explicitly covered by implement tasks). Successful reads omit the key.
- `shape` MUST NOT contain secret bytes; tests assert the synthetic secret string is absent from the entire serialized `CallToolResult`.
- `actions[].name` is one of `externalize` | `encrypt` | `dismiss`.
- When `available` is false, `reason` SHOULD be a stable machine token (`sops_not_installed`, `no_age_recipient`, `unsupported_format`, `path_rule_only`, …).
- Human refusal text remains the primary text block; `_meta` is additive.
- Single-writer: refusal/read-gate (or equivalent seam that already knows findings) attaches this key — not arbitrary tools.

## Versioning

Bump `contract_version` on any field rename, claim change, or semantic change to `shape` / availability reasons.
