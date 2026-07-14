1. Environment variable: `SYMFORGE_SURFACE`. It is read by `surface_profile_from_env()` using `std::env::var(...)` at [surface_probe.rs:26](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:26) and [surface_probe.rs:33](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:33).

2. Default profile: `Full`. Missing, explicit `full`, or otherwise unrecognized values fall through to `SurfaceProfile::Full` at [surface_probe.rs:37](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:37).

3. Compact profile tools, exactly:

   - `symforge`
   - `symforge_edit`
   - `status`

   The canonical array is defined at [surface.rs:7](<temp>/symforge-token-shakedown-a10ff102/src/stel/surface.rs:7), with the enum-to-string mapping at [surface.rs:22](<temp>/symforge-token-shakedown-a10ff102/src/stel/surface.rs:22).

4. Function chain:

   - Reads environment: `surface_profile_from_env()` — [surface_probe.rs:26](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:26).
   - Chooses the production profile-specific list: `<SymForgeServer as ServerHandler>::list_tools()` — it reads the profile and selects `compact_surface_tools()` for compact, otherwise `list_tools_for_profile(profile)`, at [protocol/mod.rs:1335](<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:1335).
   - Constructs the production compact list: `compact_surface_tools()` — [surface_list.rs:37](<temp>/symforge-token-shakedown-a10ff102/src/stel/surface_list.rs:37).
   - Supporting non-compact/profile helper: `list_tools_for_profile()` matches `Full`, `Compact`, and `Meta` at [surface_probe.rs:167](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:167). The production handler deliberately bypasses its compact probe branch in favor of `compact_surface_tools()`.

No files were changed, and no builds or tests were run.