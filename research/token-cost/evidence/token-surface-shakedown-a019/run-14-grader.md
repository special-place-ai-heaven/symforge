1. The controlling environment variable is `SYMFORGE_SURFACE`. `surface_profile_from_env()` reads it at [src/protocol/surface_probe.rs:33](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:33).

2. The default profile is `Full`. Unset or unrecognized values fall through to `SurfaceProfile::Full` at [src/protocol/surface_probe.rs:37](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:37).

3. The compact profile contains exactly:

   - `symforge`
   - `symforge_edit`
   - `status`

   The canonical three-name array is defined at [src/stel/surface.rs:7](/<temp>/symforge-token-shakedown-a10ff102/src/stel/surface.rs:7).

4. The relevant functions are:

   - `surface_profile_from_env()` — reads the environment and resolves `Compact`, `Meta`, or default `Full`: [src/protocol/surface_probe.rs:26](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:26).
   - `ServerHandler::list_tools()` — production profile-specific chooser; compact calls `compact_surface_tools()`, while other profiles delegate to `list_tools_for_profile()`: [src/protocol/mod.rs:1335](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:1335).
   - `list_tools_for_profile()` — maps the non-production-probe profile variants to their corresponding lists and constructs the full list from the router: [src/protocol/surface_probe.rs:167](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:167).
   - `compact_surface_tools()` — constructs the production compact three-tool `Vec<Tool>`: [src/stel/surface_list.rs:37](/<temp>/symforge-token-shakedown-a10ff102/src/stel/surface_list.rs:37).

No files were changed, and no builds or tests were run.