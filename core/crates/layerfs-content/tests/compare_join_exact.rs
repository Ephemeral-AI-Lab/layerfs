//! The decision-spelling twin of `join` (round-6 D1 F2): a name outside
//! UTF-8 refuses `InvalidRecord` instead of collapsing to an empty
//! segment, while the display `join` keeps the listing contract — and
//! the collapsed display spelling is exactly what must never key a
//! decision.
use layerfs_content::compare::tree::{join, join_exact};
use layerfs_content::{ContentError, PathName};

#[test]
fn utf8_names_join_identically_on_both_twins() {
    let name = PathName::from_bytes("中文🚀.txt".as_bytes()).unwrap();
    assert_eq!(join_exact("/dir", &name).unwrap(), join("/dir", &name));
    assert_eq!(join_exact("/", &name).unwrap(), join("/", &name));
}

#[test]
fn a_name_outside_utf8_refuses_the_decision_spelling() {
    let lone = PathName::from_bytes(&[0xED, 0xA0, 0x80][..]).unwrap(); // unpaired high
                                                                       // Display keeps the contract: the segment renders empty.
    assert_eq!(join("/dir", &lone), "/dir/");
    let error = join_exact("/dir", &lone).unwrap_err();
    assert!(
        matches!(error, ContentError::InvalidRecord(_)),
        "the fence refuses, got {error:?}"
    );
    // The harm the fence closes: two distinct names collapse to one
    // display key, so the display spelling can never decide anything.
    let other = PathName::from_bytes(&[0xED, 0xB2, 0x80][..]).unwrap(); // unpaired low
    assert_ne!(lone.as_bytes(), other.as_bytes());
    assert_eq!(join("/dir", &lone), join("/dir", &other));
    assert!(join_exact("/dir", &other).is_err());
}
