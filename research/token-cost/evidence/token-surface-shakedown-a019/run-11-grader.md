1. Environment variable: `SYMFORGE_SURFACE`, read by `surface_profile_from_env()` via `std::env::var` at [src/protocol/surface_probe.rs:26](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:26) and [line 33](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:33).

2. Default profile: `SurfaceProfile::Full`. Unset, `"full"`, and unrecognized values reach the fallback arm at [src/protocol/surface_probe.rs:37](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:37)–39.

3. Compact profile tools, exactly:

   - `symforge`
   - `symforge_edit`
   - `status`

   The canonical array is defined at [src/stel/surface.rs:7](<temp>/symforge-token-shakedown-a10ff102/src/stel/surface.rs:7), with the enum-to-string mapping at [line 22](<temp>/symforge-token-shakedown-a10ff102/src/stel/surface.rs:22).

4. Function path:

   - `surface_profile_from_env()` reads and resolves the environment profile: [src/protocol/surface_probe.rs:26](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:26).
   - The production `ServerHandler::list_tools()` implementation chooses the list: compact calls `compact_surface_tools()`; other profiles call `list_tools_for_profile(profile)`: [src/protocol/mod.rs:1335](<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:1335)–1344.
   - `list_tools_for_profile()` is the general profile-selection helper, mapping Full, Compact probe, and Meta lists: [src/protocol/surface_probe.rs:167](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:167).
   - `compact_surface_tools()` constructs the production three-tool list and schemas: [src/stel/surface_list.rs:37](<temp>/symforge-token-shakedown-a10ff102/src/stel/surface_list.rs:37)–55.

No files were changed, and no builds or tests were run.