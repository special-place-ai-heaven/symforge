use globset::{Glob, GlobSetBuilder};

#[test]
fn bench_first_match() {
    let mut b = GlobSetBuilder::new();
    b.add(Glob::new("*.rs").unwrap());
    b.add(Glob::new("*.toml").unwrap());
    b.add(Glob::new("src/*").unwrap());
    let set = b.build().unwrap();
    assert_eq!(set.first_match("src/lib.rs"), Some(0));
    assert_eq!(set.first_match("Cargo.toml"), Some(1));
    assert_eq!(set.first_match("src/README"), Some(2));
    assert_eq!(set.first_match("README"), None);
}
