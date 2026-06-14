//! Minimal cfg-if-style corpus for STEL multi-hop golden replay.

/// Named symbol target for `get_symbol` replay (`cfg_if`).
pub fn cfg_if() -> bool {
    true
}

pub fn cfg_if_demo() {
    let _ = cfg_if();
}
