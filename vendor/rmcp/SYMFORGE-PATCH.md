# SYMFORGE-PATCH: rmcp 3.5.0

This directory is the crates.io `rmcp` 3.5.0 package, copied verbatim (minus
cargo's `.cargo-ok` marker) and wired in through `[patch.crates-io]` in the
root `Cargo.toml`. Exactly two files differ from upstream; the full diff is
below. Nothing else in this directory may be edited.

## What and why

1. **Sticky request-metadata requirement.** When the first request on a
   session is `server/discover`, `serve_server` calls
   `Peer::require_request_metadata()` (`src/service/server.rs:625`). That
   `AtomicBool` is never cleared. If the client then falls back to
   `initialize` (its discover timed out, or it prefers the session
   lifecycle), the handler still rejects every later request lacking
   2026-07-28 `_meta` (`src/handler/server.rs:78-83`). Claude Code sessions
   ended with no tools on 2026-09-25 this way. Fix: a successful
   `initialize` clears the requirement. A request that itself declares
   2026-07-28 in `_meta` is still validated strictly.
2. **Requested version stored as peer info.** The in-loop default
   `initialize` calls `set_peer_info(request.clone())`, recording the
   REQUESTED version, while the pre-init path records the NEGOTIATED one
   (`src/service/server.rs:669`). After discover + `initialize(2026-07-28)`
   the client is answered `2025-11-25` but `context.protocol_version()` says
   2026-07-28, so `ping` returns method-not-found and SEP-2322 result shapes
   switch on. Fix: after a successful `initialize`, record the negotiated
   version, mirroring the pre-init path. It is done in the dispatch arm,
   not the default `initialize`, so a server overriding `initialize` is
   covered too.

Acceptance: `tests/protocol_lifecycle_matrix.rs` (54 rows). Unpatched
3.5.0 fails 16 rows, all discover-then-initialize.

## Rebase on the next rmcp bump

1. If upstream fixed both bugs (replay the matrix test without the patch),
   delete this directory and the `rmcp` line under `[patch.crates-io]`.
2. Otherwise copy the new crate here, re-apply the diff below, update the
   version in this file, and run the matrix test.

Upstream: modelcontextprotocol/rust-sdk. Issue not yet filed at the time of
writing; a draft exists with the replay steps.

## Caveats

- `[patch]` is ignored when symforge is installed from crates.io
  (`cargo install symforge`). That path keeps both bugs until upstream ships
  a fix. The npm and GitHub release binaries are built from this tree and
  carry the patch.
- `build.rs` runs `git config core.hooksPath .githooks` on the directory two
  levels up when it holds both `.githooks/` and `.git`. For this vendored
  copy that is the symforge repo root, which has no `.githooks/` today, so it
  is a no-op. Adding a root `.githooks/` would make every build rewrite the
  repo's hooks path.

## Diff against crates.io rmcp 3.5.0

```diff
--- a/src/service.rs
+++ b/src/service.rs
@@ -1045,6 +1045,13 @@
             .store(true, std::sync::atomic::Ordering::Release);
     }
 
+    // SYMFORGE-PATCH: a successful `initialize` selects the session lifecycle,
+    // so a requirement latched by an earlier `server/discover` must not outlive it.
+    pub(crate) fn clear_request_metadata_requirement(&self) {
+        self.request_metadata_required
+            .store(false, std::sync::atomic::Ordering::Release);
+    }
+
     pub(crate) fn request_metadata_required(&self) -> bool {
         self.request_metadata_required
             .load(std::sync::atomic::Ordering::Acquire)
--- a/src/handler/server.rs
+++ b/src/handler/server.rs
@@ -101,10 +101,21 @@
         let legacy_request =
             uses_legacy_lifecycle(protocol_version.as_ref(), requires_request_metadata);
         let result = match request {
-            ClientRequest::InitializeRequest(request) => self
-                .initialize(request.params, context)
-                .await
-                .map(ServerResult::InitializeResult),
+            ClientRequest::InitializeRequest(request) => {
+                // SYMFORGE-PATCH: mirror the pre-init path (service/server.rs):
+                // record the NEGOTIATED version as peer info, and let the client's
+                // choice of the initialize lifecycle clear a requirement latched
+                // by an earlier `server/discover`.
+                let peer = context.peer.clone();
+                let mut peer_info = request.params.clone();
+                let result = self.initialize(request.params, context).await;
+                if let Ok(init) = &result {
+                    peer_info.protocol_version = init.protocol_version.clone();
+                    peer.set_peer_info(peer_info);
+                    peer.clear_request_metadata_requirement();
+                }
+                result.map(ServerResult::InitializeResult)
+            }
             ClientRequest::DiscoverRequest(_request) => self
                 .discover(context)
                 .await
```
