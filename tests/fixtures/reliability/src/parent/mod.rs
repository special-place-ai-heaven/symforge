pub mod leaf;

pub fn run_parent() -> u32 {
    leaf::target_fn(1) + self::leaf::target_fn(2)
}

pub mod inner {
    pub fn via_super() -> u32 {
        super::leaf::target_fn(3)
    }
}
