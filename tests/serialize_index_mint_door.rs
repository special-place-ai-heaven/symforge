//! GOAL_SERIALIZE_INDEX_MINT_DOOR — DROP-PUB proof.
//!
//! `serialize_index` captured a Complete+None snapshot (manifest absent) onto
//! `index.bin` without the Ada-B gate. Production must use
//! `checkpoint_shared_index` / `serialize_shared_index` instead.

#[test]
fn serialize_index_is_not_public_overwrite_entry() {
    let source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/live_index/persist.rs"
    ));
    assert!(
        !source.contains("pub fn serialize_index("),
        "serialize_index must not be pub — external callers would bypass the Ada-B gate"
    );
    assert!(
        !source.contains("pub(crate) fn serialize_index("),
        "serialize_index must not be crate-visible — use checkpoint_shared_index in production"
    );
}
