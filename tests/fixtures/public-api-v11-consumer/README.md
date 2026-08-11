# V11 public API consumer fixtures

This directory is the pre-activation input corpus for Feature 020 task T006. It
is projected from the closed allowlist in
`specs/020-repository-knowledge-index/contracts/public-api-v11.json`.

The corpus intentionally does **not** assert that the V11 Rust API is currently
exported or compilable. Product exposure is forbidden until the refreeze approval
gate and later activation slices complete. Accordingly:

- `fixture-manifest.json` records all execution-dependent facts as `null`;
- `graph-cover.json` enumerates the 26 supported target/feature cells without
  claiming a rustdoc graph or BDD proof exists;
- `all-cfg/` is a synthetic completeness crate for extractor testing, not a
  SymForge product API;
- `dependent-positive/` names the future allowlisted external API from a separate
  crate and is first expected to compile against the dark adapter in T049;
- `compile-fail/` contains external-consumer templates and a closed case catalog.
  A later harness must materialize one temporary crate per atomic case so one
  compiler error cannot mask another. Its `From<Probe>` and `Borrow<Probe>` cases
  are deliberately non-reflexive: Rust's unavoidable upstream `From<Self>` and
  `Borrow<Self>` blanket edges are the only permitted family exceptions. The
  higher-ranked `Deserialize` compiler probe is also only a non-exhaustive witness:
  a lifetime-specific implementation such as `Deserialize<'static>` is excluded by
  the exhaustive public-item graph, not by that one compiler failure.

No Cargo, build, rustdoc, Clippy, or formatting command was run while creating
this pre-activation corpus. The first legitimate compiler evidence belongs to the
slice that introduces the dark adapter; release evidence belongs to T086.
