//! Frozen fixture. `symforge_edit` REAL-WRITE target: the harness replaces
//! `compute` on disk, snapshots the resulting file, then restores it.
//! If an on-disk edit ever corrupts this file, the snapshot diff catches it.

/// Add two numbers. The harness rewrites this body and checks the file stays valid.
pub fn compute(a: i64, b: i64) -> i64 {
    a + b
}

/// Untouched neighbor — proves the edit does not disturb surrounding symbols.
pub fn neighbor(x: i64) -> i64 {
    x * 2
}
