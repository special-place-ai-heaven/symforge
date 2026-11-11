use crate::other::leaf::other_fn;

pub struct Thing;

impl Thing {
    pub fn target_fn(&self) -> u32 {
        other_fn()
    }
}

// The name target_fn in a comment is not a use.
pub const NAME: &str = "target_fn";

pub fn drive(thing: &Thing) -> u32 {
    thing.target_fn()
}
