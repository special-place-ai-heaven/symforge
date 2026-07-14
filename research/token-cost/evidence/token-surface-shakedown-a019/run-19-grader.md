1. Environment variable: `SYMFORGE_SURFACE`. It is read by `surface_profile_from_env()` using `std::env::var` at [surface_probe.rs:26](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:26) and [surface_probe.rs:33](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:33).

2. Default profile: `Full`. Unset, invalid, and explicit `full` values all reach the catch-all `SurfaceProfile::Full` branch at [surface_probe.rs:37](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:37)–[surface_probe.rs:40](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:40).

3. Compact profile tools, in order:

   - `symforge`
   - `symforge_edit`
   - `status`

   The canonical names are declared at [surface.rs:3](<temp>/symforge-token-shakedown-a10ff102/src/stel/surface.rs:3)–[surface.rs:8](<temp>/symforge-token-shakedown-a10ff102/src/stel/surface.rs:8), with the enum-to-string mapping at [surface.rs:22](<temp>/symforge-token-shakedown-a10ff102/src/stel/surface.rs:22)–[surface.rs:27](<temp>/symforge-token-shakedown-a10ff102/src/stel/surface.rs:27).

4. Relevant functions:

   - `surface_profile_from_env()` reads and parses the environment: [surface_probe.rs:26](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:26).
   - The production MCP `list_tools()` handler chooses the list: it gets the profile, routes compact to `compact_surface_tools()`, and delegates other profiles to `list_tools_for_profile()` at [mod.rs:1335](<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:1335)–[mod.rs:1349](<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:1349).
   - `list_tools_for_profile()` contains the general `Full`/`Compact`/`Meta` profile switch at [surface_probe.rs:167](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:167)–[surface_probe.rs:176](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:176).
   - `compact_surface_tools()` constructs the production three-tool vector at [surface_list.rs:36](<temp>/symforge-token-shakedown-a10ff102/src/stel/surface_list.rs:36)–[surface_list.rs:55](<temp>/symforge-token-shakedown-a10ff102/src/stel/surface_list.rs:55).

No files were changed, and no builds or tests were run.