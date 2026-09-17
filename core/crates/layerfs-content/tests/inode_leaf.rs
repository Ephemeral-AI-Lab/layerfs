//! Checked compact inode-leaf grammar: the physical-pooling input format.
//!
//! These cases exercise the grammar directly, without a Store: exact layout,
//! derived physical width, pooled body round trip and every rejected malformation.
//! A malformed row, order, count or length must fail rather than be repaired.

mod support;

use layerfs_content::inode_leaf::{
    decode_inode_value, decode_pooled_body, decode_pooled_value, encode_inode_value,
    encode_pooled_value, pooled_body, pooled_physical_length, InodeKind, InodeLeaf, InodeLeafRow,
    InodeValue, LEAF_ROW_BYTES, MAXIMUM_LEAF_ROWS, POOLED_PREFIX_BYTES, POOLED_ROW_BYTES,
};
use layerfs_content::{ContentError, ObjectId};

fn value(kind: InodeKind, refs: u64, seed: u8) -> InodeValue {
    InodeValue {
        kind,
        namespace_ref_count: refs,
        content_root: ObjectId::for_bytes(&[seed; 4]),
        metadata_root: ObjectId::for_bytes(&[seed.wrapping_add(1); 4]),
    }
}

fn leaf(rows: usize) -> InodeLeaf {
    InodeLeaf {
        // The recorded subtree byte total is the encoded row width: one 8-byte
        // serial plus the 73-byte value, exactly as the reference encoder writes
        // it and as the sealed reference fixture records it.
        subtree_bytes: rows as u64 * LEAF_ROW_BYTES as u64,
        rows: (0..rows)
            .map(|index| InodeLeafRow {
                serial: index as u64 + 1,
                value: encode_inode_value(value(InodeKind::RegularFile, 1, index as u8)),
            })
            .collect(),
    }
}

#[test]
fn a_leaf_round_trips_with_its_exact_layout() {
    for rows in [1_usize, 2, 63, 64, 100] {
        let original = leaf(rows);
        let canonical = original.encode().expect("encodes");
        assert_eq!(
            canonical.len(),
            POOLED_PREFIX_BYTES + rows * LEAF_ROW_BYTES,
            "canonical width for {rows} rows"
        );
        let decoded = InodeLeaf::decode(&canonical).expect("decodes");
        assert_eq!(decoded, original);
        assert_eq!(decoded.row_count(), rows);
        assert_eq!(
            pooled_physical_length(canonical.len()).expect("physical width"),
            POOLED_PREFIX_BYTES + rows * POOLED_ROW_BYTES
        );
    }
}

#[test]
fn a_pooled_body_keeps_the_prefix_and_the_serials() {
    let original = leaf(3);
    let canonical = original.encode().expect("encodes");
    let ordinals = [7_u32, 9, 11];
    let body = pooled_body(&canonical, &ordinals).expect("pooled body");
    assert_eq!(body.len(), POOLED_PREFIX_BYTES + 3 * POOLED_ROW_BYTES);
    assert_eq!(
        &body[..POOLED_PREFIX_BYTES],
        &canonical[..POOLED_PREFIX_BYTES]
    );
    let (prefix, rows) = decode_pooled_body(&body).expect("decode");
    assert_eq!(prefix.len(), POOLED_PREFIX_BYTES);
    assert_eq!(rows.len(), 3);
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row.serial, index as u64 + 1);
        assert_eq!(row.ordinal, ordinals[index]);
    }
}

#[test]
fn an_inode_value_round_trips_and_rejects_a_foreign_kind() {
    for (kind, code) in [
        (InodeKind::RegularFile, 1_u8),
        (InodeKind::Directory, 2),
        (InodeKind::Symlink, 3),
    ] {
        let encoded = encode_inode_value(value(kind, 3, 9));
        assert_eq!(encoded.len(), 73);
        assert_eq!(encoded[0], code);
        assert_eq!(
            decode_inode_value(&encoded).expect("decodes"),
            value(kind, 3, 9)
        );
    }
    let mut bad = encode_inode_value(value(InodeKind::Directory, 1, 1));
    bad[0] = 9;
    assert!(matches!(
        decode_inode_value(&bad),
        Err(ContentError::InvalidRecord(_))
    ));
    assert!(decode_inode_value(&bad[..72]).is_err());
}

#[test]
fn a_pooled_value_object_is_exactly_ninety_four_canonical_bytes() {
    let raw = encode_inode_value(value(InodeKind::Directory, 0, 4));
    let canonical = encode_pooled_value(&raw).expect("canonical value");
    assert_eq!(canonical.len(), 94);
    assert_eq!(decode_pooled_value(&canonical).expect("decodes"), raw);
    // A damaged magic is a grammar failure. A flipped payload byte is *not*:
    // every 32-byte root is a structurally valid identity, so the object identity
    // - not the grammar - is what rejects it, exactly as it does for every other
    // canonical object.
    let mut damaged = canonical.clone();
    damaged[13] ^= 0xff;
    assert!(matches!(
        decode_pooled_value(&damaged),
        Err(ContentError::InvalidRecord("pooled value framing"))
    ));
    let mut payload = canonical;
    *payload.last_mut().expect("byte") ^= 0xff;
    assert!(decode_pooled_value(&payload).is_ok());
}

/// Canonical bytes of the envelope a value is carried in.
///
/// A value offset is a canonical offset plus this. The reviewed version of the
/// case below patched `canonical[14]` and `canonical[13]` and called them "a wrong
/// role tag" and "a count that disagrees with the row area": both are inside the
/// envelope's magic, so those two cases only exercised envelope framing and the
/// fields they named were never touched. They are patched at their real offsets
/// here, and every refusal is asserted as its exact variant rather than as a bare
/// `is_err()`.
const ENVELOPE_LEN: usize = 13;

#[test]
fn malformed_leaves_are_rejected() {
    let original = leaf(4);
    let canonical = original.encode().expect("encodes");

    // Trailing bytes.
    let mut trailing = canonical.clone();
    trailing.push(0);
    assert!(matches!(
        InodeLeaf::decode(&trailing),
        Err(ContentError::TrailingBytes)
    ));

    // A wrong role tag: value byte 10.
    let mut role = canonical.clone();
    role[ENVELOPE_LEN + 10] = 8;
    assert!(matches!(
        InodeLeaf::decode(&role),
        Err(ContentError::InvalidRecord("inode leaf role/level"))
    ));

    // A count that disagrees with the subtree count in the same header: value
    // bytes 13..15 against value bytes 15..23.
    let mut count = canonical.clone();
    count[ENVELOPE_LEN + 13] = 0;
    count[ENVELOPE_LEN + 14] = 5;
    assert!(matches!(
        InodeLeaf::decode(&count),
        Err(ContentError::NonCanonicalPagePartition)
    ));

    // A row area shorter than the header's count: the last row is cut away.
    let mut short_rows = canonical.clone();
    short_rows.truncate(short_rows.len() - 1);
    assert!(matches!(
        InodeLeaf::decode(&short_rows),
        Err(ContentError::TrailingBytes | ContentError::UnexpectedEof)
    ));

    // The envelope's own magic, damaged: still refused, and as framing.
    let mut magic = canonical.clone();
    magic[ENVELOPE_LEN + 1] ^= 0xff;
    assert!(matches!(
        InodeLeaf::decode(&magic),
        Err(ContentError::UnsupportedFraming)
    ));

    // Out-of-order serials are rejected by the encoder and by a re-encode check.
    let mut unordered = original.clone();
    unordered.rows.swap(0, 1);
    assert!(matches!(
        unordered.encode(),
        Err(ContentError::InvalidRecord("inode key order"))
    ));

    // A zero serial is not a valid key.
    let mut zero = original.clone();
    zero.rows[0].serial = 0;
    assert!(matches!(
        zero.encode(),
        Err(ContentError::InvalidRecord("inode serial"))
    ));

    // The subtree byte total must match the row count.
    let mut wrong_total = original.clone();
    wrong_total.subtree_bytes += LEAF_ROW_BYTES as u64;
    assert!(matches!(
        wrong_total.encode(),
        Err(ContentError::LengthMismatch { .. })
    ));

    // More rows than the profile allows.
    let big = leaf(MAXIMUM_LEAF_ROWS + 1);
    assert!(matches!(
        big.encode(),
        Err(ContentError::NonCanonicalPagePartition)
    ));

    // Zero rows is not a page.
    let empty = leaf(0);
    assert!(matches!(
        empty.encode(),
        Err(ContentError::NonCanonicalPagePartition)
    ));
}

#[test]
fn a_pooled_body_rejects_a_zero_ordinal_and_a_wrong_width() {
    let canonical = leaf(2).encode().expect("encodes");
    assert!(matches!(
        pooled_body(&canonical, &[1, 0]),
        Err(ContentError::InvalidRecord("pooled ordinal count"))
    ));
    assert!(matches!(
        pooled_body(&canonical, &[1]),
        Err(ContentError::InvalidRecord("pooled ordinal count"))
    ));
    let mut body = pooled_body(&canonical, &[1, 2]).expect("body");
    body.push(0);
    assert!(matches!(
        decode_pooled_body(&body),
        Err(ContentError::InvalidRecord("pooled body rows"))
    ));
    let short = vec![0_u8; POOLED_PREFIX_BYTES - 1];
    assert!(matches!(
        decode_pooled_body(&short),
        Err(ContentError::UnexpectedEof)
    ));
}
