# Contract: `symforge serve` CLI

## Synopsis
```
symforge serve [--listen <HOST:PORT>] [--api-key <KEY> | --api-key-env <VAR>]
```

## Flags
| Flag | Default | Meaning |
|------|---------|---------|
| `--listen` | `127.0.0.1:8787` | bind address; `HOST` may be loopback or non-loopback; `PORT=0` → OS-assigned |
| `--api-key` | none | single static Bearer key (inline). **Visible in process listings** (`ps` / Task Manager); allowed on loopback only — prefer `--api-key-env`. |
| `--api-key-env` | none | name of env var holding the key. **Required for a non-loopback (network) bind**; the only production-safe path (secret stays out of argv). |

## Behavior
- Resolves the key from `--api-key`, else `--api-key-env`, else none.
- **Refuse-to-start (exit code 2)** when bind host is non-loopback AND no key resolved. Message names the cause.
- **Inline-key source policy (P2-E):**
  - Passing an inline `--api-key` emits a startup **WARNING** (it is visible in process listings) recommending `--api-key-env`.
  - **Refuse-to-start (exit code 2)** when an inline `--api-key` is passed on a **non-loopback** bind — a network bind must use `--api-key-env`. Loopback binds may still accept an inline key for local convenience.
- On success: prints the attach URL (`http://HOST:PORT/mcp`) to stdout, then runs one long-lived server until SIGINT/SIGTERM (graceful shutdown), serving the MCP surface over `/mcp`.
- The `/mcp` surface is concurrency-bounded by the shared `RequestGovernor` (P2-F): each request acquires one permit; beyond `max_concurrency` (default 16) requests queue, and a saturated server sheds with `503 Service Unavailable` + `Retry-After`.
- Exit code `0` on clean shutdown; non-zero on bind error (port in use → message names the address) or refuse-to-start.

## Non-goals (this slice)
- No multi-key management, no TLS termination flags, no daemon/background detach (operator backgrounds it). These are later features.

## Acceptance
- `serve --listen 0.0.0.0:8787` (no key) → exit 2, nothing listening.
- `serve --listen 0.0.0.0:8787 --api-key k` (inline key on routable bind) → exit 2 (P2-E refusal), message names the argv-leak cause and recommends `--api-key-env`.
- `serve --listen 127.0.0.1:0 --api-key k` → prints URL with the assigned port, stays up (emits the inline-key WARNING).
- `serve --listen 0.0.0.0:8787 --api-key-env SYMFORGE_KEY` (with the env var set) → starts (network bind sources the key from the environment).
