1. The environment variable is `SYMFORGE_SURFACE`, read by `surface_profile_from_env()` via `std::env::var` at [surface_probe.rs:26](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:26) and [surface_probe.rs:33](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:33).

2. The default profile is **Full**. Unset or unrecognized values reach `_ => SurfaceProfile::Full` at [surface_probe.rs:37](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:37)–[39](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:39).

3. The compact profile contains exactly:

   - `symforge`
   - `symforge_edit`
   - `status`

   The canonical names are declared at [surface.rs:6](<temp>/symforge-token-shakedown-a10ff102/src/stel/surface.rs:6)–[8](<temp>/symforge-token-shakedown-a10ff102/src/stel/surface.rs:8).

4. Function flow:

   - `surface_profile_from_env()` reads and interprets the environment: [surface_probe.rs:26](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:26)–[40](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:40).
   - `SymForgeServer`’s `ServerHandler::list_tools()` chooses the production profile-specific list, directly selecting `compact_surface_tools()` for Compact and delegating other profiles to `list_tools_for_profile()`: [mod.rs:1335](<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:1335)–[1344](<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:1344).
   - `list_tools_for_profile()` contains the general Full/Compact/Meta mapping: [surface_probe.rs:167](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:167)–[176](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:176).
   - `compact_surface_tools()` constructs the production three-entry vector: [surface_list.rs:36](<temp>/symforge-token-shakedown-a10ff102/src/stel/surface_list.rs:36)–[55](<temp>/symforge-token-shakedown-a10ff102/src/stel/surface_list.rs:55).

No files were changed, and no builds or tests were run.