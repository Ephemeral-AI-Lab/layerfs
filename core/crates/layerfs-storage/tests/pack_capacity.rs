//! Requested Rust allocation and real stored-pack/cache ownership; no native/global fit.
#[path = "support/allocation_observer.rs"]
mod allocation;
mod support;
use allocation::{Observation, ObservedSystem};
use layerfs_content::{encode_whole_file_payload, ObjectId, ObjectRole};
use layerfs_storage::{
    encoding::{
        codec::DecompressionWorkspace,
        delta::read::{BodyCaches, ChainCounters, Resolver},
        pool::PoolReader,
        GroupCache,
    },
    policy::{DECODED_GROUP_CACHE_BYTES, DEPENDENCY_PACK_CACHE_BYTES, SINGLETON_PACK_LIMIT},
    sqlite::{connection, lookup, ObjectLocation},
    StorageError,
};
use rusqlite::{params, Connection};
use std::{collections::BTreeMap, sync::Mutex};
use support::{create_store, TempDir};
#[global_allocator]
static ALLOCATOR: ObservedSystem = ObservedSystem;
static SERIAL: Mutex<()> = Mutex::new(());

// Independent v16 singleton transcribed from frozen wire fields. One raw stored
// WholeFile record: header24/directory16/group8/record9/raw. This intentionally
// covers historical readable larger records, independently of C1's current cutoff.
fn singleton(raw: &[u8]) -> (Vec<u8>, Vec<u8>, ObjectId) {
    let canonical = encode_whole_file_payload(raw).unwrap();
    let id = ObjectId::for_bytes(&canonical);
    let used = 57 + raw.len();
    let mut bytes = vec![0u8; used];
    bytes[..8].copy_from_slice(b"LFPACK\0\0");
    bytes[8..12].copy_from_slice(&16u32.to_le_bytes());
    bytes[12..16].copy_from_slice(&1u32.to_le_bytes());
    bytes[16..20].copy_from_slice(&(used as u32).to_le_bytes());
    bytes[24..28].copy_from_slice(&40u32.to_le_bytes());
    bytes[28..32].copy_from_slice(&((used - 40) as u32).to_le_bytes());
    bytes[32..36].copy_from_slice(&((used - 40) as u32).to_le_bytes());
    // CodecRaw=0 at directory byte36; reserved bytes37..40 stay zero.
    bytes[40..44].copy_from_slice(&1u32.to_le_bytes());
    bytes[44..48].copy_from_slice(&((used - 48) as u32).to_le_bytes());
    bytes[48] = 2; // Stored payload.
    bytes[49..53].copy_from_slice(&(raw.len() as u32).to_le_bytes());
    bytes[53..57].copy_from_slice(&(raw.len() as u32).to_le_bytes());
    bytes[57..].copy_from_slice(raw);
    (bytes, canonical, id)
}
fn publish(
    connection: &Connection,
    pack: i64,
    bytes: &[u8],
    canonical: &[u8],
    id: ObjectId,
    padded: usize,
) -> ObjectLocation {
    assert!(padded >= bytes.len() && padded <= SINGLETON_PACK_LIMIT);
    let mut row = Vec::with_capacity(padded);
    row.extend_from_slice(bytes);
    row.resize(padded, 0);
    connection
        .execute(
            "INSERT INTO object_packs VALUES(?1,1,?2)",
            params![pack, row],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO objects VALUES(?1,1,1,?2,?3,0,0)",
            params![id.as_bytes().as_slice(), canonical.len() as i64, pack],
        )
        .unwrap();
    ObjectLocation {
        object_id: id,
        role: ObjectRole::WholeFile,
        canonical_length: canonical.len(),
        pack_id: pack,
        group_number: 0,
        record_number: 0,
    }
}
fn fixture(label: &str) -> (TempDir, layerfs_storage::Store, Connection) {
    let dir = TempDir::new(label);
    let path = dir.store_path(label);
    let store = create_store(&path);
    let connection = connection::open(&path, false).unwrap();
    connection
        .execute(
            "INSERT INTO saves(save_id,publication,pack_ceiling) VALUES(1,1,100)",
            [],
        )
        .unwrap();
    (dir, store, connection)
}
#[test]
fn padded_16mib_row_copies_only_validated_used_bytes_and_held_vec_owns_that_allocation() {
    let _serial = SERIAL.lock().unwrap();
    let (_dir, _store, connection) = fixture("pack-used-capacity");
    let raw = support::noise(96_000);
    let (pack, canonical, id) = singleton(&raw);
    publish(&connection, 1, &pack, &canonical, id, SINGLETON_PACK_LIMIT);
    let observed = Observation::start([pack.len(), usize::MAX], [usize::MAX - 1, usize::MAX - 2]);
    let held = lookup::pack_bytes(&connection, 1).unwrap();
    assert_eq!(held, pack);
    assert_eq!(held.capacity(), pack.len());
    let stopped = observed.stop();
    assert!(!stopped.overflow);
    assert_eq!(stopped.live[0], pack.len());
    assert!(
        stopped.largest_request <= pack.len(),
        "no full padded BLOB Rust Vec: {stopped:?}"
    );
    drop(connection); // A held buffer survives its provider/row callback.
    assert_eq!(held, pack);
    drop(held);
    let released = observed.released();
    assert_eq!(
        released.live_total(),
        0,
        "actual data destruction releases every captured request"
    );
}
#[test]
fn invalid_used_width_and_hidden_publication_refuse_before_used_buffer_allocation() {
    let _serial = SERIAL.lock().unwrap();
    let (_dir, _store, connection) = fixture("pack-used-refusal");
    let (pack, canonical, id) = singleton(&support::noise(96_000));
    publish(&connection, 1, &pack, &canonical, id, SINGLETON_PACK_LIMIT);
    connection
        .execute("UPDATE temp.layerfs_read_scope SET publication=0", [])
        .unwrap();
    let observed = Observation::start(
        [pack.len(), SINGLETON_PACK_LIMIT],
        [usize::MAX - 1, usize::MAX - 2],
    );
    assert!(matches!(
        lookup::pack_bytes(&connection, 1),
        Err(StorageError::Integrity("pack row is missing"))
    ));
    assert_eq!(observed.stop().peak[0], 0);
    assert_eq!(observed.released().live_total(), 0);
    connection
        .execute("UPDATE temp.layerfs_read_scope SET publication=1", [])
        .unwrap();
    let mut invalid = pack;
    invalid[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    connection
        .execute("UPDATE object_packs SET data=?1 WHERE pack_id=1", [invalid])
        .unwrap();
    let observed = Observation::start(
        [96_057, SINGLETON_PACK_LIMIT],
        [usize::MAX - 1, usize::MAX - 2],
    );
    assert!(matches!(
        lookup::pack_bytes(&connection, 1),
        Err(StorageError::Integrity("pack length"))
    ));
    assert_eq!(observed.stop().peak[0], 0);
    assert_eq!(observed.released().live_total(), 0);
}
#[test]
fn real_resolver_crosses_4mib_by_capacity_and_large_current_body_is_a_separate_last_owner() {
    let _serial = SERIAL.lock().unwrap();
    let (_dir, store, connection) = fixture("pack-cache-capacity");
    const USED: usize = 262_144;
    let mut selected = Vec::new();
    for ordinal in 1..=18 {
        let mut raw = support::noise(USED - 57);
        raw[0] = ordinal as u8;
        let (pack, canonical, id) = singleton(&raw);
        let location = publish(&connection, ordinal, &pack, &canonical, id, USED);
        selected.push((location, canonical));
    }
    let raw = support::noise(5 * 1024 * 1024);
    let (large, expected, id) = singleton(&raw);
    let large_location = publish(&connection, 30, &large, &expected, id, SINGLETON_PACK_LIMIT);
    let capacities = store.capacities();
    let mut workspace = DecompressionWorkspace::new().unwrap();
    let mut packs = BTreeMap::new();
    let mut pool = PoolReader::new();
    let mut groups = GroupCache::new();
    let mut counters = ChainCounters::default();
    let observed = Observation::start([USED, large.len()], [usize::MAX - 1, usize::MAX - 2]);
    for (index, (location, expected)) in selected.iter().enumerate() {
        let mut resolver = Resolver::new(
            &connection,
            i64::MAX,
            &capacities,
            BodyCaches {
                packs: &mut packs,
                pool: &mut pool,
            },
            &mut groups,
            &mut workspace,
            &mut counters,
        );
        let (canonical, verified) = resolver.resolve_at(*location).unwrap();
        assert_eq!(&canonical, expected);
        assert_eq!(verified, location.object_id);
        assert_eq!(
            resolver.packs_read(),
            1,
            "account/base/decode share the actual body"
        );
        assert_eq!(resolver.current_pack_bytes(), 0);
        drop(canonical);
        drop(resolver);
        let retained: usize = packs.values().map(Vec::capacity).sum();
        assert_eq!(
            retained,
            if index < 16 {
                (index + 1) * USED
            } else {
                (index - 15) * USED
            }
        );
        assert!(retained <= DEPENDENCY_PACK_CACHE_BYTES);
    }
    assert_eq!(
        observed.stop().live[0],
        2 * USED,
        "cache retains real data while consumer/context pauses"
    );
    // Capture resumes with a new observer only after the previous cache owners die.
    drop(packs);
    assert_eq!(observed.released().live_total(), 0);
    let mut packs = BTreeMap::new();
    let observed = Observation::start([large.len(), usize::MAX], [usize::MAX - 1, usize::MAX - 2]);
    let mut resolver = Resolver::new(
        &connection,
        i64::MAX,
        &capacities,
        BodyCaches {
            packs: &mut packs,
            pool: &mut pool,
        },
        &mut groups,
        &mut workspace,
        &mut counters,
    );
    let (canonical, verified) = resolver.resolve_at(large_location).unwrap();
    assert_eq!(canonical, expected);
    assert_eq!(verified, id);
    assert_eq!(
        resolver.packs_read(),
        1,
        "large current body is reused through base/account/decode"
    );
    assert_eq!(resolver.current_pack_bytes(), large.len());
    assert_eq!(observed.stop().live[0], large.len());
    drop(resolver);
    assert!(packs.is_empty(), "oversized body never enters cache4MiB");
    assert_eq!(
        observed.stop().live[0],
        0,
        "slot data dies with the real resolver owner"
    );
    assert_eq!(
        canonical, expected,
        "held returned canonical remains independent"
    );
    drop(canonical);
    drop(packs);
    assert_eq!(observed.released().live_total(), 0);
}
#[test]
fn decoded_group_spare_capacity_replacement_and_crossing_use_actual_owned_capacity() {
    let _serial = SERIAL.lock().unwrap();
    let mut cache = GroupCache::new();
    for ordinal in 0..4 {
        let mut bytes = Vec::with_capacity(DECODED_GROUP_CACHE_BYTES / 4);
        bytes.push(ordinal);
        let capacity = bytes.capacity();
        cache.insert(1, ordinal as usize, bytes).unwrap();
        assert_eq!(cache.retained_bytes(), (ordinal as usize + 1) * capacity);
    }
    let mut replacement = Vec::with_capacity(1024);
    replacement.push(7);
    cache.insert(1, 0, replacement).unwrap();
    assert_eq!(
        cache.retained_bytes(),
        3 * (DECODED_GROUP_CACHE_BYTES / 4) + 1024
    );
    let mut incoming = Vec::with_capacity(DECODED_GROUP_CACHE_BYTES / 4);
    incoming.push(8);
    cache.insert(2, 0, incoming).unwrap();
    assert_eq!(
        cache.retained_bytes(),
        DECODED_GROUP_CACHE_BYTES / 4,
        "crossing clears old capacities wholesale"
    );
    let mut oversized = Vec::with_capacity(DECODED_GROUP_CACHE_BYTES + 1);
    oversized.push(9);
    assert!(matches!(
        cache.insert(3, 0, oversized),
        Err(StorageError::CapacityExceeded {
            what: "decoded group cache capacity",
            ..
        })
    ));
    assert_eq!(
        cache.retained_bytes(),
        DECODED_GROUP_CACHE_BYTES / 4,
        "incoming refusal preserves existing owner"
    );
}
