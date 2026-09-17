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
///   which no fixture can build; the field is instead refused at the codec - the
///   level tag is rejected with the role by `inode_leaf::malformed_leaves_are_rejected`
///   and `filesystem_codec`'s role/level cases - and `MAXIMUM_TREE_LEVEL`
///   appears in the profile identity, so moving it moves every identity in the
///   frozen profile. Derived, unverified at scale.
/// - `MAXIMUM_LEVELS` (32) is the ordering run store's tier ceiling. Tiers fill
///   like a binary counter's bits - tier k holds the rows of 2^k spilled
///   batches - so all 32 tiers are occupied only after 2^32 - 1 spills, and the
///   spill-time refusal of a 33rd tier is not reachable by any fixture. The
///   figure bounds the store's retained lookup buffers (at most
///   `MAXIMUM_LEVELS` buffers of `merge_buffer_bytes`) and is pinned by value
///   below. Derived, unverified at scale.
/// - `MAXIMUM_PAGE_BYTES` (8,192) is enforced by the page encoders; the boundary
///   is a function of a page's exact encoded size, and the codec cases in
///   `filesystem_codec` assert the boundary through the encoders rather than by
///   constructing an 8,193-byte page, which the encoder cannot produce.
/// - The storage-side group/record/transaction constants live in
///   `layerfs-storage`; their boundary cases and derived notes are the ones in
///   that package's `storage_limits` suite, not here.
#[test]
fn the_limits_this_suite_cannot_bound_are_named_with_their_reason() {
    // A case that keeps the notes above part of the target rather than a comment
    // in a file nothing reads: the figures below are exactly the limits this suite
    // does not probe, and each is asserted here so a reader can tell a
    // deliberately unbounded row from a forgotten one.
    use layerfs_content::filesystem::limits::{MAXIMUM_PAGE_BYTES, MAXIMUM_TREE_LEVEL};
    use layerfs_content::filesystem::references::runs::MAXIMUM_LEVELS;
    assert_eq!(MAXIMUM_TREE_LEVEL, 31);
    assert_eq!(MAXIMUM_PAGE_BYTES, 8_192);
    assert_eq!(MAXIMUM_LEVELS, 32);
    // Occupying every tier needs one spill per binary-counter pattern up to
    // 2^32 - 1: tier k holds the rows of 2^k batches, so the 33rd-tier refusal
    // the run store guards against cannot be produced by any fixture.
    assert_eq!(
        1_u64.checked_shl(u32::try_from(MAXIMUM_LEVELS).expect("small") - 1),
        Some(1_u64 << 31),
        "the tier arithmetic this note relies on"
    );
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

#[test]
fn a_read_wave_is_accepted_at_4096_demands_and_refused_at_4097() {
    // The wave's own demand ceiling bounds the slice a caller hands the
    // operation, independent of the payload byte ceiling the read path enforces.
    // A wave at the ceiling passes the count check and reaches the provider (an
    // empty store answers MissingObject, which is what proves the wave was not
    // refused by the count); one demand over the ceiling is refused before the
    // provider is asked at all.
    use layerfs_content::filesystem::objects::MAXIMUM_READ_DEMANDS;
    use layerfs_content::filesystem::FilesystemObjects;
    use layerfs_content::ObjectId;
    use support::filesystem::TreeStore;

    assert_eq!(MAXIMUM_READ_DEMANDS, 4_096);
    let at_limit: Vec<ObjectId> = (0..MAXIMUM_READ_DEMANDS)
        .map(|index| ObjectId::for_bytes(format!("limits/wave/{index}").as_bytes()))
        .collect();
    let over: Vec<ObjectId> = (0..MAXIMUM_READ_DEMANDS + 1)
        .map(|index| ObjectId::for_bytes(format!("limits/wave/{index}").as_bytes()))
        .collect();

    let empty = TreeStore::new();
    let mut sink = TreeStore::new();
    let mut objects = FilesystemObjects::new(&empty, &mut sink);
    assert!(
        matches!(
            objects.read_batch(&at_limit),
            Err(ContentError::MissingObject)
        ),
        "a wave at the ceiling passes the count check and reaches the provider"
    );
    assert!(
        matches!(
            objects.read_batch(&over),
            Err(ContentError::ObjectLimitExceeded { limit, actual })
                if limit == MAXIMUM_READ_DEMANDS && actual == MAXIMUM_READ_DEMANDS + 1
        ),
        "one demand over the ceiling is refused by the declared bound"
    );
}
