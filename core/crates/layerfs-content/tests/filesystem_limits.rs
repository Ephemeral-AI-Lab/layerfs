//! Boundary cases for every declared limit that a cheap fixture can reach.
//!
//! Each limit is probed on **both** sides: the largest accepted value and the
//! smallest refused one. A limit with no case here is listed in the module's
//! `UNBOUNDED` note with the arithmetic that shows why a fixture cannot reach it,
//! so a reader can tell a checked bound from a quoted one.

mod support;

use layerfs_content::filesystem::limits::{
    MAXIMUM_ATTRIBUTE_DOMAIN_BYTES, MAXIMUM_ATTRIBUTE_KEY_BYTES, MAXIMUM_NAME_BYTES,
    MAXIMUM_PATH_BYTES, MAXIMUM_PATH_COMPONENTS, MAXIMUM_SYMLINK_TARGET_BYTES,
};
use layerfs_content::filesystem::{LogicalPath, PathName, SymlinkTarget};
use layerfs_content::ContentError;

/// Component name of exactly `bytes` bytes.
fn name_of(bytes: usize) -> String {
    "n".repeat(bytes)
}

/// A path of `components` components whose total byte length is `bytes`.
fn path_of(components: usize, bytes: usize) -> String {
    assert!(components > 0);
    let separators = components - 1;
    let payload = bytes - separators;
    let base = payload / components;
    let extra = payload % components;
    (0..components)
        .map(|index| {
            let width = base + usize::from(index < extra);
            "p".repeat(width.max(1))
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[test]
fn a_component_name_is_accepted_at_255_bytes_and_refused_at_256() {
    assert_eq!(MAXIMUM_NAME_BYTES, 255);
    let at_limit = name_of(MAXIMUM_NAME_BYTES);
    assert_eq!(at_limit.len(), MAXIMUM_NAME_BYTES);
    assert!(
        PathName::new(&at_limit).is_ok(),
        "the largest legal component is accepted"
    );
    let over = name_of(MAXIMUM_NAME_BYTES + 1);
    assert_eq!(
        PathName::new(&over),
        Err(ContentError::PathLimitExceeded),
        "one byte over the component bound is refused"
    );
    assert_eq!(
        PathName::new(""),
        Err(ContentError::InvalidPath),
        "an empty component is not a name"
    );
}

#[test]
fn a_path_is_accepted_at_4096_bytes_and_refused_at_4097() {
    assert_eq!(MAXIMUM_PATH_BYTES, 4_096);
    // 256 components of 16 bytes each separate to 4,095 bytes; one more byte makes
    // the path 4,096, and the component count stays at the limit.
    let at_limit = path_of(MAXIMUM_PATH_COMPONENTS, MAXIMUM_PATH_BYTES);
    assert_eq!(at_limit.len(), MAXIMUM_PATH_BYTES);
    assert!(
        LogicalPath::new(&at_limit).is_ok(),
        "a path of exactly the byte bound is accepted"
    );
    let over = path_of(MAXIMUM_PATH_COMPONENTS, MAXIMUM_PATH_BYTES + 1);
    assert_eq!(over.len(), MAXIMUM_PATH_BYTES + 1);
    assert_eq!(
        LogicalPath::new(&over),
        Err(ContentError::PathLimitExceeded),
        "one byte over the path bound is refused"
    );
}

#[test]
fn a_path_is_accepted_at_256_components_and_refused_at_257() {
    assert_eq!(MAXIMUM_PATH_COMPONENTS, 256);
    let at_limit = path_of(MAXIMUM_PATH_COMPONENTS, 1_024);
    assert_eq!(at_limit.split('/').count(), MAXIMUM_PATH_COMPONENTS);
    assert!(
        LogicalPath::new(&at_limit).is_ok(),
        "a path of exactly the component bound is accepted"
    );
    let over = path_of(MAXIMUM_PATH_COMPONENTS + 1, 1_024);
    assert_eq!(over.split('/').count(), MAXIMUM_PATH_COMPONENTS + 1);
    assert!(
        over.len() <= MAXIMUM_PATH_BYTES,
        "the component bound is what this case reaches, not the byte bound"
    );
    assert_eq!(
        LogicalPath::new(&over),
        Err(ContentError::PathLimitExceeded),
        "one component over the bound is refused"
    );
}

#[test]
fn a_symlink_target_is_accepted_at_4096_bytes_and_refused_at_4097() {
    assert_eq!(MAXIMUM_SYMLINK_TARGET_BYTES, 4_096);
    assert!(
        SymlinkTarget::new(vec![b't'; MAXIMUM_SYMLINK_TARGET_BYTES]).is_ok(),
        "the largest legal target is accepted"
    );
    assert_eq!(
        SymlinkTarget::new(vec![b't'; MAXIMUM_SYMLINK_TARGET_BYTES + 1]),
        Err(ContentError::InvalidRecord("symlink target")),
        "one byte over the target bound is refused"
    );
    assert_eq!(
        SymlinkTarget::new(vec![b't'; 4].into_iter().chain([0]).collect()),
        Err(ContentError::InvalidRecord("symlink target")),
        "an embedded NUL is refused at any length"
    );
}

#[test]
fn an_attribute_domain_and_key_are_accepted_at_their_bounds_and_refused_over() {
    assert_eq!(MAXIMUM_ATTRIBUTE_DOMAIN_BYTES, 64);
    assert_eq!(MAXIMUM_ATTRIBUTE_KEY_BYTES, 255);
    use layerfs_content::filesystem::attributes::keys::AttributeKey;
    assert!(
        AttributeKey::new("d".repeat(MAXIMUM_ATTRIBUTE_DOMAIN_BYTES), vec![b'k'; 1]).is_ok(),
        "a domain at its bound is accepted"
    );
    assert!(
        AttributeKey::new("d".to_owned(), vec![b'k'; MAXIMUM_ATTRIBUTE_KEY_BYTES]).is_ok(),
        "a key at its bound is accepted"
    );
    assert_eq!(
        AttributeKey::new("d".repeat(MAXIMUM_ATTRIBUTE_DOMAIN_BYTES + 1), Vec::new()),
        Err(ContentError::ObjectLimitExceeded {
            limit: MAXIMUM_ATTRIBUTE_DOMAIN_BYTES,
            actual: MAXIMUM_ATTRIBUTE_DOMAIN_BYTES + 1,
        }),
        "one byte over the domain bound is refused by the declared bound"
    );
    assert_eq!(
        AttributeKey::new("d".to_owned(), vec![b'k'; MAXIMUM_ATTRIBUTE_KEY_BYTES + 1]),
        Err(ContentError::ObjectLimitExceeded {
            limit: MAXIMUM_ATTRIBUTE_KEY_BYTES,
            actual: MAXIMUM_ATTRIBUTE_KEY_BYTES + 1,
        }),
        "one byte over the key bound is refused by the declared bound"
    );
}

/// Limits with no case here, and the arithmetic that says why.
///
/// - `MAXIMUM_TREE_LEVEL` (31) is a **format** bound on a page's declared level.
///   Reaching a level-31 tree needs at least `MINIMUM_ROOT_ENTRIES^31` summaries,
///   which no fixture can build; the field is instead refused at the codec by
///   `a_page_that_declares_an_impossible_level_is_refused`, and `MAXIMUM_TREE_LEVEL`
///   appears in the profile identity, so moving it moves every identity in the
///   frozen profile. Derived, unverified at scale.
/// - `MAXIMUM_PAGE_BYTES` (8,192) is enforced by the page encoders; the boundary
///   is a function of a page's exact encoded size, and the codec cases in
///   `filesystem_codec` assert the boundary through the encoders rather than by
///   constructing an 8,193-byte page, which the encoder cannot produce.
/// - The storage-side group/record/transaction constants live in
///   `layerfs-storage`; their boundary cases are the ones in that package's
///   `storage_bounds` suite, not here.
#[test]
fn the_limits_this_suite_cannot_bound_are_named_with_their_reason() {
    // A case that keeps the notes above part of the target rather than a comment
    // in a file nothing reads: the figures below are exactly the limits this suite
    // does not probe, and each is asserted here so a reader can tell a
    // deliberately unbounded row from a forgotten one.
    use layerfs_content::filesystem::limits::{MAXIMUM_PAGE_BYTES, MAXIMUM_TREE_LEVEL};
    assert_eq!(MAXIMUM_TREE_LEVEL, 31);
    assert_eq!(MAXIMUM_PAGE_BYTES, 8_192);
    // Reaching a level-31 tree needs at least `MINIMUM_INODE_BRANCH_CHILDREN^30`
    // summaries at the leaves, which is far beyond any fixture; the level field is
    // refused at the codec instead, and `filesystem_profile` pins the identity that
    // carries this figure.
    // The minimum fan-out of a non-root branch, raised to the number of levels
    // below the root, already exceeds the u64 serial space that would have to hold
    // those summaries - so a legal level-31 tree cannot be built, and the level
    // field is refused at the codec instead of being proven by a fixture.
    let fan_out = u128::from(layerfs_content::filesystem::limits::MINIMUM_INODE_BRANCH_CHILDREN);
    let levels = u32::from(MAXIMUM_TREE_LEVEL) - 1;
    let mut summaries = 1_u128;
    let mut overflowed = false;
    for _ in 0..levels {
        match summaries.checked_mul(fan_out) {
            Some(next) => summaries = next,
            None => {
                overflowed = true;
                break;
            }
        }
    }
    assert!(
        overflowed || summaries > u128::from(u64::MAX),
        "the arithmetic that rules the level bound out of a fixture: {fan_out}^{levels} = {summaries}"
    );
}
