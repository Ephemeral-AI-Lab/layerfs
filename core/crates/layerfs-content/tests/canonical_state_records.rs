//! Literal count/zero/release wire and simultaneous-capacity oracles.
use layerfs_content::filesystem::references::record::Row;
use layerfs_content::filesystem::rows::BindingAuthority;
use layerfs_content::filesystem::state::*;
use layerfs_content::filesystem::{scope_for_seed, PathName};
use layerfs_content::ObjectId;
fn scope() -> CanonicalScope {
    let source = BindingAuthority::new().unwrap();
    let subject = GraphSubject::new(
        source.source_id(),
        scope_for_seed([0x24; 32]),
        None,
        1,
        GraphCapacity::default(),
    )
    .unwrap();
    let mut selection = StateSelection::issue([0x34; 32]).unwrap();
    selection.bind_owner([0x54; 32]).unwrap();
    CanonicalScope::counts(selection, FactSubject::new(subject, None).unwrap()).unwrap()
}
#[test]
fn count_record_uses_literal_original_row_and_monotone_touched_byte() {
    let scope = scope();
    let serial = 0x0102_0304_0506_0708;
    let row = CountRecord {
        row: Row::Effect {
            serial,
            value: None,
            delta: -9,
        },
        touched: true,
    };
    let mut expected = [0u8; 97];
    expected[..8].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
    expected[8] = 1;
    expected[9] = 2;
    expected[10..12].copy_from_slice(&96u16.to_be_bytes());
    expected[13..21].copy_from_slice(&(-9i64).to_be_bytes());
    expected[96] = 1;
    assert_eq!(row.encode_value().unwrap(), expected);
    assert_eq!(
        CountRecord::decode(&scope, &scope.key(serial).unwrap(), &expected).unwrap(),
        row
    );
    expected[96] = 2;
    assert!(CountRecord::decode(&scope, &scope.key(serial).unwrap(), &expected).is_err());
    expected[96] = 1;
    assert!(CountRecord::decode(&scope, &scope.key(serial + 1).unwrap(), &expected).is_err());
}
#[test]
fn independent_count_transcript_includes_untouched_declaration_and_separate_epoch() {
    let scope = scope();
    let rows = [
        CountRecord {
            row: Row::Count {
                serial: 2,
                value: None,
                count: 0,
            },
            touched: false,
        },
        CountRecord {
            row: Row::Effect {
                serial: 5,
                value: None,
                delta: -1,
            },
            touched: true,
        },
    ];
    let mut hash = blake3::Hasher::new();
    hash.update(b"layerfs/reference-counts/v1\0");
    hash.update(&scope.encode());
    hash.update(&[1]);
    for row in rows {
        hash.update(&25u16.to_be_bytes());
        hash.update(&scope.key(row.serial()).unwrap());
        hash.update(&97u32.to_be_bytes());
        hash.update(&row.encode_value().unwrap());
    }
    hash.update(&2u64.to_be_bytes());
    hash.update(&1u64.to_be_bytes());
    hash.update(&256u64.to_be_bytes());
    let mut ledger = CountLedger::new(scope.clone(), CountEpoch::Effects).unwrap();
    ledger.append(&rows).unwrap();
    let seal = ledger.seal().unwrap();
    assert_eq!(
        (seal.records, seal.touched, seal.bytes, seal.maximum),
        (2, 1, 256, Some(5))
    );
    assert_eq!(seal.digest, *hash.finalize().as_bytes());
    assert_eq!(seal.encode().len(), 294);
    let mut final_ledger = CountLedger::new(scope, CountEpoch::Final).unwrap();
    final_ledger.append(&rows).unwrap();
    assert_ne!(seal.digest, final_ledger.seal().unwrap().digest);
    assert!(ZeroLedger::new(final_ledger.seal().unwrap()).is_err());
}
#[test]
fn release_full_name255_utf8_and_absent_eof_have_literal_width322() {
    let scope = scope().frames().unwrap();
    let name = "é".repeat(127) + "x";
    assert_eq!(name.len(), 255);
    let frame = ReleaseFrame {
        depth: 0x0102_0304_0506_0708,
        root: ObjectId::from_bytes(&[0x71; 32]).unwrap(),
        after: Some(ReleaseName::new(&name).unwrap()),
        finished: false,
    };
    assert_eq!(
        ReleaseName::from_path_name(&PathName::new(&name).unwrap()),
        frame.after.unwrap()
    );
    assert_eq!(frame.after.unwrap().as_bytes(), name.as_bytes());
    let encoded = frame.encode_value();
    assert_eq!(&encoded[..32], &[0x71; 32]);
    assert_eq!(encoded[32], 1);
    assert_eq!(&encoded[33..35], &255u16.to_be_bytes());
    assert_eq!(&encoded[35..290], name.as_bytes());
    assert_eq!(encoded[290], 0);
    assert_eq!(
        ReleaseFrame::decode(&scope, &scope.key(frame.depth).unwrap(), &encoded).unwrap(),
        frame
    );
    let eof = ReleaseFrame {
        after: None,
        finished: true,
        ..frame
    };
    frame.advances_to(eof).unwrap();
    assert_eq!(&eof.encode_value()[32..290], &[0; 258]);
    assert_eq!(eof.encode_value()[290], 1);
    assert!(frame.advances_to(frame).is_err());
    assert!(eof.advances_to(frame).is_err());
    let mut malformed = encoded;
    malformed[35] = 0xff;
    assert!(ReleaseFrame::decode(&scope, &scope.key(frame.depth).unwrap(), &malformed).is_err());
}
#[test]
fn facts_counts_roots_seeds_jobs_and_frames_share_the_exact_same_byte_ceiling() {
    let live = CanonicalOccupancy {
        namespace: FactOccupancy {
            facts: 3,
            parents: 4,
            roots: 5,
            ..Default::default()
        },
        counts: 6,
        zeros: 7,
        jobs: 8,
        frames: 9,
    };
    let bytes = 3657 + 3 * 105 + 4 * 32 + 5 * 63 + 6 * 128 + 7 * 105 + 8 * 113 + 9 * 322;
    CanonicalCapacity::new(6, 7, 8, 9, bytes)
        .unwrap()
        .check(live)
        .unwrap();
    assert!(CanonicalCapacity::new(6, 7, 8, 9, bytes - 1)
        .unwrap()
        .check(live)
        .is_err());
    assert!(CanonicalCapacity::new(6, 7, 8, 8, bytes)
        .unwrap()
        .check(live)
        .is_err());
    assert!(CanonicalCapacity::verified_empty()
        .check(CanonicalOccupancy::default())
        .is_ok());
    assert!(CanonicalCapacity::verified_empty()
        .check(CanonicalOccupancy {
            counts: 1,
            ..Default::default()
        })
        .is_err());
}
