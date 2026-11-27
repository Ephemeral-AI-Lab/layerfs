//! A name in a fix line is one shell word: as it is when a shell takes it
//! literally, else single-quoted (LF2 review: the fork fix of
//! `sandbox create --mount`).
use layerfs_home::reply::outcome::shell_word;

#[test]
fn a_name_is_one_shell_word() {
    assert_eq!(shell_word("feature-1.x"), "feature-1.x");
    assert_eq!(shell_word("shop/api"), "shop/api");
    assert_eq!(shell_word("no such"), "'no such'");
    assert_eq!(shell_word("it's"), "'it'\\''s'");
    assert_eq!(shell_word("$(rm)"), "'$(rm)'");
    assert_eq!(shell_word(""), "''");
}
