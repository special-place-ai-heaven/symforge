use std::cmp::max;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs::read_to_string;

fn load(path: &str) -> String {
    let text = read_to_string(path);
    text.unwrap()
}

fn build_map() -> HashMap<String, i32> {
    HashMap::new()
}

fn build_tree() -> BTreeMap<String, i32> {
    BTreeMap::new()
}

fn pick() -> i32 {
    max(1, 2)
}
