1. The environment variable is `SYMFORGE_SURFACE`. It is read by `surface_profile_from_env()` using `std::env::var` ([surface_probe.rs:26](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:26), [line 33](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:33)).

2. The default profile is `Full`. Unset or unrecognized values reach the `_ => SurfaceProfile::Full` fallback ([surface_probe.rs:37](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:37)).

3. The compact profile contains exactly:

   - `symforge`
   - `symforge_edit`
   - `status`

   These are declared canonically in `COMPACT_TOOL_NAMES` ([surface.rs:7](/<temp>/symforge-token-shakedown-a10ff102/src/stel/surface.rs:7)) and mapped from the compact-tool enum at [surface.rs:22](/<temp>/symforge-token-shakedown-a10ff102/src/stel/surface.rs:22).

4. The production function chain is:

   - `surface_profile_from_env()` reads and parses the environment ([surface_probe.rs:26](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:26)).
   - `ServerHandler::list_tools()` chooses the profile-specific list: compact calls `crate::stel::compact_surface_tools()`; all other profiles go through `surface_probe::list_tools_for_profile(profile)` ([mod.rs:1335](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:1335), [lines 1340–1344](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:1340)).
   - `compact_surface_tools()` constructs the production three-element `Vec<Tool>` ([surface_list.rs:36](/<temp>/symforge-token-shakedown-a10ff102/src/stel/surface_list.rs:36)).
   - `list_tools_for_profile()` is the secondary dispatcher for full/meta lists; although it retains a compact probe branch, production compact selection bypasses it in `ServerHandler::list_tools()` ([surface_probe.rs:167](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:167)).

No files were changed, and no builds or tests were run.