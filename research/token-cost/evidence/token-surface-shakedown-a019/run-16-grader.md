1. Environment variable: `SYMFORGE_SURFACE`, read via `std::env::var` in [`surface_profile_from_env()`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:26), specifically at [line 33](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:33).

2. Default profile: `Full`. Unset, `full`, and unrecognized values reach the fallback `_ => SurfaceProfile::Full` at [line 39](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:39). The preceding comment explicitly identifies full as the default at [line 27](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:27).

3. Compact-profile tools, exactly:

   - `symforge`
   - `symforge_edit`
   - `status`

   The canonical array is defined in [`src/stel/surface.rs:7`](/<temp>/symforge-token-shakedown-a10ff102/src/stel/surface.rs:7), with the literal names on [line 8](/<temp>/symforge-token-shakedown-a10ff102/src/stel/surface.rs:8).

4. Relevant functions:

   - `surface_profile_from_env()` reads and interprets the environment: [`surface_probe.rs:26`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:26).
   - `SymForgeServer::list_tools()` selects the profile-specific list, routing compact to `compact_surface_tools()` and other profiles to `list_tools_for_profile()`: [`protocol/mod.rs:1335`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:1335), especially [lines 1340–1343](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:1340).
   - `compact_surface_tools()` constructs the production compact list: [`stel/surface_list.rs:37`](/<temp>/symforge-token-shakedown-a10ff102/src/stel/surface_list.rs:37). Its three entries are assembled on lines 38–54.