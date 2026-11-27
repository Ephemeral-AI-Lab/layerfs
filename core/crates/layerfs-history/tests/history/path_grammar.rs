//! The `project.remove_file` path grammar and its keys, read through the
//! public contract. Moved out of `contract/path.rs`: product source holds no
//! tests (the product-boundary guard).

use layerfs_history::catalog::path::{
    governs, path_key, trace_key, NormalizedPath, MAXIMUM_PATH_BYTES, MAXIMUM_PATH_COMPONENTS,
};
use layerfs_history::LayerStackId;

#[test]
fn accepts_one_leading_slash_and_relative_paths() {
    assert_eq!(NormalizedPath::new("/a").unwrap().as_str(), "a");
    assert_eq!(NormalizedPath::new("a").unwrap().as_str(), "a");
    assert_eq!(NormalizedPath::new("/a/b").unwrap().as_str(), "a/b");
    assert_eq!(NormalizedPath::new("a-b_c.d").unwrap().as_str(), "a-b_c.d");
}

#[test]
fn refuses_the_bad_grammar() {
    for text in [
        "", "/", "//a", "a//b", "a/", "./a", "a/./b", ".", "..", "/./a", "a\\b", "a\0b",
    ] {
        assert!(NormalizedPath::new(text).is_err(), "accepted {text:?}");
    }
}

#[test]
fn refuses_long_paths_and_many_components() {
    assert!(NormalizedPath::new(&"a".repeat(MAXIMUM_PATH_BYTES)).is_ok());
    assert!(NormalizedPath::new(&"a".repeat(MAXIMUM_PATH_BYTES + 1)).is_err());
    let many = vec!["a"; MAXIMUM_PATH_COMPONENTS].join("/");
    assert!(NormalizedPath::new(&many).is_ok());
    let more = vec!["a"; MAXIMUM_PATH_COMPONENTS + 1].join("/");
    assert!(NormalizedPath::new(&more).is_err());
}

#[test]
fn subtree_matches_only_at_a_slash_boundary() {
    assert!(governs(b"a", b"a"));
    assert!(governs(b"a", b"a/b"));
    assert!(!governs(b"a", b"ab"));
    assert!(!governs(b"ab", b"a"));
    assert!(governs(b"a/b", b"a/b/c"));
    assert!(!governs(b"a/b", b"a/bc"));
}

#[test]
fn keys_are_stable_and_salted_traces_differ() {
    let path = NormalizedPath::new("/a/b").unwrap();
    assert_eq!(path.key(), path_key(b"a/b"));
    let stack = LayerStackId::from_authority([7; 16]);
    let first = trace_key(&[1; 32], stack, path.as_bytes());
    let second = trace_key(&[2; 32], stack, path.as_bytes());
    assert_ne!(first, second);
}
