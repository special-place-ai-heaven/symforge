# 0015. Project-config trust gating for `.symforge`

Date: 2026-05-19
Status: Accepted

## Context

This ADR is about trust for project-local `.symforge` configuration. It is
not about the existing search/context trust envelope used by tools such as
`search_files`, `search_text`, `search_symbols`, `get_file_context`, or
`diff_symbols`. Recent "trust gate" language in plans and release notes refers
to search-and-context evidence quality; this decision covers a separate future
security boundary: whether SymForge should trust project-owned `.symforge`
configuration before it influences runtime behavior.

Current source status matters:

- `src/edit_safety/mod.rs` exports only `tee`.
- `src/edit_safety/trust.rs` is absent.
- `src/hash.rs` already exposes SHA-256 helpers.
- `Cargo.toml` already has `sha2`, `dirs`, `dunce`, `serde`, and `serde_json`.
- Existing `.symforge` files include runtime state such as snapshot stores,
  sidecar port/session files, frecency/coupling databases, and tee snapshots.

RTK is only the source of a selectively borrowed pattern: hash project-local
configuration, persist the trusted hash outside the project, and revoke trust
when content changes. RTK is not a SymForge runtime dependency. SymForge will
not import RTK shell hooks, hook installers, command rewriting, permission
parsers, CLI output filters, OpenClaw plugin code, Homebrew formula code, or
HTTP telemetry.

The problem is real but narrower than RTK's problem. SymForge is a local-first
MCP code-intelligence server, not a command-output rewriting CLI. A malicious
repository should not be able to change project-local SymForge configuration and
silently affect MCP behavior. At the same time, SymForge should not bulk-import
RTK's hook surface or turn trust checks into a broad interactive prompt system.

## Decision

SymForge adopts a minimal project-config trust gate for future `.symforge`
configuration. The trust gate records a content hash for the trust-covered
project configuration and reports explicit evidence when the current content is
untrusted or changed.

The trust-covered configuration tree is deliberately narrower than every file
under `.symforge`. It covers project configuration inputs, not volatile runtime
artifacts. Current and future implementation must treat the phrase
"`.symforge` project config" as:

- `.symforge/config.toml`, if present;
- files under `.symforge/config/`, if present;
- later explicit project-config paths added by an ADR or goal that names them.

The trust gate must not hash volatile runtime artifacts such as
`.symforge/index.bin`, `.symforge/frecency.db`, `.symforge/coupling.db`,
`.symforge/sidecar.port`, `.symforge/sidecar.session`,
`.symforge/hook-adoption.log`, or `.symforge/tee/**` as project config. Those
files change as part of normal operation and would make trust unusable.

### Trust status vocabulary

The implementation must expose these status names:

```rust
TrustStatus::Trusted
TrustStatus::Untrusted
TrustStatus::ContentChanged { expected, actual }
TrustStatus::EnvOverride
```

Implementations may attach additional evidence fields, but the status names and
the `ContentChanged { expected, actual }` hash pair are the contract. `expected`
is the hash recorded in the user-local trust store. `actual` is the hash
computed from the current trust-covered project-config tree.

Missing trust records, unreadable trust records, corrupt JSON, unsupported store
versions, canonicalization failures, and malformed hashes must not collapse to
`Trusted`. They must report `Untrusted` with warning evidence unless the
CI-only override below applies.

### Trust store

Trust records live outside the project at:

```text
dirs::data_local_dir()/symforge/trust.json
```

The store is user-local and versioned. Version 1 records at least:

- schema version;
- canonical project path key;
- trusted hash;
- RFC3339 `trusted_at` timestamp;
- SymForge version or writer identifier when available.

The store must not contain raw project config content, raw prompts, secrets,
`.env` contents, provider credentials, private keys, unbounded source blobs, or
telemetry payloads. Hashes and canonical path keys are enough.

Missing store file means no trust has been recorded yet. Corrupt or unsupported
store means SymForge cannot prove trust. Both states fail secure by returning
`Untrusted` evidence and allowing default LOG_ONLY behavior to decide whether
the current operation continues.

### Canonical project keys

Trust records are keyed by canonical project path, not by a user-supplied string
or current working directory text.

The canonicalization contract is: use `std::fs::canonicalize` semantics and
apply `dunce` normalization so Windows verbatim prefixes such as `\\?\` do not
create duplicate trust keys. The existing `src/worktree.rs::canonicalize`
helper, which delegates to `dunce::canonicalize`, is an acceptable implementation
shape if it preserves the same normalized key invariant.

If a project path cannot be canonicalized, SymForge must not write a trust
record for it.

### TOCTOU-safe recording

Trust recording must use the precomputed hash from the same evaluation that
reported the trust decision. The recording API must not perform a separate
"trust whatever is on disk now" walk after the user has reviewed a different
hash.

For CLI UX this means an accept command must either:

- receive the displayed `actual` hash and record it only if a same-command
  evaluation produces the same hash; or
- operate on an evaluation object that already contains the computed hash.

The important invariant is that the stored hash is the hash the operator chose
to trust, not a later silently recomputed value.

### CI-only override

`SYMFORGE_TRUST_PROJECT_CONFIG=1` is a CI escape hatch, not a local developer
shortcut.

It produces `TrustStatus::EnvOverride` only when at least one known CI
environment variable is present. The initial recognized CI indicators are:

- `CI`
- `GITHUB_ACTIONS`
- `GITLAB_CI`
- `BUILDKITE`
- `CIRCLECI`
- `JENKINS_URL`
- `TEAMCITY_VERSION`
- `TF_BUILD`
- `APPVEYOR`
- `DRONE`

Outside those environments, `SYMFORGE_TRUST_PROJECT_CONFIG=1` is ignored and the
response must say the override was ignored because the process is not recognized
as CI. The override must not write or mutate trust records.

### Mode and rollout

The default runtime mode is LOG_ONLY. In LOG_ONLY, untrusted, changed, missing,
or corrupt trust state does not crash the daemon and does not silently succeed.
The next relevant SymForge response must carry a concise warning with the status
and hash evidence.

ENFORCE is opt-in only. The first ENFORCE implementation uses
`SYMFORGE_PROJECT_CONFIG_TRUST_MODE=enforce`, resolved at call time per
[ADR 0016](./0016-call-time-capability-resolution.md). Any later config-based
policy must keep the policy outside the untrusted project config being checked,
or it becomes circular.

Default calls that do not load or act on trust-covered project configuration
must preserve current deterministic behavior.

### Control surface

The selected user-control surface is CLI subcommands on the existing `symforge`
binary, not MCP trust tools and not RTK hooks.

The minimum future CLI surface is:

- `symforge trust project-config status --project <path>`
- `symforge trust project-config accept --project <path> --hash <actual>`
- `symforge trust project-config revoke --project <path>`

`status` reports the same TrustStatus vocabulary as the core. `accept` records
trust only through the TOCTOU-safe hash flow above. `revoke` removes the
canonical project key from the user-local trust store.

This is the minimum SymForge-native surface because:

- SymForge already has a CLI binary for `init`, `daemon`, and hook entry points.
- Trust mutation is an operator decision, not a task an agent should perform
  implicitly through an MCP tool call.
- Adding MCP trust tools would expand the public tool surface and trigger the
  tool-consolidation/backward-compat obligations in
  [ADR 0001](./0001-tool-consolidation-contract.md).
- RTK hook prompts, command rewriting, and output filtering solve a different
  product problem and are out of scope.

MCP tool responses may carry warning evidence in LOG_ONLY mode, but MCP is not
the selected trust-mutation surface. If a later ADR wants an MCP status-only
tool, it must justify the tool addition separately.

### Integrity sidecars

Integrity sidecars are deferred. Current SymForge `.symforge` files are runtime
state, derived stores, ports, sessions, or edit-safety snapshots. They are not
equivalent to RTK's installed command-rewrite hook script.

Do not add hash sidecars in this trust-gate implementation. A later integrity
sidecar task is authorized only if SymForge adds executable or security-sensitive
project-local behavior that needs a per-file tamper baseline. Until then, the
project-config trust store is the security boundary and sidecars are bulk.

## Non-Goals

This ADR does not implement trust code.

This ADR does not add CLI subcommands, MCP tools, daemon wiring, warning suffix
logic, database migrations, watchers, prompts, or integrity sidecars.

This ADR does not trust RTK as a dependency and does not import RTK runtime code.

This ADR does not protect volatile `.symforge` runtime artifacts from ordinary
local mutation. It only defines project-config trust semantics.

## Relationships To Prior ADRs

[ADR 0011](./0011-frecency-bump-policy.md) establishes the pattern of shipping
new behavior behind explicit policy and preserving deterministic defaults. This
trust gate follows that pattern with default LOG_ONLY and opt-in ENFORCE.

[ADR 0012](./0012-edit-and-ranker-hook-architecture.md) keeps shared edit and
ranking bodies feature-blind. Trust warning suffixes must not bypass those
extension points or inline feature-specific behavior into unrelated handlers.

[ADR 0014](./0014-watcher-subsystem-spawn-blocking-discipline.md) governs
watcher-owned `spawn_blocking` mutation sites. The trust gate must not add a
`.symforge` watcher or live rehash loop in its first implementation. Evaluate
trust at explicit call/startup boundaries instead.

[ADR 0016](./0016-call-time-capability-resolution.md) is load-bearing for trust
mode and policy. User/operator-controlled trust policy must resolve at call time
and responses must report fallback, unavailable, ignored override, or enforced
states explicitly.

## Consequences

**Easier**

- SRTK06 can implement a pure trust core without deciding CLI or daemon UX.
- SRTK07 has one selected mutation surface: CLI subcommands plus LOG_ONLY warning
  evidence, not both CLI and MCP tools.
- Trust records remain local-first and outside malicious project control.
- RTK's useful hash-and-revoke idea is reused without importing RTK's product
  shape.

**Harder**

- The implementation must distinguish project config from volatile `.symforge`
  runtime artifacts.
- A CLI accept flow must carry hashes carefully to avoid recording a different
  post-review value.
- ENFORCE cannot use untrusted project config as its own policy source.
- Corrupt or missing trust-store state must produce explicit evidence, which
  adds response text even in default LOG_ONLY mode.

**New invariants future code must respect**

1. Project-config trust status must never silently report changed, corrupt,
   missing, skipped, unavailable, or ignored override states as success.
2. Trust records must be keyed by normalized canonical project paths.
3. Trust recording must store the precomputed hash the operator reviewed.
4. `SYMFORGE_TRUST_PROJECT_CONFIG=1` must be honored only under recognized CI.
5. No MCP trust-mutation tool is authorized by this ADR.
6. No RTK runtime dependency, hook installer, command rewriter, or telemetry
   surface is authorized by this ADR.
7. No integrity sidecar is authorized until a later decision identifies
   executable or security-sensitive `.symforge` project-local behavior.

## Acceptance Criteria

- `TrustStatus` has the four named states above.
- Missing or corrupt trust store degrades to explicit `Untrusted` evidence.
- Changed project config reports `ContentChanged { expected, actual }`.
- Trust records use `dirs::data_local_dir()/symforge/trust.json`.
- Canonical project keys use `std::fs::canonicalize` semantics with `dunce`
  normalization.
- `SYMFORGE_TRUST_PROJECT_CONFIG=1` produces `EnvOverride` only under recognized
  CI indicators.
- Default rollout is LOG_ONLY; ENFORCE is opt-in and resolved at call time.
- The selected trust-control surface is CLI subcommands, not MCP tools.
- Integrity sidecars are deferred.
