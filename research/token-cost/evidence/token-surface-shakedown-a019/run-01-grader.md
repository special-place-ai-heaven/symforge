1. Environment variable: `SYMFORGE_SURFACE`. `surface_profile_from_env()` reads it with `std::env::var` at [src/protocol/surface_probe.rs:26](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:26) and [line 33](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:33).

2. Default profile: `Full`. Only `compact` and `meta` receive explicit match arms; missing, `full`, or unrecognized values reach `_ => SurfaceProfile::Full` at [src/protocol/surface_probe.rs:37](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:37).

3. Compact profile tools, in order:

   - `symforge`
   - `symforge_edit`
   - `status`

   The canonical array is defined at [src/stel/surface.rs:7](<temp>/symforge-token-shakedown-a10ff102/src/stel/surface.rs:7).

4. Relevant functions:

   - `surface_profile_from_env()` reads and interprets the environment: [src/protocol/surface_probe.rs:26](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:26).
   - `SymForgeServer::list_tools()` selects the profile-specific list. Compact calls `compact_surface_tools()`; other profiles call `list_tools_for_profile(profile)`: [src/protocol/mod.rs:1335](<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:1335), especially [line 1340](<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:1340).
   - `list_tools_for_profile()` dispatches the non-production-compact/profile helper lists for `Full`, `Compact`, and `Meta`: [src/protocol/surface_probe.rs:167](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:167).
   - `compact_surface_tools()` constructs the production three-tool `Vec<Tool>`: [src/stel/surface_list.rs:37](<temp>/symforge-token-shakedown-a10ff102/src/stel/surface_list.rs:37). Its enum-to-name mapping is explicit at [src/stel/surface.rs:22](<temp>/symforge-token-shakedown-a10ff102/src/stel/surface.rs:22).

No files were changed, and no builds or tests were run.