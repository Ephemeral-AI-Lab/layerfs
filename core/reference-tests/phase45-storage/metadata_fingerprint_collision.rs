//! Two distinct pooled values with the same candidate fingerprint.
//!
//! The pooled candidate filter keeps `(fingerprint, ordinal)` pairs, where the
//! fingerprint is only the low eight digest bytes of a canonical value: it is a
//! filter, and the full 73 bytes decide equality. A real collision cannot be
//! written by hand, so it was searched for and found:
//!
//! `crates/layerfs-content/examples/fingerprint_collision_search.rs` ran a
//! distinguished-point collision search over valid canonical inode values and found
//! the pair below after 435,008,316 single-start steps (431 chains), recorded in
//! `docs/roadmap/0.1/0.1.7/evidence/stages-3-4-fingerprint-collision-20260917T021500Z/`.
//!
//! These cases use the pair through the real Store: the first value must be stored
//! on its own, the colliding second value must NOT be confused with it, and a leaf
//! holding both must read back byte-exact and identity-exact, which can only happen
//! when the values were kept separate.

mod support;

use layerfs_content::inode_leaf::{
    decode_inode_value, InodeLeaf, InodeLeafRow, INODE_VALUE_BYTES, LEAF_ROW_BYTES,
};
use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};
use support::{create_store, read_objects, save_one, TempDir};

/// First colliding canonical value (73 bytes).
const FIRST_HEX: &str = concat!(
    "010000000000000001c9b8525eb4ccd5b796b9fa361957ca8bf2a265aebe4dc6",
    "95be4dc695f2a265aea5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5",
    "a5a5a5a5a5a5a5a5a5",
);
/// Second colliding canonical value (73 bytes), different bytes, same fingerprint.
const SECOND_HEX: &str = concat!(
    "0100000000000000011a991dd849bcbad78957f75a23b3033bc04ee2d5bdd6c8",
    "ecbdd6c8ecc04ee2d5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5",
    "a5a5a5a5a5a5a5a5a5",
);
/// The fingerprint the search reported for both values.
const FINGERPRINT_HEX: &str = "972d4e33fafff505";

fn value_of(hex: &str) -> [u8; INODE_VALUE_BYTES] {
    let mut bytes = [0_u8; INODE_VALUE_BYTES];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).expect("hex fixture");
    }
    bytes
}

/// The candidate filter, recomputed from the public identity of a value.
fn fingerprint(value: &[u8; INODE_VALUE_BYTES]) -> u64 {
    let canonical = ObjectId::for_bytes(value);
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&canonical.as_bytes()[..8]);
    u64::from_le_bytes(bytes)
}

fn leaf(first_serial: u64, values: &[[u8; INODE_VALUE_BYTES]]) -> FinalizedObject {
    let rows = values
        .iter()
        .enumerate()
        .map(|(index, value)| InodeLeafRow {
            serial: first_serial + index as u64,
            value: *value,
        })
        .collect::<Vec<_>>();
    let canonical = InodeLeaf {
        // The recorded total is the encoded row width: an 8-byte serial plus the
        // 73-byte value, exactly as the reference encoder writes it.
        subtree_bytes: rows.len() as u64 * LEAF_ROW_BYTES as u64,
        rows,
    }
    .encode()
    .expect("canonical leaf");
    FinalizedObject::new(ObjectRole::InodeLeaf, canonical).expect("finalized leaf")
}

#[test]
fn the_searched_pair_is_a_real_collision_of_valid_values() {
    let first = value_of(FIRST_HEX);
    let second = value_of(SECOND_HEX);
    assert_eq!(FIRST_HEX.len(), INODE_VALUE_BYTES * 2, "fixture width");
    assert_eq!(SECOND_HEX.len(), INODE_VALUE_BYTES * 2, "fixture width");
    assert_ne!(first, second, "the pair must be two distinct values");
    assert_eq!(
        fingerprint(&first),
        u64::from_str_radix(FINGERPRINT_HEX, 16).expect("fingerprint fixture"),
        "the recorded fingerprint must be reproducible"
    );
    assert_eq!(
        fingerprint(&first),
        fingerprint(&second),
        "the pair must collide in the candidate filter"
    );
    assert!(
        decode_inode_value(&first).is_ok() && decode_inode_value(&second).is_ok(),
        "both values must be valid canonical inode values"
    );
}

#[test]
fn colliding_values_are_never_confused_through_the_store() {
    let dir = TempDir::new("fingerprint-collision");
    let path = dir.store_path("collision");
    let store = create_store(&path);
    let first = value_of(FIRST_HEX);
    let second = value_of(SECOND_HEX);

    // The first value is admitted on its own.
    let first_leaf = leaf(1, &[first]);
    let first_id = first_leaf.id();
    let outcome = save_one(&store, first_leaf).expect("first value");
    assert_eq!(outcome.pool.new_values, 1);
    assert_eq!(outcome.pool.reused_values, 0);

    // The colliding second value is a different value, so it must not reuse the
    // first one's ordinal: the fingerprint matches and the bytes do not.
    let second_leaf = leaf(2, &[second]);
    let second_id = second_leaf.id();
    let outcome = save_one(&store, second_leaf).expect("colliding value");
    assert_eq!(
        outcome.pool.reused_values, 0,
        "a fingerprint match must not be treated as value equality"
    );
    assert_eq!(outcome.pool.new_values, 1);

    // A leaf holding both resolves both to their own ordinals: the readback is
    // byte-exact and identity-exact only when the two values stayed separate.
    let both = leaf(3, &[first, second]);
    let both_id = both.id();
    let expected = both.canonical().to_vec();
    let outcome = save_one(&store, both).expect("both values");
    assert_eq!(outcome.pool.reused_values, 2, "both values are candidates");
    assert_eq!(outcome.pool.new_values, 0);
    assert_eq!(outcome.pool.groups, 0, "no group was written");

    let (read, _) =
        read_objects(&store, &[first_id, second_id, both_id]).expect("readback through the pool");
    assert_eq!(read.len(), 3);
    assert_eq!(
        ObjectId::for_bytes(&read[2]),
        both_id,
        "the pooled readback must reproduce the admitted leaf"
    );
    assert_eq!(read[2], expected, "and its exact canonical bytes");
    assert_eq!(ObjectId::for_bytes(&read[0]), first_id);
    assert_eq!(ObjectId::for_bytes(&read[1]), second_id);

    // Each single-value leaf reads back as itself as well.
    let (single, _) = read_objects(&store, &[first_id, second_id]).expect("single readback");
    assert_ne!(
        single[0], single[1],
        "the two ordinals hold different bytes"
    );
    assert_eq!(ObjectId::for_bytes(&single[0]), first_id);
    assert_eq!(ObjectId::for_bytes(&single[1]), second_id);
}
