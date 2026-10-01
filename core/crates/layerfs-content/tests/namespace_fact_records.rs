//! Literal private grammar and simultaneous logical-capacity oracles.
use layerfs_content::filesystem::inode::read::InodeTable;
use layerfs_content::filesystem::rows::BindingAuthority;
use layerfs_content::filesystem::state::*;
use layerfs_content::filesystem::{scope_for_seed, FilesystemRootId};
use layerfs_content::{ContentError, ObjectId};
fn scope() -> FactScope {
    let source = BindingAuthority::new().unwrap();
    let base = FilesystemRootId(ObjectId::from_bytes(&[0x2a; 32]).unwrap());
    let selected = GraphSubject::new(
        source.source_id(),
        scope_for_seed([0x31; 32]),
        Some(base),
        9,
        GraphCapacity::default(),
    )
    .unwrap();
    let table = InodeTable {
        root: ObjectId::from_bytes(&[0x73; 32]).unwrap(),
        root_serial: 9,
    };
    let subject = FactSubject::new(selected, Some(table)).unwrap();
    let mut selection = StateSelection::issue([0x82; 32]).unwrap();
    selection.bind_owner([0x91; 32]).unwrap();
    FactScope::new(selection, subject).unwrap()
}
#[test]
fn actual_table_and_original_base_are_distinct_full_width_subject_fields() {
    let scope = scope();
    let selected = scope.subject().selected();
    let encoded = scope.subject().encode();
    assert_eq!(&encoded[42..74], &[0x2a; 32]);
    assert_eq!(encoded[106], 1);
    assert_eq!(&encoded[107..139], &[0x73; 32]);
    assert_eq!(&encoded[139..147], &9u64.to_be_bytes());
    assert_eq!(
        FactSubject::decode(selected, &encoded).unwrap(),
        *scope.subject()
    );
    assert!(FactSubject::new(selected.clone(), None).is_err());
    let table = InodeTable {
        root: ObjectId::from_bytes(&[0x73; 32]).unwrap(),
        root_serial: 10,
    };
    assert!(FactSubject::new(selected.clone(), Some(table)).is_err());
    let mut changed = encoded;
    changed[42] ^= 1;
    assert!(FactSubject::decode(selected, &changed).is_err());
}
#[test]
fn absent_fact_value_and_scoped_scalar_key_have_literal_nontruncated_encoding() {
    let scope = scope();
    let serial = 0x0102_0304_0506_0708;
    let mut expected = [0; 25];
    expected[..8].copy_from_slice(&scope.state().selection().token().to_be_bytes());
    expected[8..16].copy_from_slice(&4u64.to_be_bytes());
    expected[16] = 17;
    expected[17..25].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(scope.key(serial).unwrap(), expected);
    assert_eq!(scope.serial(&expected).unwrap(), serial);
    let absent = BaseFact {
        serial,
        value: None,
    };
    assert_eq!(absent.encode_value().unwrap(), [0; 74]);
    assert_eq!(BaseFact::decode(serial, &[0; 74]).unwrap(), absent);
    let mut malformed = [0; 74];
    malformed[73] = 1;
    assert!(BaseFact::decode(serial, &malformed).is_err());
    expected[16] = 18;
    assert!(scope.serial(&expected).is_err());
}
#[test]
fn independently_framed_ordered_absence_transcript_requires_exact_counts_and_last_key() {
    let scope = scope();
    let rows = [
        BaseFact {
            serial: 7,
            value: None,
        },
        BaseFact {
            serial: 255,
            value: None,
        },
    ];
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs/namespace-facts/v1\0");
    hasher.update(&scope.encode());
    for row in rows {
        hasher.update(&25u16.to_be_bytes());
        hasher.update(&scope.key(row.serial).unwrap());
        hasher.update(&74u32.to_be_bytes());
        hasher.update(&[0; 74]);
    }
    hasher.update(&2u64.to_be_bytes());
    hasher.update(&210u64.to_be_bytes());
    let mut ledger = FactLedger::new(scope).unwrap();
    ledger.base(&rows[..1]).unwrap();
    ledger.base(&rows[1..]).unwrap();
    let seal = ledger.seal();
    assert_eq!(
        (seal.records, seal.bytes, seal.maximum),
        (2, 210, Some(255))
    );
    assert_eq!(seal.digest, *hasher.finalize().as_bytes());
    assert!(ledger.base(&rows).is_err());
}
#[test]
fn live_sites_facts_aliases_graph_and_roots_share_one_exact_byte_limit() {
    let live = FactOccupancy {
        facts: 3,
        parents: 4,
        sites: 5,
        alias_facts: 6,
        alias_jobs: 7,
        graph_nodes: 8,
        graph_edges: 9,
        roots: 10,
    };
    // Prospective fixed1609 + exact live population widths; no max-overlap shortcut.
    let bytes = 1609 + 3 * 105 + 4 * 32 + 5 * 60 + 6 * 40 + 7 * 39 + 8 * 60 + 9 * 43 + 10 * 63;
    FactCapacity::new(3, 4, bytes).unwrap().check(live).unwrap();
    assert!(
        matches!(FactCapacity::new(3,4,bytes-1).unwrap().check(live), Err(ContentError::BoundedCapacityExceeded {what:"facts.aggregate_bytes",actual,..}) if actual==bytes)
    );
    assert!(matches!(
        FactCapacity::new(3, 3, bytes).unwrap().check(live),
        Err(ContentError::BoundedCapacityExceeded {
            what: "parents.records",
            limit: 3,
            actual: 4
        })
    ));
    assert!(FactCapacity::verified_empty()
        .check(FactOccupancy::default())
        .is_ok());
    assert!(FactCapacity::verified_empty()
        .check(FactOccupancy {
            facts: 1,
            ..Default::default()
        })
        .is_err());
}
#[test]
fn held_fact_page_credit_lasts_until_the_actual_records_owner_drops() {
    let memory = GraphMemory::new();
    let start = memory.reserved_bytes();
    let scope = scope();
    let mut ledger = FactLedger::new(scope).unwrap();
    let fact = BaseFact {
        serial: 1,
        value: None,
    };
    ledger.base(&[fact]).unwrap();
    let mut records = Vec::new();
    records.try_reserve_exact(128).unwrap();
    records.push(fact);
    let charge = std::mem::size_of::<FactPage<BaseFact>>()
        + records.capacity() * std::mem::size_of::<BaseFact>();
    let lease = memory.reserve(charge).unwrap();
    let page = FactPage::new(lease, ledger.seal(), records, Some(1), true).unwrap();
    assert_eq!(memory.reserved_bytes(), start + charge);
    assert!(memory.reserve(memory.limit() - start - charge + 1).is_err());
    let moved = page;
    assert_eq!(moved.records(), &[fact]);
    drop(moved);
    assert_eq!(memory.reserved_bytes(), start);
}
