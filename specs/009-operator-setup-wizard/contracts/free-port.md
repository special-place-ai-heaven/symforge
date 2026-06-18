# Contract: collision-free serve port (US1)

**Surface**: `server::serve` — the no-explicit-address bind path + a new probe helper.

## Behavior (FR-001/002/003)
- **No explicit address**: prefer `DEFAULT_LISTEN` (127.0.0.1:8787); attempt to bind it.
  - free → bind it, print the exact URL.
  - occupied → bind `127.0.0.1:0` (OS-assigned ephemeral free port), use the returned
    port, print that URL. Never a dead second listener.
- **Explicit address**: honor exactly when free; when occupied → **fail loudly** with the
  conflict (no silent substitution).

## Helper
`probe_free_port(preferred: Option<SocketAddr>) -> io::Result<SocketAddr>`: tries
`preferred` (if any) via a real bind; on failure binds `:0` and returns the OS port.
Reuses `bind_listener` (SO_REUSEADDR, existing error path). Returns the bound listener or
its address so the reported URL == the bound URL (FR-020).

## Regression
`serve_binds_free_port_when_default_occupied`: occupy 8787 with a dummy listener, start
serve with no explicit address, assert it binds a DIFFERENT reachable port (a GET to the
reported URL succeeds), and never leaves a non-serving listener (SC-003). Control: 8787
free → binds 8787. Explicit-occupied → loud error, no dead listener.
