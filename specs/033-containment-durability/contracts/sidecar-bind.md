# Contract: Sidecar bind and non-loopback token

**Story:** US2 (F-002)
**Implement in:** `src/paths.rs` (`pub(crate) is_loopback_bind_host`), `src/sidecar/server.rs`, `src/sidecar/router.rs`

## Environment

| Name | Role |
|------|------|
| `SYMFORGE_SIDECAR_BIND` | Host, default `127.0.0.1` (unchanged) |
| `SYMFORGE_SIDECAR_ALLOW_NON_LOOPBACK` | Truthy (`1`/`true`/`yes`/`on`) required for non-loopback |
| `SYMFORGE_SIDECAR_TOKEN` | Required iff bind is non-loopback |

Do not read `SYMFORGE_DAEMON_ALLOW_NON_LOOPBACK` for sidecar.

## Spawn matrix

| Bind | Opt-in | Token | Result |
|------|--------|-------|--------|
| loopback / localhost / `::1` | any | any | Start. No Bearer layer. |
| `0.0.0.0` / LAN | unset | any | Fail. No listener. |
| non-loopback | set | missing | Fail. No listener. |
| non-loopback | set | present | Warn. Start. Bearer required on every route except `/health` (health may stay unauthenticated for probes — implementer: if `/health` is exempt, document it; source-read MUST be gated). |

Loopback predicate: same as daemon (`localhost` case-insensitive, or parsed `IpAddr::is_loopback`).

## HTTP

When the listener is `NonLoopbackAuthenticated`, attach Bearer to the **whole router**. Requests without `Authorization: Bearer <token>` receive 401. Optional documented `/health` exemption only; `/workflows/source-read` is a MUST-gated example, not the only gated route. Constant-time compare after hashing both sides to 32 bytes if a compare helper already exists on the daemon (`daemon.rs` hashes then `constant_time_eq`); reuse that shape, do not invent a second compare. Do not copy daemon always-on loopback Bearer.

## Tests (RED first)

Mirror `test_daemon_bind_policy_rejects_non_loopback_without_opt_in` / `_allows_non_loopback_with_explicit_opt_in` next to sidecar spawn. Additional: opt-in without token fails; loopback spawn does not require the new token env.
