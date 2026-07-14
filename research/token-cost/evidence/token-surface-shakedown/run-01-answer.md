1. Environment variable: `SYMFORGE_SURFACE`, read by `surface_profile_from_env()` at [src/protocol/surface_probe.rs:33](</C:/Users/rakovnik/AppData/Local/Temp/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:33>).

2. Default profile: `Full`. Unset, `full`, or unrecognized values reach the fallback arm returning `SurfaceProfile::Full` at [src/protocol/surface_probe.rs:39](</C:/Users/rakovnik/AppData/Local/Temp/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:39>). The unset behavior is also asserted at [tests/surface_default.rs:73](</C:/Users/rakovnik/AppData/Local/Temp/symforge-token-shakedown-a10ff102/tests/surface_default.rs:73>).

3. Compact profile tools, exactly:

   - `symforge`
   - `symforge_edit`
   - `status`

   The canonical array is defined at [src/stel/surface.rs:7](</C:/Users/rakovnik/AppData/Local/Temp/symforge-token-shakedown-a10ff102/src/stel/surface.rs:7>).

4. Function flow:

   - `surface_profile_from_env()` reads and interprets the environment: [src/protocol/surface_probe.rs:26](</C:/Users/rakovnik/AppData/Local/Temp/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:26>).
   - `ServerHandler::list_tools()` performs the production MCP `tools/list` selection. It calls the environment reader and routes compact to the production constructor: [src/protocol/mod.rs:1335](</C:/Users/rakovnik/AppData/Local/Temp/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:1335>), specifically [line 1342](</C:/Users/rakovnik/AppData/Local/Temp/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:1342>).
   - `list_tools_for_profile()` contains the general `Full`/`Compact`/`Meta` profile mapping used by other callers and by production for non-compact profiles: [src/protocol/surface_probe.rs:167](</C:/Users/rakovnik/AppData/Local/Temp/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:167>).
   - `compact_surface_tools()` constructs the production three-tool compact list: [src/stel/surface_list.rs:37](</C:/Users/rakovnik/AppData/Local/Temp/symforge-token-token-shakedown-a10ff102/src/stel/surface_list.rs:37>).

No files were changed, and no builds or tests were run.