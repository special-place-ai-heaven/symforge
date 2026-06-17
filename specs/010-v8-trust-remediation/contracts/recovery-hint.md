# Contract: `empty_index_recovery_hint(profile)` (US4)

**Surface**: every empty-index / not-loaded error reachable by an agent (the message
fanned through 37 `loading_guard!` sites + 4 distinct dead-end strings — TR-02, N-5).

## Function
`empty_index_recovery_hint(profile) -> String` — one centralized, surface-aware source.

## Guarantees
1. On the **compact** surface, the hint names ONLY actions callable on that surface:
   re-launch from the project root, or the documented opt-out (`SYMFORGE_SURFACE=full`).
   It MUST NOT name `index_folder` or any gated capability (FR-012, US4 AC-2).
2. On the **full** surface, it MAY name `index_folder`.
3. The hint is computed from the **active** surface, never a fixed string (edge case:
   recovery callable on full but not compact).
4. All 4 dead-end strings + the 37 guard sites route through this one function (N-5) — no
   residual hardcoded "Call index_folder" on a compact-reachable path.

## Regression
`compact_surface_index_not_loaded_message_never_mentions_blocked_tools`: on the compact
profile, assert no empty-index message mentions a tool the compact gate forbids.

## Pairing with cold start (TR-03)
Recovery text is the *fallback*; the primary fix is that a fresh default attach indexes a
discoverable workspace automatically (D5). The hint covers the case where it genuinely
cannot.
