//! The namespace profile text is derived from the bounds it names.
//!
//! [`PROFILE_DESCRIPTION`] is hashed into the profile identity, so the numbers in
//! its text *are* the accepted partition bounds of the compact scoped-inline
//! profile. Nothing else keeps them in step: this case rebuilds the text from the
//! declarations in `limits`, the inode-value grammar and the identity width, and
//! fails when a bound moves without the profile moving with it.

use layerfs_content::filesystem::limits::{
    MAXIMUM_INODE_BRANCH_CHILDREN, MAXIMUM_INODE_LEAF_ROWS, MAXIMUM_PAGE_BYTES, MAXIMUM_TREE_LEVEL,
    MINIMUM_INODE_BRANCH_CHILDREN, MINIMUM_INODE_LEAF_ROWS,
};
use layerfs_content::filesystem::root::{profile_id, FilesystemRoot, PROFILE_DESCRIPTION};
use layerfs_content::object::inode_leaf::LEAF_ROW_BYTES;
use layerfs_content::{ContentError, InodeScope, ObjectId, DIGEST_BYTES};

/// The two numbers with no named declaration of their own: the stored serial
/// width, and the 2/5 fill rule the directory and attribute pages are held to.
const SERIAL_BYTES: usize = 8;
const FILL_NUMERATOR: usize = 2;
const FILL_DENOMINATOR: usize = 5;

#[test]
fn the_profile_text_names_the_bounds_it_is_hashed_with() {
    let expected = format!(
        "layerfs/namespace-profile/scoped-inline/v1\0\
         scope{DIGEST_BYTES};serial{SERIAL_BYTES};inode{LEAF_ROW_BYTES};\
         leaf{MINIMUM_INODE_LEAF_ROWS}-{MAXIMUM_INODE_LEAF_ROWS};\
         branch{MINIMUM_INODE_BRANCH_CHILDREN}-{MAXIMUM_INODE_BRANCH_CHILDREN};\
         page{MAXIMUM_PAGE_BYTES};depth{MAXIMUM_TREE_LEVEL};\
         directory-fill{FILL_NUMERATOR}/{FILL_DENOMINATOR}"
    );
    assert_eq!(
        std::str::from_utf8(PROFILE_DESCRIPTION).expect("the profile text is text"),
        expected,
        "the namespace profile text and the accepted bounds disagree"
    );
    // The identity is the hash of that text and of nothing else, so a bound that
    // moves the text moves the accepted profile with it.
    assert_eq!(profile_id(), ObjectId::for_bytes(PROFILE_DESCRIPTION));
}

#[test]
fn a_foreign_profile_is_refused_before_a_root_exists() {
    // Not tautological: the constructor is handed a foreign identity and the
    // refusal names the profile, so the check is on the value and not on a field
    // that was just written from the same constant.
    let foreign = ObjectId::for_bytes(b"layerfs/namespace-profile/other/v1\0");
    let scope = InodeScope::from_object(ObjectId::for_bytes(b"scope"));
    let table = ObjectId::for_bytes(b"table");
    assert!(matches!(
        FilesystemRoot::new(foreign, scope, 1, table),
        Err(ContentError::UnsupportedProfile { .. })
    ));
    let root = FilesystemRoot::new(profile_id(), scope, 1, table).expect("the frozen profile");
    assert_eq!(root.profile(), profile_id());
    assert_eq!(root.references(), [table]);
}
