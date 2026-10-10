# Embedded room consumer

This separate Cargo package depends on SymForge with `default-features = false` and `features = ["embed"]`. It opens an admitted local source through the public library API, sends a serialized room request, and checks a full query receipt. It starts no MCP server or daemon.

From the repository root:

```text
cargo run --manifest-path examples/embed_room_consumer/Cargo.toml -- . Cargo.toml
```

The first argument is the trusted local source root. The optional second argument is a repository-relative file to query after the source reaches `Current`. A real room broker constructs `HostRoomGrant` from its authenticated source selection and retains the opaque grant outside the wire request.
