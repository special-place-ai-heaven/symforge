# Contract: `secret_remediate` tool

Normative request/response behavior for the remediation tool (spec US2–US4 / FR-004–FR-011).

## Tool name

Working name: `secret_remediate`. Implementation may adjust only if tool-registry pins require a SymForge naming convention — behavior stays as specified.

## Request

| Param | Type | Required | Default | Notes |
|---|---|---|---|---|
| `scope` | string \| string[] \| `"repo"` | yes | — | Paths resolved via `discovery::resolve_repo_path` |
| `finding_ids` | string[] | yes | — | Empty array is rejected unless a later amendment explicitly defines "all in scope" |
| `action` | enum | yes | — | `externalize` \| `encrypt` \| `dismiss` |
| `preview` | bool | no | `true` | `false` selects apply (write tool) |
| `idempotency_key` | string | no | — | Honored on apply like edit-lane |

## Preview response (`preview=true`)

- Text (and/or structured content) includes:
  - Masked unified diff using `«secret:N»` placeholders
  - List of files that would be created or modified
- `_meta` MAY include a machine preview summary; MUST NOT include secret bytes
- Worktree unchanged

## Apply response (`preview=false`)

- Requires harness write permission (MCP client approval)
- On success, report MUST include:
  - Written paths (observed)
  - Fresh re-scan summary for those paths (observed)
  - History note: old value remains in git history since `<sha>` OR honest working-tree/uncommitted wording
- On failure: no success claim; partial writes rolled back
- Idempotent replay returns the recorded success without re-leaking secrets

## Action-specific rules

### externalize

- Ensure `.env` exists (create if needed) and is gitignored (add `.gitignore` entry if needed)
- Rewrite source in file idiom (`process.env.X` / `os.environ["X"]` / `std::env::var("X")` / `${X}`)
- Unsupported idiom → action unavailable for that finding (preview explains)

### encrypt

- Requires `sops` on PATH and public recipient from `.sops.yaml` or `SOPS_AGE_RECIPIENTS`
- Formats: JSON, YAML, TOML, ENV
- Spawn only via `process_util::hidden_command`
- Never accepts private keys or decrypts for the agent

### dismiss

- Writes committed dismissal record bound to **line content digest**
- Satisfies the five anti-allowlist invariants in `data-model.md`
- Does not embed bypass instructions in refusal text

## Forbidden

- Returning secret bytes or private keys
- Custom crypto formats
- Apply without preview capability existing as the default path
- Disk I/O inside `refuse_by_policy`
