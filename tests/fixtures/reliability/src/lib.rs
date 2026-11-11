pub mod long;
pub mod other;
pub mod parent;
pub mod short;
pub mod unrelated;
pub mod user;

pub fn top() -> u32 {
    parent::run_parent()
}
