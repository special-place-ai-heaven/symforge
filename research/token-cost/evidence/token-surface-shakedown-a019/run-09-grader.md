1. **Environment variable:** `SYMFORGE_SURFACE`. `surface_profile_from_env()` reads it at [src/protocol/surface_probe.rs:26](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:26), specifically via `std::env::var("SYMFORGE_SURFACE")` on line 33.

2. **Default profile:** `Full`. The fallback match arm `_ => SurfaceProfile::Full` is on [src/protocol/surface_probe.rs:39](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:39), covering an unset or unrecognized value.

3. **Compact profile tools:** exactly:

   - `symforge`
   - `symforge_edit`
   - `status`

   The canonical array appears at [src/stel/surface.rs:7](/<temp>/symforge-token-shakedown-a10ff102/src/stel/surface.rs:7), with the three literal names on line 8.

4. **Functions involved:**

   - `surface_profile_from_env()` reads the environment and resolves the profile: [src/protocol/surface_probe.rs:26](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:26).
   - `SymForgeServer::list_tools()` chooses the profile-specific list. Lines 1340–1344 read the profile, route `Compact` to `compact_surface_tools()`, and all other profiles to `list_tools_for_profile(profile)`: [src/protocol/mod.rs:1335](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:1335).
   - `compact_surface_tools()` constructs the compact list from the three compact tool variants: [src/stel/surface_list.rs:37](/<temp>/symforge-token-shakedown-a10ff102/src/stel/surface_list.rs:37). The entries begin on lines 39, 44, and 49.