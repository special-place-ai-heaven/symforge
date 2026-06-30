fn helper(x: i32) -> i32 {
    x + 1
}

fn triple(x: i32) -> i32 {
    helper(x) * 3
}

fn double(x: i32) -> i32 {
    let a = triple(x);
    a + a
}

struct Calc {
    base: i32,
}

impl Calc {
    fn new(base: i32) -> Calc {
        Calc { base }
    }

    fn compute(&self) -> i32 {
        double(self.base)
    }
}

fn run() -> i32 {
    let c = Calc::new(10);
    c.compute()
}
