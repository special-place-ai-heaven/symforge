//! Frozen fixture. `batch_rename` target: `tally` is defined once and called from
//! three sites. The harness dry-run-renames it and checks the tool reports the def
//! PLUS all three call sites — a dropped call site is a real bug.

/// Sum a slice. Renamed by the harness; must update every caller.
pub fn tally(xs: &[i64]) -> i64 {
    xs.iter().sum()
}

pub fn double_sum(xs: &[i64]) -> i64 {
    tally(xs) + tally(xs)
}

pub fn describe(xs: &[i64]) -> String {
    format!("total={}", tally(xs))
}
