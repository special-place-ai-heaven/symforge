use std::cmp::max;

pub fn target_fn(x: u32) -> u32 {
    step_e(step_d(step_c(step_b(step_a(x)))))
}

pub fn calls_locally() -> u32 {
    max(target_fn(10), 1)
}

pub fn shadowed() -> u32 {
    let target_fn = |v: u32| v * 2;
    target_fn(5)
}

fn step_a(x: u32) -> u32 {
    x + 1
}

fn step_b(x: u32) -> u32 {
    x + 2
}

fn step_c(x: u32) -> u32 {
    x + 3
}

fn step_d(x: u32) -> u32 {
    x + 4
}

fn step_e(x: u32) -> u32 {
    x + 5
}
