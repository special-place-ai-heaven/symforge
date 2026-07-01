use std::collections::HashSet;

struct Bag {
    seen: HashSet<i32>,
}

impl Bag {
    fn new() -> Bag {
        Bag {
            seen: HashSet::new(),
        }
    }

    fn len(&self) -> usize {
        self.seen.len()
    }
}

fn run(bag: &Bag, extra: &[i32]) -> usize {
    let base = bag.len();
    base + extra.len()
}

fn aggregate(items: &[i32]) -> i32 {
    let s = crate::math::sum(items);
    s + helper_total(items)
}
