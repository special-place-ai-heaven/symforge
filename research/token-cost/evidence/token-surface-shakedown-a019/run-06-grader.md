1. The environment variable is `SYMFORGE_SURFACE`, read by `surface_profile_from_env()` at [surface_probe.rs:26](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:26>) and specifically accessed at [surface_probe.rs:33](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:33>).

2. The default profile is `Full`. Unset, invalid, or explicit `full` values reach the fallback at [surface_probe.rs:37](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:37>)–[39](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:39>).

3. The compact profile exposes exactly:

   - `symforge`
   - `symforge_edit`
   - `status`

   The canonical array is defined at [surface.rs:7](</<temp>/symforge-token-shakedown-a10ff102/src/stel/surface.rs:7>)–[8](</<temp>/symforge-token-shakedown-a10ff102/src/stel/surface.rs:8>).

4. The function chain is:

   - `surface_profile_from_env()` reads and parses the environment: [surface_probe.rs:26](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:26>).
   - `ServerHandler for SymForgeServer::list_tools()` chooses the production tool list: compact calls `compact_surface_tools()`, while other profiles delegate to `list_tools_for_profile()`: [mod.rs:1335](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:1335>)–[1344](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:1344>).
   - `list_tools_for_profile()` is the shared Full/Compact/Meta selection helper: [surface_probe.rs:167](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:167>)–[176](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:176>).
   - `compact_surface_tools()` constructs the production three-tool vector: [surface_list.rs:37](</<temp>/symforge-token-shakedown-a10ff102/src/stel/surface_list.rs:37>)–[54](</<temp>/symforge-token-shakedown-a10ff102/src/stel/surface_list.rs:54>).

No files were changed, and no builds or tests were run.