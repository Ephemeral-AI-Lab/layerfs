//! Durable bounded physical units exercised through the application-facing port.
mod support;
use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};
use layerfs_history::{HistoryCatalog, HistoryCatalogConfig, ReserveRequest};
use layerfs_persistence::{BackendSelection, Handles, PersistenceConfig};
use layerfs_storage::{
    location::{ObjectLocation, PackDomain, PackInfo, SignatureRow},
    pack::{assemble, build_group, PackLane},
    port::*,
    Storage, StoragePolicy,
};
use std::sync::Arc;
use support::Temp;
fn config() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: b"durable-publication".to_vec(),
        cursor_key: [71; 32],
        incarnation: 7,
    }
}
fn create(path: &std::path::Path) -> Handles {
    Handles::create(
        PersistenceConfig::sqlite(path),
        StoragePolicy::frozen_default(),
        &config(),
    )
    .unwrap()
}
fn unit(pack_id: i64, objects: usize) -> Publication {
    let body = assemble(
        PackLane::WholeFile,
        &[build_group(PackLane::WholeFile, &[vec![0x10; 64]], None).unwrap()],
    )
    .unwrap();
    Publication {
        packs: vec![PublishedPack {
            info: PackInfo {
                pack_id,
                domain: PackDomain::Payload,
                key: ObjectKey::for_bytes(&body),
                length: body.len(),
            },
            body: Arc::new(body),
        }],
        objects: (0..objects)
            .map(|n| ObjectLocation {
                object_id: ObjectId::for_bytes(&n.to_be_bytes()),
                role: ObjectRole::WholeFile,
                canonical_length: 64,
                pack_id,
                group_number: 0,
                record_number: 0,
            })
            .collect(),
        ..Publication::default()
    }
}
#[test]
fn wal_full_settings_and_unavailable_engine_are_explicit() {
    let t = Temp::new("durable-profile");
    let path = t.join("db");
    let mut c = PersistenceConfig::sqlite(&path);
    c.backend = BackendSelection::Postgres;
    assert_eq!(
        Handles::create(c, StoragePolicy::frozen_default(), &config()).err(),
        Some(PersistenceError::BackendUnavailable)
    );
    assert!(!path.exists());
    let h = create(&path);
    let p = h.profile();
    assert_eq!(p.identity, "sqlite-wal-full-macos-fullfsync-v1");
    assert_eq!(
        (
            &*p.journal_mode,
            p.synchronous,
            p.foreign_keys,
            p.fullfsync,
            p.checkpoint_fullfsync
        ),
        ("wal", 2, 1, 1, 1)
    );
    assert_eq!(
        (
            p.page_size,
            p.busy_timeout,
            p.mmap_size,
            p.wal_autocheckpoint,
            p.cache_size
        ),
        (4096, 0, 0, 1000, -2048)
    );
    assert!(p.variable_limit >= 6);
    assert!(p.column_limit >= 13);
    assert!(p.length_limit >= layerfs_storage::policy::SINGLETON_PACK_LIMIT + 1024);
    assert!(p.sql_length_limit > 192);
    let names = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type='table'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap();
    assert_eq!(names, 12);
}
#[test]
fn final_body_multirow_locators_and_first_wins_have_one_atomic_ack() {
    let t = Temp::new("bulk-ack");
    let h = create(&t.join("db"));
    let block = h
        .storage
        .reserve(Reserve {
            packs: 2,
            ordinals: 0,
        })
        .unwrap();
    let batch = unit(block.first_pack_id, 512);
    let before = h.diagnostics().unwrap();
    assert!(h.storage.publish(&batch).unwrap().lost.is_empty());
    let after = h.diagnostics().unwrap();
    assert_eq!(after.transactions - before.transactions, 1);
    assert_eq!(after.commits - before.commits, 1);
    assert_eq!(after.sealed_inserts - before.sealed_inserts, 1);
    // Two control statements, one body INSERT and bounded set-based locator INSERTs.
    assert_eq!(
        after.statements - before.statements,
        3 + 512_u64.div_ceil(
            (h.profile().variable_limit / 6).min((h.profile().sql_length_limit - 192) / 32) as u64
        )
    );
    assert!(after.vm_steps > before.vm_steps);
    assert_eq!(
        after.sealed_body_bytes - before.sealed_body_bytes,
        batch.packs[0].body.len() as u64
    );
    let mut second = unit(block.first_pack_id + 1, 512);
    second.objects.reverse();
    let lost = h.storage.publish(&second).unwrap().lost;
    assert_eq!(
        lost,
        second
            .objects
            .iter()
            .map(|o| o.object_id)
            .collect::<Vec<_>>()
    );
    let mut packs = Vec::new();
    h.storage
        .read_packs(&[block.first_pack_id], &mut packs)
        .unwrap();
    assert_eq!(packs[0].body, *batch.packs[0].body);
}
#[test]
fn failure_after_body_insert_rolls_back_the_entire_unit_and_published_body_is_immutable() {
    let t = Temp::new("atomic-rollback");
    let path = t.join("db");
    let h = create(&path);
    let id = h
        .storage
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .unwrap()
        .first_pack_id;
    let mut bad = unit(id, 1);
    bad.objects[0].pack_id = id + 500;
    let before = h.diagnostics().unwrap();
    assert!(h.storage.publish(&bad).is_err());
    let after = h.diagnostics().unwrap();
    assert_eq!(after.rollbacks - before.rollbacks, 1);
    let mut packs = Vec::new();
    assert_eq!(
        h.storage.read_packs(&[id], &mut packs),
        Err(PersistenceError::Missing)
    );
    let good = unit(id, 1);
    h.storage.publish(&good).unwrap();
    let sql = rusqlite::Connection::open(&path).unwrap();
    assert!(sql
        .execute(
            "UPDATE pack SET body=zeroblob(length) WHERE pack_id=?1",
            [id]
        )
        .is_err());
    assert!(sql
        .execute("DELETE FROM pack WHERE pack_id=?1", [id])
        .is_err());
    let mut malformed = unit(id + 1, 1);
    malformed.packs[0].info.key = ObjectKey::for_bytes(b"wrong");
    let before = h.diagnostics().unwrap();
    assert_eq!(
        h.storage.publish(&malformed),
        Err(PersistenceError::Malformed)
    );
    assert_eq!(h.diagnostics().unwrap().transactions, before.transactions);
}
#[test]
fn readonly_refusal_and_external_writer_busy_do_not_retry() {
    let t = Temp::new("authority-busy");
    let path = t.join("db");
    let h = create(&path);
    let reader = Handles::open_read_only(
        PersistenceConfig::sqlite(&path),
        &config().binding_key,
        [71; 32],
    )
    .unwrap();
    let before = reader.diagnostics().unwrap();
    assert!(reader
        .storage
        .reserve(Reserve {
            packs: 1,
            ordinals: 0
        })
        .is_err());
    assert_eq!(reader.diagnostics().unwrap().statements, before.statements);
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch("BEGIN IMMEDIATE").unwrap();
    let before = h.diagnostics().unwrap();
    assert_eq!(
        h.storage.reserve(Reserve {
            packs: 1,
            ordinals: 0
        }),
        Err(PersistenceError::Refused {
            status: "Busy".to_owned()
        })
    );
    let after = h.diagnostics().unwrap();
    assert_eq!(after.statements - before.statements, 1);
    assert_eq!(after.transactions, before.transactions);
    sql.execute_batch("ROLLBACK").unwrap();
}
#[test]
fn canonical_save_read_reuse_and_persisted_reservations_survive_reopen() {
    let t = Temp::new("canonical-reopen");
    let path = t.join("db");
    let h = create(&path);
    let storage = Storage::new(h.storage.clone()).unwrap();
    let objects = (0..600)
        .map(|n| {
            FinalizedObject::new(
                ObjectRole::FileState,
                layerfs_content::object::codec::encode_bytes_object(&[n as u8; 200]).unwrap(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let save = storage.begin_save().unwrap();
    for o in &objects {
        save.accept(o.clone()).unwrap();
    }
    let outcome = save.finish().unwrap();
    assert_eq!(outcome.inserted, 256);
    assert_eq!(outcome.reused, 344);
    let reader = storage.reader().unwrap();
    for page in objects.chunks(128) {
        assert_eq!(
            reader
                .read_objects(&page.iter().map(FinalizedObject::id).collect::<Vec<_>>())
                .unwrap(),
            page.iter()
                .map(|o| o.canonical().to_vec())
                .collect::<Vec<_>>()
        );
    }
    let scope = ObjectId::for_bytes(b"scope");
    let r = h
        .history
        .reserve_inodes(&ReserveRequest { scope, count: 7 })
        .unwrap();
    assert_eq!(r.start, 1);
    drop(reader);
    drop(storage);
    drop(h);
    let h = Handles::open_writable(
        PersistenceConfig::sqlite(&path),
        &config().binding_key,
        [71; 32],
    )
    .unwrap();
    assert_eq!(
        h.history
            .reserve_inodes(&ReserveRequest { scope, count: 1 })
            .unwrap()
            .start,
        8
    );
    assert!(!h.checkpoint().unwrap().busy);
}

#[test]
fn writable_open_does_not_migrate_an_unrelated_database_journal() {
    let t = Temp::new("foreign-journal");
    let path = t.join("db");
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch("CREATE TABLE marker(value TEXT); INSERT INTO marker VALUES('untouched');")
        .unwrap();
    assert!(Handles::open_writable(
        PersistenceConfig::sqlite(&path),
        &config().binding_key,
        [71; 32]
    )
    .is_err());
    let mode: String = sql
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "delete");
    assert_eq!(
        sql.query_row("SELECT value FROM marker", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "untouched"
    );
    assert!(!std::path::PathBuf::from(format!("{}-wal", path.display())).exists());
}

#[test]
fn signature_publication_is_bounded_set_based_and_preserves_stamps() {
    let t = Temp::new("bulk-signatures");
    let h = create(&t.join("db"));
    let id = h
        .storage
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .unwrap()
        .first_pack_id;
    let seed = unit(id, 1);
    let object = seed.objects[0].object_id;
    h.storage.publish(&seed).unwrap();
    let batch = Publication {
        signatures: (0..512)
            .map(|slot| SignatureRow {
                slot,
                stamp: slot as u64 + 1,
                object_id: object,
                signature: [slot as u8; 32],
            })
            .collect(),
        ..Publication::default()
    };
    let before = h.diagnostics().unwrap();
    h.storage.publish(&batch).unwrap();
    let after = h.diagnostics().unwrap();
    // One atomic unit, with one SQL signature page under this host's actual limits.
    assert_eq!(after.statements - before.statements, 3);
    assert_eq!(after.write_commits - before.write_commits, 1);
    let mut rows = Vec::new();
    h.storage.signatures(&mut rows).unwrap();
    assert_eq!(rows, batch.signatures);
    for (stamp, value) in [(8193, 9), (1, 8), (8193, 7)] {
        h.storage
            .publish(&Publication {
                signatures: vec![SignatureRow {
                    slot: 0,
                    stamp,
                    object_id: object,
                    signature: [value; 32],
                }],
                ..Publication::default()
            })
            .unwrap();
    }
    rows.clear();
    h.storage.signatures(&mut rows).unwrap();
    assert_eq!(rows.last().unwrap().stamp, 8193);
    assert_eq!(rows.last().unwrap().signature, [7; 32]);
}
#[test]
fn failing_signature_page_rolls_back_prior_body_and_all_signature_rows() {
    let t = Temp::new("signature-rollback");
    let h = create(&t.join("db"));
    let first = h
        .storage
        .reserve(Reserve {
            packs: 2,
            ordinals: 0,
        })
        .unwrap()
        .first_pack_id;
    let seed = unit(first, 1);
    let object = seed.objects[0].object_id;
    h.storage.publish(&seed).unwrap();
    let mut bad = unit(first + 1, 1);
    bad.signatures = (0..513)
        .map(|slot| SignatureRow {
            slot,
            stamp: slot as u64 + 1,
            object_id: object,
            signature: [slot as u8; 32],
        })
        .collect();
    bad.signatures.last_mut().unwrap().object_id = ObjectId::for_bytes(b"absent-signature-owner");
    let before = h.diagnostics().unwrap();
    assert!(h.storage.publish(&bad).is_err());
    let after = h.diagnostics().unwrap();
    assert_eq!(after.rollbacks - before.rollbacks, 1);
    assert_eq!(after.write_commits, before.write_commits);
    let mut packs = Vec::new();
    assert_eq!(
        h.storage.read_packs(&[first + 1], &mut packs),
        Err(PersistenceError::Missing)
    );
    let mut rows = Vec::new();
    h.storage.signatures(&mut rows).unwrap();
    assert!(rows.is_empty());
}

#[test]
fn signature_pages_share_one_acknowledgement_at_the_existing_row_bound() {
    let t = Temp::new("signature-pages");
    let h = create(&t.join("db"));
    let id = h
        .storage
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .unwrap()
        .first_pack_id;
    let seed = unit(id, 1);
    let object = seed.objects[0].object_id;
    h.storage.publish(&seed).unwrap();
    let batch = Publication {
        signatures: (0..513)
            .map(|slot| SignatureRow {
                slot,
                stamp: slot as u64 + 1,
                object_id: object,
                signature: [slot as u8; 32],
            })
            .collect(),
        ..Publication::default()
    };
    let before = h.diagnostics().unwrap();
    h.storage.publish(&batch).unwrap();
    let after = h.diagnostics().unwrap();
    assert_eq!(after.statements - before.statements, 4);
    assert_eq!(after.write_commits - before.write_commits, 1);
    let mut rows = Vec::new();
    h.storage.signatures(&mut rows).unwrap();
    assert_eq!(rows, batch.signatures);
}
