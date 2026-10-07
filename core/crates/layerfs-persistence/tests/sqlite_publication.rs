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
    assert_eq!(p.identity, "sqlite-wal-full-v2");
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
    assert_eq!(packs[0].body(), batch.packs[0].body.as_slice());
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
        Err(PersistenceError::Busy)
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
    h.seal().unwrap();
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

#[test]
fn pack_insert_pages_preserve_bodies_and_atomic_conflict_refusal() {
    let t = Temp::new("pack-insert-pages");
    let h = create(&t.join("db"));
    let mut batch = Publication::default();
    // Cross the 512-row statement bound while staying below transaction bytes.
    for id in 1..=513 {
        batch.packs.extend(unit(id, 0).packs);
    }
    let before = h.diagnostics().unwrap();
    assert!(h.storage.publish(&batch).unwrap().lost.is_empty());
    let after = h.diagnostics().unwrap();
    // BEGIN, two pack INSERT pages and COMMIT; no per-pack statements.
    assert_eq!(after.statements - before.statements, 4);
    assert_eq!(after.write_commits - before.write_commits, 1);
    assert_eq!(after.sealed_inserts - before.sealed_inserts, 513);
    let mut read = Vec::new();
    h.storage.read_packs(&[1, 512, 513], &mut read).unwrap();
    assert_eq!(read.len(), 3);
    for pack in read {
        assert_eq!(
            pack.body(),
            batch.packs[pack.info().pack_id as usize - 1]
                .body
                .as_slice()
        );
    }
    let mut conflict = Publication::default();
    // A new first page must also roll back when the second page conflicts.
    for id in 1000..1512 {
        conflict.packs.extend(unit(id, 0).packs);
    }
    conflict.packs.extend(unit(1, 0).packs);
    assert!(h.storage.publish(&conflict).is_err());
    let mut absent = Vec::new();
    assert_eq!(
        h.storage.read_packs(&[1000, 1511], &mut absent),
        Err(PersistenceError::Missing)
    );
    assert!(absent.is_empty());
    let mut existing = Vec::new();
    h.storage.read_packs(&[1], &mut existing).unwrap();
    assert_eq!(existing[0].body(), batch.packs[0].body.as_slice());
}

#[test]
fn pack_insert_pages_keep_large_blob_binding_ownership_separate() {
    let t = Temp::new("pack-page-byte-bound");
    let h = create(&t.join("db"));
    let group = build_group(PackLane::WholeFile, &[vec![0x10; 64000]], None).unwrap();
    let body = assemble(PackLane::WholeFile, &[group.clone(), group.clone(), group]).unwrap();
    assert!(body.len() > layerfs_storage::policy::PACK_LIMIT / 2);
    let mut batch = Publication::default();
    for id in 1..=3 {
        let mut p = unit(id, 0).packs.remove(0);
        p.info.key = ObjectKey::for_bytes(&body);
        p.info.length = body.len();
        p.body = Arc::new(body.clone());
        batch.packs.push(p);
    }
    let before = h.diagnostics().unwrap();
    h.storage.publish(&batch).unwrap();
    let after = h.diagnostics().unwrap();
    assert_eq!(after.statements - before.statements, 5);
    assert_eq!(after.write_commits - before.write_commits, 1);
    let mut read = Vec::new();
    h.storage.read_packs(&[1, 2, 3], &mut read).unwrap();
    assert_eq!(read.len(), 3);
    assert!(read.iter().all(|p| p.body() == body.as_slice()));
}

#[test]
fn explicit_profiles_keep_atomic_publication_and_matching_reopen_contracts() {
    use layerfs_persistence::SqlitePersistenceProfile::Durable;
    for selected in DEVELOPMENT_PROFILES {
        let t = Temp::new("explicit-profile");
        let path = t.join("db");
        let cfg = PersistenceConfig::sqlite(&path).with_sqlite_profile(selected);
        let h = Handles::create(cfg.clone(), StoragePolicy::frozen_default(), &config()).unwrap();
        let profile = h.profile();
        assert_eq!(profile.persistence, selected);
        assert_eq!(profile.journal_mode, "wal");
        assert_eq!(profile.synchronous, if selected == Durable { 2 } else { 0 });
        assert_eq!(profile.fullfsync, i64::from(selected == Durable));
        assert_eq!((profile.checkpoint_fullfsync, profile.temp_store), (1, 2));
        assert_eq!(
            (profile.page_size, profile.cache_size, profile.mmap_size),
            (4096, -2048, 0)
        );
        let storage = Storage::new(h.storage.clone()).unwrap();
        let object = FinalizedObject::new(
            ObjectRole::FileState,
            layerfs_content::object::codec::encode_bytes_object(&[17; 4096]).unwrap(),
        )
        .unwrap();
        let save = storage.begin_save().unwrap();
        save.accept(object.clone()).unwrap();
        assert_eq!(save.finish().unwrap().inserted, 1);
        // Failure after a new body INSERT must abort the whole unit in both profiles.
        let mut invalid = unit(9999, 1);
        invalid.objects[0].pack_id = 9998;
        assert!(h.storage.publish(&invalid).is_err());
        let mut absent = Vec::new();
        assert_eq!(
            h.storage.read_packs(&[9999], &mut absent),
            Err(PersistenceError::Missing)
        );
        drop(storage);
        let completed = h.seal().unwrap();
        assert_eq!(completed.profile, selected);
        assert!(!t.join("db-wal").exists());
        assert!(!t.join("db-shm").exists());
        // Both profiles use WAL; mixed-profile provisioning cannot be detected.
        let reopened =
            Handles::open_writable(cfg.clone(), b"durable-publication", [71; 32]).unwrap();
        let read = Storage::new(reopened.storage.clone()).unwrap();
        assert_eq!(
            read.reader().unwrap().read_objects(&[object.id()]).unwrap(),
            vec![object.canonical().to_vec()]
        );
        drop(read);
        drop(reopened);
        let readonly = Handles::open_read_only(cfg, b"durable-publication", [71; 32]).unwrap();
        assert_eq!(readonly.profile().persistence, selected);
        let before = readonly.diagnostics().unwrap();
        assert!(readonly
            .storage
            .reserve(Reserve {
                packs: 1,
                ordinals: 0
            })
            .is_err());
        assert_eq!(
            readonly.diagnostics().unwrap().statements,
            before.statements
        );
        assert!(readonly.seal().is_err());
    }
}

#[test]
fn multi_page_publication_preserves_first_wins_loss_order_and_late_failure_atomicity() {
    for profile in DEVELOPMENT_PROFILES {
        let temp = Temp::new("publication-subpages");
        let h = Handles::create(
            PersistenceConfig::sqlite(temp.join("db")).with_sqlite_profile(profile),
            StoragePolicy::frozen_default(),
            &config(),
        )
        .unwrap();
        let block = h
            .storage
            .reserve(Reserve {
                packs: 3,
                ordinals: 0,
            })
            .unwrap();
        let first = unit(block.first_pack_id, 1025);
        let before = h.diagnostics().unwrap();
        assert!(h.storage.publish(&first).unwrap().lost.is_empty());
        let after = h.diagnostics().unwrap();
        assert_eq!(after.transactions - before.transactions, 1);
        assert_eq!(after.write_commits - before.write_commits, 1);
        let mut conflict = unit(block.first_pack_id + 1, 1025);
        conflict.objects.reverse();
        assert_eq!(
            h.storage.publish(&conflict).unwrap().lost,
            conflict
                .objects
                .iter()
                .map(|o| o.object_id)
                .collect::<Vec<_>>()
        );
        let mut failing = unit(block.first_pack_id + 2, 1537);
        failing.objects.last_mut().unwrap().pack_id += 1000;
        let absent = failing.objects[1200].object_id;
        let before = h.diagnostics().unwrap();
        assert!(h.storage.publish(&failing).is_err());
        assert_eq!(h.diagnostics().unwrap().rollbacks - before.rollbacks, 1);
        let mut located = Vec::new();
        h.storage.locate(&[absent], &mut located).unwrap();
        assert!(located.is_empty());
        let mut bodies = Vec::new();
        assert_eq!(
            h.storage
                .read_packs(&[block.first_pack_id + 2], &mut bodies),
            Err(PersistenceError::Missing)
        );
    }
}

/// Owner direction 2026-10-07: Disposable is the sole active development
/// verification profile. Durable execution of these bodies is NOT_RUN —
/// deferred by owner for Disposable-only development. Durable support and its
/// retained historical receipts are unchanged.
const DEVELOPMENT_PROFILES: [layerfs_persistence::SqlitePersistenceProfile; 1] =
    [layerfs_persistence::SqlitePersistenceProfile::Disposable];
