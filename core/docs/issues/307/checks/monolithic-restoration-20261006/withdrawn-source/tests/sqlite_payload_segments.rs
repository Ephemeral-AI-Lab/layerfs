//! Immutable filesystem payloads through the ordinary public persistence port.
mod support;
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{Handles, PersistenceConfig, SqliteAcquisitionSchema, SqlitePackLayout};
use layerfs_storage::{
    location::{PackDomain, PackInfo},
    pack::{assemble, build_group, PackLane},
    port::*,
    StoragePolicy,
};
use std::{path::Path, sync::Arc};
fn config() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: b"payload-segments".to_vec(),
        cursor_key: [71; 32],
        incarnation: 1,
    }
}
fn create(path: &Path) -> Handles {
    Handles::create(
        PersistenceConfig::sqlite(path).with_sqlite_pack_layout(SqlitePackLayout::PayloadSegments),
        StoragePolicy::frozen_default(),
        &config(),
    )
    .unwrap()
}
fn pack(id: i64, size: usize) -> PublishedPack {
    let mut random = id as u64 + 932871;
    let record = (0..size)
        .map(|_| {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            random as u8
        })
        .collect::<Vec<_>>();
    let lane = if size > 262000 {
        PackLane::Singleton
    } else {
        PackLane::WholeFile
    };
    let body = assemble(lane, &[build_group(lane, &[record], None).unwrap()]).unwrap();
    PublishedPack {
        info: PackInfo {
            pack_id: id,
            domain: PackDomain::Payload,
            key: ObjectKey::for_bytes(&body),
            length: body.len(),
        },
        body: Arc::new(body),
    }
}
#[test]
fn one_publication_has_one_segment_and_reopens_exact_bodies() {
    let t = support::Temp::new("segments-roundtrip");
    let path = t.join("db");
    let h = create(&path);
    let packs = vec![pack(1, 90000), pack(2, 80000), pack(3, 100)];
    h.storage
        .publish(&Publication {
            packs: packs.clone(),
            ..Publication::default()
        })
        .unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM body_segment", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM pack WHERE body IS NULL", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(std::fs::read_dir(t.join("db.payload")).unwrap().count(), 1);
    drop(db);
    drop(h);
    let h = Handles::open_read_only(
        PersistenceConfig::sqlite(&path),
        &config().binding_key,
        config().cursor_key,
    )
    .unwrap();
    assert_eq!(h.profile().pack_layout, SqlitePackLayout::PayloadSegments);
    let mut actual = Vec::new();
    h.storage.read_packs(&[3, 2, 1], &mut actual).unwrap();
    for (got, expected) in actual.iter().zip(&packs) {
        assert_eq!(got.body(), expected.body.as_slice());
        assert_eq!(got.info(), expected.info);
    }
    assert!(h.storage.publish(&Publication::default()).is_err());
}
struct Plan(PackReadChoice);
impl PackReadPlan for Plan {
    fn select(&mut self, _: PackInfo, _: &[u8]) -> Result<PackReadChoice, PersistenceError> {
        Ok(self.0.clone())
    }
}
fn segment_path(t: &support::Temp) -> std::path::PathBuf {
    t.join("db.payload/0000000000000001.segment")
}
#[test]
fn strict_and_scoped_ranges_preserve_pack_authentication_and_exact_bytes() {
    let t = support::Temp::new("segments-ranges");
    let h = create(&t.join("db"));
    let p = pack(1, 90000);
    h.storage
        .publish(&Publication {
            packs: vec![p.clone()],
            ..Publication::default()
        })
        .unwrap();
    let range = PackRange {
        offset: 4000,
        length: 100,
    };
    let PersistedPackRead::Ranges(got) = h
        .storage
        .read_pack_selection(1, &mut Plan(PackReadChoice::Ranges(vec![range])))
        .unwrap()
    else {
        panic!("strict strategy");
    };
    assert_eq!(got.ranges()[0].1, p.body[4000..4100]);
    let before = h.diagnostics().unwrap();
    let AcquiredPackRead::Units(got) = h
        .storage
        .read_scoped_pack(1, &mut Plan(PackReadChoice::Ranges(vec![range])))
        .unwrap()
    else {
        panic!("scoped strategy");
    };
    assert_eq!(got.ranges()[0].1, p.body[4000..4100]);
    let after = h.diagnostics().unwrap();
    assert_eq!(
        after.segment_read_bytes - before.segment_read_bytes,
        got.acquired_bytes() as u64
    );
    assert!(got.acquired_bytes() < p.body.len() / 2);
    assert_eq!(after.blob_open_calls, before.blob_open_calls);
    assert_eq!(after.segment_close_calls - before.segment_close_calls, 2);
    assert!(matches!(
        h.storage
            .read_scoped_pack(2, &mut Plan(PackReadChoice::Whole)),
        Err(PersistenceError::Missing)
    ));
}
#[test]
fn missing_short_tampered_symlink_and_hardlinked_bodies_are_refused() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    for mutation in ["missing", "short", "tampered", "symlink", "hardlink"] {
        let t = support::Temp::new(mutation);
        let h = create(&t.join("db"));
        let p = pack(1, 90000);
        h.storage
            .publish(&Publication {
                packs: vec![p.clone()],
                ..Publication::default()
            })
            .unwrap();
        let body = segment_path(&t);
        std::fs::set_permissions(&body, std::fs::Permissions::from_mode(0o600)).unwrap();
        match mutation {
            "missing" => std::fs::remove_file(&body).unwrap(),
            "short" => std::fs::write(&body, b"short").unwrap(),
            "tampered" => {
                let mut bytes = p.body.to_vec();
                bytes[4000] ^= 1;
                std::fs::write(&body, bytes).unwrap();
            }
            "symlink" => {
                std::fs::rename(&body, t.join("other")).unwrap();
                symlink(t.join("other"), &body).unwrap();
            }
            "hardlink" => std::fs::hard_link(&body, t.join("other")).unwrap(),
            _ => unreachable!(),
        }
        assert!(
            h.storage.read_packs(&[1], &mut Vec::new()).is_err(),
            "{mutation}"
        );
        assert!(
            h.storage
                .read_pack_selection(1, &mut Plan(PackReadChoice::Whole))
                .is_err(),
            "{mutation}"
        );
        if mutation != "tampered" {
            assert!(
                h.storage
                    .read_scoped_pack(1, &mut Plan(PackReadChoice::Whole))
                    .is_err(),
                "{mutation}"
            );
        }
    }
}
#[test]
fn replaced_directory_and_wrong_extent_are_refused_without_repair() {
    let t = support::Temp::new("segments-directory");
    let path = t.join("db");
    let h = create(&path);
    h.storage
        .publish(&Publication {
            packs: vec![pack(1, 90000)],
            ..Publication::default()
        })
        .unwrap();
    std::fs::rename(t.join("db.payload"), t.join("original")).unwrap();
    std::fs::create_dir(t.join("db.payload")).unwrap();
    assert!(h.storage.read_packs(&[1], &mut Vec::new()).is_err());
    drop(h);
    assert!(Handles::open_read_only(
        PersistenceConfig::sqlite(&path),
        &config().binding_key,
        config().cursor_key
    )
    .is_err());
    std::fs::remove_dir(t.join("db.payload")).unwrap();
    std::fs::rename(t.join("original"), t.join("db.payload")).unwrap();
    let h = Handles::open_writable(
        PersistenceConfig::sqlite(&path),
        &config().binding_key,
        config().cursor_key,
    )
    .unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(
        "DROP TRIGGER pack_immutable_update; UPDATE pack SET segment_offset=1 WHERE pack_id=1",
    )
    .unwrap();
    drop(db);
    let before = h.diagnostics().unwrap();
    assert!(h.storage.read_packs(&[1], &mut Vec::new()).is_err());
    assert_eq!(
        h.diagnostics().unwrap().segment_read_calls,
        before.segment_read_calls
    );
}
#[test]
fn definite_catalogue_failure_retains_file_and_does_not_reuse_consumed_id() {
    let t = support::Temp::new("segments-rollback");
    let path = t.join("db");
    let h = create(&path);
    let mut batch = Publication {
        packs: vec![pack(1, 90000)],
        ..Publication::default()
    };
    batch
        .objects
        .push(layerfs_storage::location::ObjectLocation {
            object_id: layerfs_content::ObjectId::for_bytes(b"bad"),
            role: layerfs_content::ObjectRole::FileState,
            canonical_length: 1,
            pack_id: 999,
            group_number: 0,
            record_number: 0,
        });
    assert!(h.storage.publish(&batch).is_err());
    assert!(segment_path(&t).is_file());
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM pack", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM body_segment", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    drop(db);
    assert!(h
        .storage
        .publish(&Publication {
            packs: vec![pack(1, 90000)],
            ..Publication::default()
        })
        .is_err());
    h.storage
        .publish(&Publication {
            packs: vec![pack(2, 80000)],
            ..Publication::default()
        })
        .unwrap();
    assert_eq!(std::fs::read_dir(t.join("db.payload")).unwrap().count(), 2);
}
#[test]
fn reservations_canonical_storage_and_all_old_versions_keep_their_own_layout() {
    use layerfs_storage::Storage;
    for (layout, version) in [
        (SqlitePackLayout::Monolithic, 1),
        (SqlitePackLayout::GroupRows, 2),
        (SqlitePackLayout::GroupRowsIndexed, 3),
        (SqlitePackLayout::PayloadSegments, 7),
    ] {
        for acquisition in [
            SqliteAcquisitionSchema::Absent,
            SqliteAcquisitionSchema::Tables,
        ] {
            let t = support::Temp::new("segments-versions");
            let path = t.join("db");
            let h = Handles::create(
                PersistenceConfig::sqlite(&path)
                    .with_sqlite_pack_layout(layout)
                    .with_sqlite_acquisition(acquisition),
                StoragePolicy::frozen_default(),
                &config(),
            )
            .unwrap();
            let object = layerfs_content::FinalizedObject::new(
                layerfs_content::ObjectRole::WholeFile,
                layerfs_content::file::encode_whole_file_payload(&pack(17, 90000).body).unwrap(),
            )
            .unwrap();
            let storage = Storage::new(h.storage.clone()).unwrap();
            let save = storage.begin_save().unwrap();
            save.accept(object.clone()).unwrap();
            save.finish().unwrap();
            let root = object.id();
            drop(storage);
            drop(h);
            let h = Handles::open_read_only(
                PersistenceConfig::sqlite(&path)
                    .with_sqlite_pack_layout(SqlitePackLayout::PayloadSegments),
                &config().binding_key,
                config().cursor_key,
            )
            .unwrap();
            assert_eq!(h.profile().pack_layout, layout);
            assert_eq!(h.profile().acquisition, acquisition);
            assert_eq!(
                Storage::new(h.storage.clone())
                    .unwrap()
                    .reader()
                    .unwrap()
                    .read_objects(&[root])
                    .unwrap(),
                vec![object.canonical().to_vec()]
            );
            drop(h);
            let db = rusqlite::Connection::open(&path).unwrap();
            assert_eq!(
                db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                version
                    + if acquisition == SqliteAcquisitionSchema::Tables {
                        3
                    } else {
                        0
                    }
            );
        }
    }
}
#[test]
fn physical_charge_admission_precedes_creation_and_singleton_keeps_its_bound() {
    let t = support::Temp::new("segments-capacity");
    let h = create(&t.join("db"));
    let p = pack(1, 90000);
    assert_eq!(
        h.storage.publication_pack_cost(&p).unwrap(),
        (2, p.body.len() as u64 + 128)
    );
    let batch = Publication {
        packs: (1..49).map(|id| pack(id, 90000)).collect(),
        ..Publication::default()
    };
    assert!(h.storage.publish(&batch).is_err());
    assert_eq!(std::fs::read_dir(t.join("db.payload")).unwrap().count(), 0);
    let p = pack(50, 5 * 1024 * 1024);
    let batch = Publication {
        packs: vec![p.clone()],
        objects: vec![layerfs_storage::location::ObjectLocation {
            object_id: layerfs_content::ObjectId::for_bytes(b"singleton"),
            role: layerfs_content::ObjectRole::FileState,
            canonical_length: 5 * 1024 * 1024,
            pack_id: 50,
            group_number: 0,
            record_number: 0,
        }],
        ..Publication::default()
    };
    h.storage.publish(&batch).unwrap();
    let mut got = Vec::new();
    h.storage.read_packs(&[50], &mut got).unwrap();
    assert_eq!(got[0].body(), p.body.as_slice());
}
#[test]
fn concurrent_first_wins_publications_preserve_exact_lost_ids_and_custody() {
    use layerfs_content::{FinalizedObject, ObjectRole};
    use layerfs_storage::location::ObjectLocation;
    let t = support::Temp::new("segments-conflict");
    let path = t.join("db");
    let left = create(&path);
    let right = Handles::open_writable(
        PersistenceConfig::sqlite(&path),
        &config().binding_key,
        config().cursor_key,
    )
    .unwrap();
    let object = FinalizedObject::new(
        ObjectRole::WholeFile,
        layerfs_content::file::encode_whole_file_payload(&pack(19, 90000).body).unwrap(),
    )
    .unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let mut threads = Vec::new();
    for (h, id) in [(left, 1), (right, 2)] {
        let barrier = barrier.clone();
        let object = object.clone();
        threads.push(std::thread::spawn(move || {
            let p = pack(id, 90000);
            let batch = Publication {
                packs: vec![p],
                objects: vec![ObjectLocation {
                    object_id: object.id(),
                    role: object.role(),
                    canonical_length: object.canonical_len(),
                    pack_id: id,
                    group_number: 0,
                    record_number: 0,
                }],
                ..Publication::default()
            };
            barrier.wait();
            h.storage.publish(&batch)
        }));
    }
    let outcomes = threads
        .into_iter()
        .map(|t| t.join().unwrap())
        .collect::<Vec<_>>();
    assert!(outcomes.iter().any(Result::is_ok));
    assert_eq!(
        outcomes
            .iter()
            .filter(|r| matches!(r,Ok(p) if p.lost.is_empty()))
            .count(),
        1
    );
    for result in outcomes {
        match result {
            Ok(p) => assert!(p.lost.is_empty() || p.lost == vec![object.id()]),
            Err(PersistenceError::Refused { status }) => assert_eq!(status, "Busy"),
            Err(e) => panic!("{e:?}"),
        }
    }
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM object_location", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM body_segment", [], |r| r
            .get::<_, i64>(0))
            .unwrap() as usize,
        std::fs::read_dir(t.join("db.payload")).unwrap().count()
    );
}
#[test]
fn segment_crash_child() {
    use std::io::Write;
    let Some(path) = std::env::var_os("LAYERFS_SEGMENT_CRASH_CHILD") else {
        return;
    };
    let h = create(Path::new(&path));
    let id = h
        .storage
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .unwrap()
        .first_pack_id;
    assert_eq!(id, 1);
    h.storage
        .publish(&Publication {
            packs: vec![pack(id, 90000)],
            ..Publication::default()
        })
        .unwrap();
    println!("SEGMENT_CATALOGUE_ACKNOWLEDGED");
    std::io::stdout().flush().unwrap();
    // Bounded parent rendezvous; the child cannot wait forever after a failed test.
    std::thread::sleep(std::time::Duration::from_secs(2));
    drop(h);
}
#[test]
fn killed_live_owner_preserves_acknowledged_segment_catalogue_and_ids() {
    use std::io::{BufRead, BufReader};
    let t = support::Temp::new("segments-kill");
    let path = t.join("db");
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "segment_crash_child", "--nocapture"])
        .env("LAYERFS_SEGMENT_CRASH_CHILD", &path)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    let output = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().take(16) {
            if line.unwrap().contains("SEGMENT_CATALOGUE_ACKNOWLEDGED") {
                let _ = send.send(());
                break;
            }
        }
    });
    let ack = receive.recv_timeout(std::time::Duration::from_secs(3));
    let killed = child.kill();
    let exit = child.wait().unwrap();
    output.join().unwrap();
    ack.unwrap();
    killed.unwrap();
    assert!(!exit.success());
    let h = Handles::open_writable(
        PersistenceConfig::sqlite(&path),
        &config().binding_key,
        config().cursor_key,
    )
    .unwrap();
    let mut got = Vec::new();
    h.storage.read_packs(&[1], &mut got).unwrap();
    assert_eq!(got[0].body(), pack(1, 90000).body.as_slice());
    assert_eq!(
        h.storage
            .reserve(Reserve {
                packs: 1,
                ordinals: 0
            })
            .unwrap()
            .first_pack_id,
        2
    );
    assert_eq!(std::fs::read_dir(t.join("db.payload")).unwrap().count(), 1);
}
