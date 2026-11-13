// SYMFORGE-PATCH: upstream's build script ran `git config core.hooksPath .githooks`
// on the directory two levels up, which for this vendored copy is the symforge
// repo root. A dependency must not rewrite its host repo's git config, so the
// script is a no-op here. It emitted no cargo directives, so nothing else changes.
fn main() {}
