//! Canonical identity, framing and frozen profile fixtures.
//!
//! The expected bytes and identities below were computed independently from the
//! frozen profile formulas (domain-separated BLAKE3 over the canonical envelope,
//! and the frozen mapping descriptor) by a separate program; they are recorded
//! here so any framing or hashing change fails loudly.

mod support;

use layerfs_content::file::mapping::{decode_chunk_payload, profile_id, CHUNK_MAGIC};
use layerfs_content::object::codec::{
    decode_bytes_object, encode_bytes_object, encode_bytes_object_to, HEADER_LEN,
    MAX_PAYLOAD_BYTES, OBJECT_MAGIC,
};
use layerfs_content::policy::{MAX_CANONICAL_OBJECT_BYTES, MAX_OBJECT_FIELD_BYTES};
use layerfs_content::{
    construct_bytes, ConstructionPolicy, ContentError, DiscardingConsumer, ObjectId, OBJECT_DOMAIN,
};
use support::{disabled_scope, patterned};

/// Domain probe fixture: `layerfs/object/v2\0` followed by this text.
const DOMAIN_PROBE: &[u8] = b"layerfs-v2-object-domain-probe";
const DOMAIN_PROBE_ID: &str = "2465a95cc821c538feddb27829458e640c9d7114cf4ffce6ee43000030006606";

/// Small canonical object fixture: `LFS5SML\0`, version 1, then this payload.
const SMALL_FIXTURE: &[u8] = b"layerfs-stage02-fixture";
const SMALL_FIXTURE_CANONICAL_LEN: usize = 46;
const SMALL_FIXTURE_CANONICAL_HEX: &str =
    "4c46534f0100000025000000214c465335534d4c0000016c6179657266732d737461676530322d66697874757265";
const SMALL_FIXTURE_ROOT: &str = "81bb74372b166542e1547b1a03c6637b72dc8dacb08f63489de0fe0fab1dcfa3";

/// Chunk fixture: `LFS4CHK\0` followed by 8192 bytes of `0x5a`.
const CHUNK_FIXTURE_CANONICAL_LEN: usize = 8_213;
const CHUNK_FIXTURE_ID: &str = "101494d51a92fa6117e54858d6776e8e8e7a1b920e2448f2a2be28b48c3302d7";

const EMPTY_LEAF_CANONICAL_HEX: &str =
    "4c46534f01000000230000001f4c4653344d4150000003080000000000000000000000000000000000000000";
const EMPTY_LEAF_ID: &str = "e176c4795c65bda8f56529d144112ee28f1b71bd50a786d821c68ad619839f6f";
const CDC_PROFILE_ID: &str = "09002d304e6872b322b31ae8e316db1d06b748fac8dd5ee3472bef6002aa0116";
const MAPPING_PROFILE_ID: &str = "e99288f3bc4adea6901bcbb2b14c16f5f573c9cb436309a6cc73d51deb335a72";
const EMPTY_STATE_ID: &str = "56ebfbabce799e6402758d692dc07ff29ccd2b89ca3720743b7fd359c6801e3f";

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(text, "{byte:02x}").expect("writing into a String cannot fail");
    }
    text
}

#[test]
fn identity_is_domain_separated_and_not_a_raw_digest() {
    assert_eq!(OBJECT_DOMAIN, b"layerfs/object/v2\0");
    let id = ObjectId::for_bytes(DOMAIN_PROBE);
    assert_eq!(id.to_string(), DOMAIN_PROBE_ID);
    assert_ne!(id.to_bytes(), *blake3::hash(DOMAIN_PROBE).as_bytes());
    let mut legacy = blake3::Hasher::new();
    legacy.update(b"layerfs/object\0");
    legacy.update(DOMAIN_PROBE);
    assert_ne!(id.to_bytes(), *legacy.finalize().as_bytes());
}

#[test]
fn identity_has_no_variable_width_or_text_shortcuts() {
    assert_eq!(ObjectId::from_bytes(&[0; 32]).unwrap().to_bytes(), [0; 32]);
    assert_eq!(
        ObjectId::from_bytes(&[0; 31]),
        Err(ContentError::InvalidIdentityLength {
            expected: 32,
            actual: 31
        })
    );
    assert_eq!(
        ObjectId::from_bytes(&[0; 33]),
        Err(ContentError::InvalidIdentityLength {
            expected: 32,
            actual: 33
        })
    );
    let id = ObjectId::for_bytes(b"round trip");
    assert_eq!(id.to_string().parse::<ObjectId>().unwrap(), id);
    assert_eq!(
        id.to_string().parse::<ObjectId>().unwrap().to_bytes(),
        id.to_bytes()
    );
    assert_eq!(
        "zz".parse::<ObjectId>(),
        Err(ContentError::InvalidIdentityText)
    );
    assert_eq!(
        "".parse::<ObjectId>(),
        Err(ContentError::InvalidIdentityText)
    );
}

#[test]
fn small_fixture_bytes_and_identity_are_frozen() {
    let policy = ConstructionPolicy::frozen_default();
    let mut consumer = DiscardingConsumer::new();
    let constructed = disabled_scope(|scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            SMALL_FIXTURE,
            &mut consumer,
            scope.child("content"),
        )
    })
    .unwrap();
    assert_eq!(constructed.root.to_string(), SMALL_FIXTURE_ROOT);
    let registry = support::RecordingStore::new();
    let mut consumer = support::RecordingStore::new();
    let constructed = disabled_scope(|scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            SMALL_FIXTURE,
            &mut consumer,
            scope.child("content"),
        )
    })
    .unwrap();
    let canonical = consumer.canonical(constructed.root).expect("stored");
    assert_eq!(canonical.len(), SMALL_FIXTURE_CANONICAL_LEN);
    assert_eq!(hex(canonical), SMALL_FIXTURE_CANONICAL_HEX);
    drop(registry);
}

#[test]
fn chunk_fixture_bytes_and_identity_are_frozen() {
    let raw = vec![0x5a_u8; 8_192];
    let canonical = layerfs_content::file::mapping::encode_chunk_object(&raw).unwrap();
    assert_eq!(canonical.len(), CHUNK_FIXTURE_CANONICAL_LEN);
    assert_eq!(
        ObjectId::for_bytes(&canonical).to_string(),
        CHUNK_FIXTURE_ID
    );
    let value = decode_bytes_object(&canonical).unwrap();
    assert_eq!(decode_chunk_payload(value).unwrap(), raw.as_slice());
    assert!(value.starts_with(CHUNK_MAGIC));
}

#[test]
fn mapping_profile_and_empty_form_identities_are_frozen() {
    assert_eq!(
        hex(&layerfs_content::file::cdc::profile_id()),
        CDC_PROFILE_ID
    );
    assert_eq!(profile_id().to_string(), MAPPING_PROFILE_ID);
    let record = support::RecordingStore::new();
    let mut store = support::RecordingStore::new();
    let policy = ConstructionPolicy::frozen_default();
    let constructed = disabled_scope(|scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            &[],
            &mut store,
            scope.child("content"),
        )
    })
    .unwrap();
    assert_eq!(constructed.root.to_string(), EMPTY_STATE_ID);
    let leaf = store
        .canonical_by_role(layerfs_content::ObjectRole::ExtentLeaf)
        .expect("empty leaf");
    assert_eq!(hex(leaf), EMPTY_LEAF_CANONICAL_HEX);
    assert_eq!(ObjectId::for_bytes(leaf).to_string(), EMPTY_LEAF_ID);
    drop(record);
}

#[test]
fn malformed_envelopes_are_rejected_exactly() {
    let value = patterned(64);
    let canonical = encode_bytes_object(&value).unwrap();
    assert_eq!(decode_bytes_object(&canonical).unwrap(), value.as_slice());
    assert_eq!(
        decode_bytes_object(&canonical[..HEADER_LEN]),
        Err(ContentError::UnexpectedEof)
    );
    let mut trailing = canonical.clone();
    trailing.push(0);
    assert_eq!(
        decode_bytes_object(&trailing),
        Err(ContentError::TrailingBytes)
    );
    let mut truncated = canonical.clone();
    truncated.pop();
    assert_eq!(
        decode_bytes_object(&truncated),
        Err(ContentError::UnexpectedEof)
    );
    let mut wrong_magic = canonical.clone();
    wrong_magic[0] = b'X';
    assert_eq!(
        decode_bytes_object(&wrong_magic),
        Err(ContentError::UnsupportedFraming)
    );
    let mut wrong_kind = canonical.clone();
    wrong_kind[4] = 9;
    assert_eq!(
        decode_bytes_object(&wrong_kind),
        Err(ContentError::WrongLogicalRole)
    );
    let mut overflow = canonical.clone();
    overflow[5..9].copy_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(
        decode_bytes_object(&overflow),
        Err(ContentError::ObjectLimitExceeded {
            limit: MAX_PAYLOAD_BYTES,
            actual: u32::MAX as usize,
        })
    );
    let mut mismatched = canonical.clone();
    mismatched[9..13].copy_from_slice(&(value.len() as u32 - 1).to_be_bytes());
    assert_eq!(
        decode_bytes_object(&mismatched),
        Err(ContentError::TrailingBytes)
    );
}

#[test]
fn writing_to_a_sink_matches_the_single_allocation_encoder() {
    let value = patterned(4_096);
    let expected = encode_bytes_object(&value).unwrap();
    let mut written = Vec::new();
    encode_bytes_object_to(&value, &mut written).unwrap();
    assert_eq!(written, expected);
    assert_eq!(&OBJECT_MAGIC, b"LFSO");
    assert_eq!(expected.len(), value.len() + HEADER_LEN + 4);
}

#[test]
fn identity_of_a_damaged_canonical_object_changes() {
    let canonical = encode_bytes_object(b"payload").unwrap();
    let original = ObjectId::for_bytes(&canonical);
    let mut damaged = canonical.clone();
    let last = damaged.len() - 1;
    damaged[last] ^= 0x01;
    assert_ne!(ObjectId::for_bytes(&damaged), original);
}

#[test]
fn the_field_ceiling_is_the_frozen_eight_mib_not_the_envelope() {
    // The frozen format bounds a single field and the envelope separately. The
    // envelope ceiling must sit above the field ceiling, and the field ceiling is
    // what actually limits a bytes-role value.
    assert_eq!(MAX_OBJECT_FIELD_BYTES, 8 * 1024 * 1024);
    assert_eq!(MAX_CANONICAL_OBJECT_BYTES, 16 * 1024 * 1024);
    // The envelope must be able to frame a maximum-size field.
    const { assert!(MAX_OBJECT_FIELD_BYTES < MAX_CANONICAL_OBJECT_BYTES) };

    // Exactly at the ceiling: accepted, and the envelope still has room for framing.
    let at_limit = vec![0x5a_u8; MAX_OBJECT_FIELD_BYTES];
    let canonical = encode_bytes_object(&at_limit).expect("a maximum-size field is accepted");
    assert_eq!(canonical.len(), MAX_OBJECT_FIELD_BYTES + HEADER_LEN + 4);
    assert_eq!(
        decode_bytes_object(&canonical).unwrap(),
        at_limit.as_slice()
    );

    // One byte over: refused on the encode side, before any allocation of the
    // envelope, with the field ceiling named rather than the envelope ceiling.
    let over = vec![0x5a_u8; MAX_OBJECT_FIELD_BYTES + 1];
    assert_eq!(
        encode_bytes_object(&over),
        Err(ContentError::ObjectLimitExceeded {
            limit: MAX_OBJECT_FIELD_BYTES,
            actual: MAX_OBJECT_FIELD_BYTES + 1,
        })
    );

    // The same divergence must be refused on the decode side, for bytes that were
    // not produced by this encoder: a hand-built envelope whose declared value lies
    // between the field ceiling and the envelope ceiling.
    let framed_len = MAX_OBJECT_FIELD_BYTES + 1;
    let mut hand_built = Vec::with_capacity(HEADER_LEN + 4 + framed_len);
    hand_built.extend_from_slice(&OBJECT_MAGIC);
    hand_built.push(1);
    hand_built.extend_from_slice(&((framed_len + 4) as u32).to_be_bytes());
    hand_built.extend_from_slice(&(framed_len as u32).to_be_bytes());
    hand_built.extend(std::iter::repeat_n(0x5a_u8, framed_len));
    assert_eq!(
        decode_bytes_object(&hand_built),
        Err(ContentError::ObjectLimitExceeded {
            limit: MAX_OBJECT_FIELD_BYTES,
            actual: framed_len,
        })
    );

    // The envelope ceiling is still reported for a genuinely oversized total.
    let mut too_large = Vec::new();
    too_large.extend_from_slice(&OBJECT_MAGIC);
    too_large.push(1);
    too_large.extend_from_slice(&(MAX_PAYLOAD_BYTES as u32 + 1).to_be_bytes());
    too_large.extend_from_slice(&0_u32.to_be_bytes());
    assert!(matches!(
        decode_bytes_object(&too_large),
        Err(ContentError::ObjectLimitExceeded { .. })
    ));
}

/// The envelope ceiling at plus or minus one, and the rejection that keeps an
/// oversized object away from a Store.
///
/// No role can *produce* a canonical object at either magnitude: the whole-file
/// envelope is cutoff-bounded (1 048 598 bytes at the largest accepted cutoff),
/// a chunk payload is at most 32 781 bytes and a pooled leaf at most 8 144, so
/// the 16 MiB envelope is a format guard rather than a reachable acceptance path
/// in C2. What is reachable is the guard itself, and it is exercised here at the
/// boundary: exactly the envelope ceiling is not refused *for its total*, one byte
/// over is refused by name, and `FinalizedObject::new` - the only constructor that
/// feeds a save - refuses the oversized bytes before any Store sees them.
#[test]
fn the_envelope_ceiling_is_enforced_at_plus_or_minus_one() {
    assert_eq!(MAX_CANONICAL_OBJECT_BYTES, 16 * 1024 * 1024);

    // Exactly at the ceiling: the total is accepted, and the *field* ceiling is
    // what refuses the hand-built declaration, which proves the total check did
    // not fire one byte early.
    let mut at_ceiling = vec![0u8; MAX_CANONICAL_OBJECT_BYTES];
    at_ceiling[..OBJECT_MAGIC.len()].copy_from_slice(&OBJECT_MAGIC);
    at_ceiling[4] = 1;
    at_ceiling[5..9].copy_from_slice(&(MAX_PAYLOAD_BYTES as u32).to_be_bytes());
    at_ceiling[HEADER_LEN..HEADER_LEN + 4]
        .copy_from_slice(&((MAX_OBJECT_FIELD_BYTES + 1) as u32).to_be_bytes());
    assert_eq!(
        decode_bytes_object(&at_ceiling),
        Err(ContentError::ObjectLimitExceeded {
            limit: MAX_OBJECT_FIELD_BYTES,
            actual: MAX_OBJECT_FIELD_BYTES + 1,
        }),
        "the envelope ceiling must not fire at exactly the envelope ceiling"
    );

    // One byte over: the envelope ceiling is named.
    let mut over_ceiling = at_ceiling.clone();
    over_ceiling.push(0);
    assert_eq!(
        decode_bytes_object(&over_ceiling),
        Err(ContentError::ObjectLimitExceeded {
            limit: MAX_CANONICAL_OBJECT_BYTES,
            actual: MAX_CANONICAL_OBJECT_BYTES + 1,
        })
    );

    // The object the Store would receive is refused before it is constructed, so
    // an oversized envelope can never reach a save, a pack or a row.
    assert_eq!(
        layerfs_content::FinalizedObject::new(
            layerfs_content::ObjectRole::WholeFile,
            over_ceiling.clone()
        )
        .unwrap_err(),
        ContentError::ObjectLimitExceeded {
            limit: MAX_CANONICAL_OBJECT_BYTES,
            actual: MAX_CANONICAL_OBJECT_BYTES + 1,
        }
    );
    assert!(
        layerfs_content::FinalizedObject::new(
            layerfs_content::ObjectRole::WholeFile,
            at_ceiling.clone()
        )
        .is_err(),
        "an envelope that declares an over-wide field is refused too"
    );
}

/// Moving an object's pieces keeps the predecessor input the save path consumes.
///
/// `into_parts` is how a consumer takes ownership of a finalized object, and the
/// production save path reads `predecessors()` on the object it is handed to pick
/// a physical representation. The tuple form dropped them, so a consumer that
/// moved its pieces silently lost an input the object had been built with. This
/// case builds an object that carries a predecessor, moves it, and checks the
/// predecessor arrives.
#[test]
fn moving_an_objects_pieces_keeps_its_advisory_predecessors() {
    use layerfs_content::{
        FinalizedObject, ObjectId, ObjectRole, PredecessorProvenance, MAXIMUM_ADVISORY_PREDECESSORS,
    };

    let bytes = patterned(4_096);
    let canonical = layerfs_content::file::encode_whole_file(
        &layerfs_content::ConstructionPolicy::frozen_default().capacities(),
        &bytes,
    )
    .expect("whole-file object");
    let object = FinalizedObject::new(ObjectRole::WholeFile, canonical.clone()).expect("finalized");
    let identity = object.id();
    let base = ObjectId::for_bytes(b"object-identity/predecessor-base");

    let object = object.with_predecessors(
        layerfs_content::AdvisoryPredecessors::explicit(base).expect("one predecessor"),
    );
    let parts = object.into_parts();
    assert_eq!(parts.id, identity);
    assert_eq!(parts.role, ObjectRole::WholeFile);
    assert_eq!(parts.canonical, canonical);
    assert!(parts.references.is_empty());
    assert_eq!(
        parts.predecessors.ids().collect::<Vec<_>>(),
        vec![base],
        "the moved pieces still carry the predecessor"
    );
    assert!(
        parts.predecessors.len() <= MAXIMUM_ADVISORY_PREDECESSORS,
        "the predecessor bound travels with them"
    );
    let entries = parts.predecessors.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id(), base);
    assert_eq!(entries[0].provenance(), PredecessorProvenance::OriginalBase);
}
