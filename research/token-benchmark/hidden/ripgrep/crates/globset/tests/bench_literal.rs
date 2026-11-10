use globset::{Glob, GlobSetBuilder};

#[test]
fn bench_literal() {
    assert!(Glob::new("Cargo.toml").unwrap().is_literal());
    assert!(!Glob::new("*.rs").unwrap().is_literal());
    assert!(!Glob::new("src/{a,b}").unwrap().is_literal());
    assert!(!Glob::new("file?.txt").unwrap().is_literal());
    let mut b = GlobSetBuilder::new();
    for g in ["Cargo.toml", "*.rs", "README.md"] {
        b.add(Glob::new(g).unwrap());
    }
    assert_eq!(b.build().unwrap().literal_count(), 2);
}
