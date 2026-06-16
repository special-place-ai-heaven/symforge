# Contract: `symforge serve` CLI

## Synopsis
```
symforge serve [--listen <HOST:PORT>] [--api-key <KEY> | --api-key-env <VAR>]
```

## Flags
| Flag | Default | Meaning |
|------|---------|---------|
| `--listen` | `127.0.0.1:8787` | bind address; `HOST` may be loopback or non-loopback; `PORT=0` → OS-assigned |
| `--api-key` | none | single static Bearer key (inline) |
| `--api-key-env` | none | name of env var holding the key (preferred over inline for secrecy) |

## Behavior
- Resolves the key from `--api-key`, else `--api-key-env`, else none.
- **Refuse-to-start (exit code 2)** when bind host is non-loopback AND no key resolved. Message names the cause.
- On success: prints the attach URL (`http://HOST:PORT/mcp`) to stdout, then runs one long-lived server until SIGINT/SIGTERM (graceful shutdown), serving the MCP surface over `/mcp`.
- Exit code `0` on clean shutdown; non-zero on bind error (port in use → message names the address) or refuse-to-start.

## Non-goals (this slice)
- No multi-key management, no TLS termination flags, no daemon/background detach (operator backgrounds it). These are later features.

## Acceptance
- `serve --listen 0.0.0.0:8787` (no key) → exit 2, nothing listening.
- `serve --listen 127.0.0.1:0 --api-key k` → prints URL with the assigned port, stays up.
