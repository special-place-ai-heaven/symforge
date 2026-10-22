# Follow-up: inserting before a Rust function steals its outer attributes

Status: open; confirmed during SymForge v11.9.1 dogfooding on 2026-10-20.

`insert_symbol(position="before")` anchored at the function item after its existing outer `#[cfg(feature = "embed")]` attribute. The new item was inserted between the attribute and the original function, so the attribute guarded the new item and the original function lost its feature boundary.

Observed on `EmbeddedSourceHandle::query` and `capture_query_snapshot` in `src/index_lifecycle/embedded.rs`. The default server-only build then failed with E0433 because the original methods referenced embed-only modules without their guards. The engineer confirmed the operation sequence and restored the original guards in the AAP adaptation.

Fix the insertion anchor to preserve outer attributes and owned documentation on the original item. Do not absorb module-level inner attributes. Add a structural-edit regression with an attributed function and assert that both item ownership and feature-specific compilation remain correct after insertion. Check derive/allow/inline attribute handling with the same ownership rule where applicable.

Workaround: inspect attributes around an insertion target and verify every supported feature cell; do not assume a function body range includes its outer attributes.
