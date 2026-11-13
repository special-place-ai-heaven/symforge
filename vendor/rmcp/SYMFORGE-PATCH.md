# SYMFORGE-PATCH: rmcp 3.10.0

This directory is the crates.io `rmcp` 3.10.0 package, copied verbatim (minus
cargo's `.cargo-ok` marker) and wired in through `[patch.crates-io]` in the
root `Cargo.toml`. Exactly four files differ from upstream; the full diff is
below. Nothing else in this directory may be edited.

## What and why

Upstream 3.5.1+ serves `server/discover` as a bootstrap probe inside the
pre-lifecycle loop of `serve_server` (`src/service/server.rs`), so the
discover-then-initialize order the 3.5.0 patch fixed now works unpatched.
Two neighbouring orders still break on 3.10.0, and the build script still
touches the host repo:

1. **Pre-lifecycle notification aborts the server.** In that loop any
   message that is not a request ends `serve_server` with
   `ExpectedInitializeRequest`, and the symforge process exits. A client
   whose discover timed out SHOULD send `notifications/cancelled` for it
   before falling back to `initialize`, so that fallback kills the server.
   On 3.5.0 discover left the loop at once and the notification reached the
   service loop. Fix: the loop ignores client notifications. Nothing is in
   flight before a lifecycle exists, so there is nothing to act on.
   Responses and errors still abort, as before.
2. **Inline lifecycle latch never clears.** A first request other than
   discover that carries complete 2026-07-28 `_meta` sets
   `Peer::inline_protocol_version`, an `OnceLock`. A later successful
   `initialize` cannot clear it, so every request without `_meta` is refused
   with "request _meta is missing", and the outbound check refuses
   `roots/list`. Fix: the latch is an `RwLock<Option<_>>` with
   `clear_inline_protocol_version()`, called after a successful
   `initialize` in the dispatch arm (`src/handler/server.rs`).
3. **Requested version stored as peer info.** The in-loop default
   `initialize` calls `set_peer_info(request.clone())`, recording the
   REQUESTED version, while the pre-init path records the NEGOTIATED one.
   After an inline opener plus `initialize(2026-07-28)`, the client is
   answered `2025-11-25` but `context.protocol_version()` says 2026-07-28.
   Fix: the same dispatch arm records the negotiated version, so a server
   overriding `initialize` is covered too.
4. **`build.rs` neutered.** Upstream's build script runs
   `git config core.hooksPath .githooks` on the directory two levels above
   the crate whenever it holds `.githooks/` and `.git`. For this vendored
   copy that is the symforge repo root, and in a worktree `.git` is a file,
   so the guard does not tell a dependency build from rmcp's own workspace.
   A dependency must not rewrite its host repo's git config, so `main` is
   empty. The script emitted no cargo directives, so nothing else changes.

Acceptance: `tests/protocol_lifecycle_matrix.rs` (102 rows). Unpatched
3.10.0 fails 40 rows: all 24 `discover_cancel_then_init` rows (the server
exits after the cancel) and 16 `inline_then_init` rows (missing `_meta`).

## Rebase on the next rmcp bump

1. Replay the matrix test without the patch. If it passes, delete this
   directory and the `rmcp` line under `[patch.crates-io]`.
2. Otherwise copy the new crate here, re-apply the diff below, update the
   version in this file, and run the matrix test. Keep `build.rs` empty
   unless upstream's script has gained real build work.

Upstream: modelcontextprotocol/rust-sdk. Not yet reported upstream at the
time of writing.

## Caveats

- `[patch]` is ignored when symforge is installed from crates.io
  (`cargo install symforge`). That path keeps these bugs until upstream
  ships a fix. The npm and GitHub release binaries are built from this tree
  and carry the patch.

## Diff against crates.io rmcp 3.10.0

```diff
--- a/build.rs
+++ b/build.rs
@@ -1,23 +1,5 @@
-// Install git hooks on build
-fn main() {
-    // Only run in the workspace root (not when building as a dependency)
-    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
-    let workspace_root = std::path::Path::new(&manifest_dir)
-        .parent()
-        .and_then(|p| p.parent());
-
-    if let Some(root) = workspace_root {
-        let githooks_dir = root.join(".githooks");
-        let git_dir = root.join(".git");
-
-        // Only configure if we're in the actual workspace (not a dependency)
-        // and git directory exists
-        if githooks_dir.exists() && git_dir.exists() {
-            // Configure git to use our hooks directory
-            let _ = std::process::Command::new("git")
-                .args(["config", "core.hooksPath", ".githooks"])
-                .current_dir(root)
-                .output();
-        }
-    }
-}
+// SYMFORGE-PATCH: upstream's build script ran `git config core.hooksPath .githooks`
+// on the directory two levels up, which for this vendored copy is the symforge
+// repo root. A dependency must not rewrite its host repo's git config, so the
+// script is a no-op here. It emitted no cargo directives, so nothing else changes.
+fn main() {}
--- a/src/handler/server.rs
+++ b/src/handler/server.rs
@@ -132,10 +132,21 @@
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
+                // choice of the initialize lifecycle clear an inline lifecycle
+                // latched by an earlier request.
+                let peer = context.peer.clone();
+                let mut peer_info = request.params.clone();
+                let result = self.initialize(request.params, context).await;
+                if let Ok(init) = &result {
+                    peer_info.protocol_version = init.protocol_version.clone();
+                    peer.set_peer_info(peer_info);
+                    peer.clear_inline_protocol_version();
+                }
+                result.map(ServerResult::InitializeResult)
+            }
             ClientRequest::DiscoverRequest(_request) => self
                 .discover(context)
                 .await
--- a/src/service/server.rs
+++ b/src/service/server.rs
@@ -57,7 +57,7 @@
         // an inline session (e.g. discover on stdio) opened on, since it never
         // stores peer info, then to peer info.
         let version = originating_protocol_version()
-            .or_else(|| peer.inline_protocol_version().cloned())
+            .or_else(|| peer.inline_protocol_version())
             .or_else(|| peer.peer_info().map(|info| info.protocol_version.clone()));
         if version.is_some_and(|version| version >= ProtocolVersion::V_2026_07_28) {
             return Err(ServiceError::McpError(ErrorData::invalid_request(
@@ -670,6 +670,13 @@
                     ct,
                 ));
             }
+            // SYMFORGE-PATCH: a notification selects no lifecycle, so it must not
+            // abort the session. The common case is `notifications/cancelled` for a
+            // discover that timed out, sent before the client falls back to
+            // `initialize`. Nothing is in flight here, so there is nothing to act on.
+            ClientJsonRpcMessage::Notification(notification) => {
+                tracing::debug!(?notification, "ignoring pre-lifecycle notification");
+            }
             other => {
                 return Err(ServerInitializeError::ExpectedInitializeRequest(Some(
                     other,
--- a/src/service.rs
+++ b/src/service.rs
@@ -749,7 +749,9 @@
     progress_timeout_watchers: ProgressTimeoutWatchers,
     info: Arc<std::sync::RwLock<Option<Arc<R::PeerInfo>>>>,
     client_request_metadata: Arc<OnceLock<ClientRequestMetadata>>,
-    inline_protocol_version: Arc<OnceLock<ProtocolVersion>>,
+    // SYMFORGE-PATCH: upstream is a `OnceLock`, which a later successful
+    // `initialize` cannot clear; see `clear_inline_protocol_version`.
+    inline_protocol_version: Arc<std::sync::RwLock<Option<ProtocolVersion>>>,
     subscription_channels: Arc<std::sync::RwLock<SubscriptionChannelMap<R::PeerNot>>>,
     #[cfg(feature = "client")]
     response_cache: client::cache::PeerResponseCache<R>,
@@ -1059,20 +1061,40 @@
     /// Marks an inline lifecycle session that opened on `protocol_version`.
     #[cfg(feature = "server")]
     pub(crate) fn set_inline_protocol_version(&self, protocol_version: ProtocolVersion) {
-        let result = self.inline_protocol_version.set(protocol_version);
-        debug_assert!(result.is_ok(), "inline protocol version set more than once");
+        let mut slot = self
+            .inline_protocol_version
+            .write()
+            .expect("inline protocol version lock poisoned");
+        debug_assert!(slot.is_none(), "inline protocol version set more than once");
+        *slot = Some(protocol_version);
+    }
+
+    /// SYMFORGE-PATCH: a successful `initialize` selects the session lifecycle,
+    /// so an inline lifecycle latched by an earlier request must not outlive it.
+    #[cfg(feature = "server")]
+    pub(crate) fn clear_inline_protocol_version(&self) {
+        *self
+            .inline_protocol_version
+            .write()
+            .expect("inline protocol version lock poisoned") = None;
     }
 
     /// The version an inline lifecycle session opened on. Such a session never
     /// stores peer info.
     #[cfg(feature = "server")]
-    pub(crate) fn inline_protocol_version(&self) -> Option<&ProtocolVersion> {
-        self.inline_protocol_version.get()
+    pub(crate) fn inline_protocol_version(&self) -> Option<ProtocolVersion> {
+        self.inline_protocol_version
+            .read()
+            .expect("inline protocol version lock poisoned")
+            .clone()
     }
 
     #[cfg(feature = "server")]
     pub(crate) fn request_metadata_required(&self) -> bool {
-        self.inline_protocol_version.get().is_some()
+        self.inline_protocol_version
+            .read()
+            .expect("inline protocol version lock poisoned")
+            .is_some()
     }
 
     pub fn is_transport_closed(&self) -> bool {
```
