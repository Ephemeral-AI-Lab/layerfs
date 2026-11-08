//! Integrity diagnosis against real Disposable databases, including live WAL.
mod support;
use layerfs_content::{encode_whole_file_payload, FinalizedObject, ObjectRole};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{
    diagnostic::{Observation, Options, Outcome},
    PersistenceConfig, SqliteAcquisitionSchema, SqliteHandles, SqlitePackLayout,
    SqlitePersistenceProfile,
};
use layerfs_storage::{Storage, StoragePolicy};
use std::time::Instant;

fn authority() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: b"doctor-fixture".to_vec(),
        cursor_key: [71; 32],
        incarnation: 7,
    }
}
fn config(path: &std::path::Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable)
}
fn observations(handles: &SqliteHandles, quick: bool) -> Vec<Observation> {
    let mut result = Vec::new();
    handles
        .diagnose(
            Options {
                quick,
                deadline: None,
                cancelled: None,
            },
            &mut |o| result.push(o),
        )
        .unwrap();
    result
}
fn put(handles: &SqliteHandles) {
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    save.accept(
        FinalizedObject::new(
            ObjectRole::WholeFile,
            encode_whole_file_payload(b"authentic retained payload").unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    save.finish().unwrap();
}

#[test]
fn all_supported_layouts_include_live_wal_without_source_writes() {
    for layout in [
        SqlitePackLayout::Monolithic,
        SqlitePackLayout::GroupRows,
        SqlitePackLayout::GroupRowsIndexed,
    ] {
        for acquisition in [
            SqliteAcquisitionSchema::Absent,
            SqliteAcquisitionSchema::Tables,
        ] {
            let temp = support::Temp::new("doctor-layout");
            let path = temp.join("store.sqlite");
            let authority = authority();
            let config = config(&path)
                .with_sqlite_pack_layout(layout)
                .with_sqlite_acquisition(acquisition);
            let writer =
                SqliteHandles::create(config.clone(), StoragePolicy::frozen_default(), &authority)
                    .unwrap();
            put(&writer);
            let reader =
                SqliteHandles::open_read_only(config, &authority.binding_key, authority.cursor_key)
                    .unwrap();
            let before = reader.diagnostics().unwrap();
            let result = observations(&reader, false);
            let after = reader.diagnostics().unwrap();
            assert!(
                !result.iter().any(|o| o.outcome == Outcome::Fail),
                "{result:?}"
            );
            assert!(
                result
                    .iter()
                    .any(|o| o.id == "store.objects.summary" && o.message.starts_with("1 winning")),
                "{result:?}"
            );
            assert_eq!(after.write_transactions, before.write_transactions);
            assert_eq!(after.write_commits, before.write_commits);
        }
    }
}

#[test]
fn retained_object_corruption_with_resealed_pack_is_detected_and_quick_excludes_it() {
    let temp = support::Temp::new("doctor-content");
    let path = temp.join("store.sqlite");
    let authority = authority();
    let writer =
        SqliteHandles::create(config(&path), StoragePolicy::frozen_default(), &authority).unwrap();
    put(&writer);
    let raw = rusqlite::Connection::open(&path).unwrap();
    let trigger: String = raw
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE name='pack_immutable_update'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    raw.execute_batch("DROP TRIGGER pack_immutable_update;")
        .unwrap();
    let (id, mut body): (i64, Vec<u8>) = raw
        .query_row("SELECT pack_id,body FROM pack", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    let needle = b"authentic retained payload";
    let at = body
        .windows(needle.len())
        .position(|w| w == needle)
        .unwrap();
    body[at] ^= 1;
    let digest = layerfs_storage::port::ObjectKey::for_bytes(&body);
    raw.execute(
        "UPDATE pack SET body=?1,digest=?2 WHERE pack_id=?3",
        rusqlite::params![body, digest.as_bytes().as_slice(), id],
    )
    .unwrap();
    raw.execute_batch(&trigger).unwrap();
    // Already-open readonly handle avoids treating deliberate fixture schema
    // damage as a substitute for the object authentication being exercised.
    let full = observations(&writer, false);
    assert!(
        full.iter()
            .any(|o| o.id == "store.objects" && o.outcome == Outcome::Fail),
        "{full:?}"
    );
    let quick = observations(&writer, true);
    assert!(quick
        .iter()
        .any(|o| o.id == "store.objects" && o.outcome == Outcome::Skipped));
}

#[test]
fn already_expired_deadline_returns_without_a_clean_report() {
    let temp = support::Temp::new("doctor-timeout");
    let path = temp.join("store.sqlite");
    let writer =
        SqliteHandles::create(config(&path), StoragePolicy::frozen_default(), &authority())
            .unwrap();
    let mut observations = Vec::new();
    assert!(writer
        .diagnose(
            Options {
                quick: false,
                deadline: Some(Instant::now()),
                cancelled: None
            },
            &mut |o| observations.push(o)
        )
        .is_err());
    assert!(!observations.iter().any(|o| o.id == "store.objects.summary"));
}

#[test]
fn concurrent_publication_is_excluded_from_the_pinned_inventory() {
    let temp = support::Temp::new("doctor-concurrent");
    let path = temp.join("store.sqlite");
    let authority = authority();
    let writer =
        SqliteHandles::create(config(&path), StoragePolicy::frozen_default(), &authority).unwrap();
    put(&writer);
    let reader =
        SqliteHandles::open_diagnostic(&path, &authority.binding_key, authority.cursor_key)
            .unwrap();
    let mut result = Vec::new();
    reader
        .diagnose(Options::default(), &mut |o| {
            if o.id == "store.snapshot" {
                let storage = Storage::new(writer.storage.clone()).unwrap();
                let save = storage.begin_save().unwrap();
                save.accept(
                    FinalizedObject::new(
                        ObjectRole::WholeFile,
                        encode_whole_file_payload(b"concurrently published distinct data").unwrap(),
                    )
                    .unwrap(),
                )
                .unwrap();
                save.finish().unwrap();
            }
            result.push(o);
        })
        .unwrap();
    assert!(
        result
            .iter()
            .any(|o| o.id == "store.objects.summary" && o.message.starts_with("1 winning")),
        "{result:?}"
    );
    let count: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row("SELECT count(*) FROM object_location", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2);
}

#[test]
fn independent_history_lineage_checks_accept_real_forks_and_detect_damage() {
    use layerfs_history::*;
    let temp = support::Temp::new("doctor-lineage");
    let path = temp.join("store.sqlite");
    let writer =
        SqliteHandles::create(config(&path), StoragePolicy::frozen_default(), &authority())
            .unwrap();
    let h = &writer.history;
    let stack = h
        .initialize_layerstack(&StackInitialization {
            logical: 0,
            stack: support::stack(1),
            name: support::name("test"),
            scope: support::root(200),
            profile: support::root(201),
            genesis_root: support::root(0),
        })
        .unwrap();
    let a = support::branch_id(1);
    let b = support::branch_id(2);
    for (branch, label) in [(a, "a"), (b, "b")] {
        h.fork(&ForkRequest {
            stack: stack.id,
            branch,
            name: support::name(label),
            source: ForkSource::Layer(stack.head_layer),
        })
        .unwrap();
    }
    let mut head = stack.head_layer;
    for (branch, n) in [(a, 1), (b, 2), (a, 3), (b, 4)] {
        let snapshot = h.branch_snapshot(branch).unwrap().unwrap();
        let outcome = h
            .commit_branch(&CommitRequest {
                branch,
                expected_head: snapshot.branch.head_commit,
                expected_base: snapshot.branch.base_layer,
                expected_root: snapshot.effective_root,
                candidate_root: support::root(n),
                profile: snapshot.profile,
                scope: snapshot.scope,
                second_parent: None,
                meta: CommitMeta::new(CommitVia::Files),
                force: false,
            })
            .unwrap();
        let CommitOutcome::Committed { record, .. } = outcome else {
            panic!("expected Commit");
        };
        let result = h
            .add_layer(&AddLayerRequest {
                stack: stack.id,
                branch,
                commit: record.id,
                expected_stack_head: head,
                expected_branch_base: record.base_layer,
            })
            .unwrap();
        if let AddLayerOutcome::Added { layer } = result {
            head = layer.id;
        }
    }
    let before = observations(&writer, true);
    assert!(
        !before.iter().any(|o| o.outcome == Outcome::Fail),
        "{before:?}"
    );
    assert!(
        before
            .iter()
            .any(|o| o.id == "store.history.lineage.summary" && o.outcome == Outcome::Pass),
        "{before:?}"
    );
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute("UPDATE commit_lineage SET newest=999", [])
        .unwrap();
    let after = observations(&writer, true);
    assert!(
        after
            .iter()
            .any(|o| o.id == "store.history.lineage" && o.outcome == Outcome::Fail),
        "{after:?}"
    );
}

#[test]
fn identical_root_is_qualified_under_every_history_context() {
    use layerfs_content::{
        filesystem::{
            attributes::PortableMetadata,
            directory::{encode_directory_page, DirectoryPage},
            inode::{encode_inode_page, InodePage},
            scope_for_seed, FilesystemRoot,
        },
        inode_leaf::{InodeKind, InodeMetadata, InodeValue},
    };
    use layerfs_history::{HistoryCatalog, StackInitialization};
    let temp = support::Temp::new("doctor-root-context");
    let path = temp.join("store.sqlite");
    let writer =
        SqliteHandles::create(config(&path), StoragePolicy::frozen_default(), &authority())
            .unwrap();
    let storage = Storage::new(writer.storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    let directory = FinalizedObject::new(
        ObjectRole::DirectoryLeaf,
        encode_directory_page(&DirectoryPage::Leaf { entries: vec![] }).unwrap(),
    )
    .unwrap();
    let directory_id = directory.id();
    save.accept(directory).unwrap();
    let inode = FinalizedObject::new(
        ObjectRole::InodeLeaf,
        encode_inode_page(&InodePage::Leaf {
            entries: vec![(
                1,
                InodeValue {
                    kind: InodeKind::Directory,
                    namespace_ref_count: 0,
                    content_root: directory_id,
                    metadata_root: InodeMetadata::Inline(PortableMetadata {
                        mode: 0o755,
                        mtime_seconds: 0,
                        mtime_nanoseconds: 0,
                    }),
                },
            )],
        })
        .unwrap(),
    )
    .unwrap()
    .with_references(vec![directory_id]);
    let inode_id = inode.id();
    save.accept(inode).unwrap();
    let scope = scope_for_seed([5; 32]);
    let root = FilesystemRoot::new(
        layerfs_content::filesystem::root::profile_id(),
        scope,
        1,
        inode_id,
    )
    .unwrap();
    let object = FinalizedObject::new(ObjectRole::FilesystemRoot, root.encode().unwrap())
        .unwrap()
        .with_references(vec![inode_id]);
    let root_id = object.id();
    save.accept(object).unwrap();
    save.finish().unwrap();
    writer
        .history
        .initialize_layerstack(&StackInitialization {
            logical: 0,
            stack: support::stack(1),
            name: support::name("good"),
            scope: scope.object(),
            profile: root.profile(),
            genesis_root: root_id,
        })
        .unwrap();
    let good = observations(&writer, false);
    assert!(!good.iter().any(|o| o.outcome == Outcome::Fail), "{good:?}");
    assert!(
        good.iter()
            .any(|o| o.id == "store.namespace.summary" && o.message.starts_with("1 historical")),
        "{good:?}"
    );
    writer
        .history
        .initialize_layerstack(&StackInitialization {
            logical: 0,
            stack: support::stack(2),
            name: support::name("wrong"),
            scope: support::root(201),
            profile: root.profile(),
            genesis_root: root_id,
        })
        .unwrap();
    let bad = observations(&writer, false);
    assert!(
        bad.iter()
            .any(|o| o.id == "store.namespace" && o.outcome == Outcome::Fail),
        "{bad:?}"
    );
    assert!(
        bad.iter()
            .any(|o| o.id == "store.namespace.summary" && o.message.starts_with("2 historical")),
        "{bad:?}"
    );
}

#[test]
fn losing_duplicate_physical_representation_and_reserved_gaps_are_valid() {
    use layerfs_storage::port::{PackPersistence, Publication, PublishedPack, Reserve};
    let temp = support::Temp::new("doctor-retained");
    let path = temp.join("store.sqlite");
    let writer =
        SqliteHandles::create(config(&path), StoragePolicy::frozen_default(), &authority())
            .unwrap();
    put(&writer);
    let object = FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(b"authentic retained payload").unwrap(),
    )
    .unwrap();
    let mut locations = Vec::new();
    writer
        .storage
        .locate(&[object.id()], &mut locations)
        .unwrap();
    let mut location = locations[0].location;
    let mut packs = Vec::new();
    writer
        .storage
        .read_packs(&[location.pack_id], &mut packs)
        .unwrap();
    let pack = packs.pop().unwrap();
    let reserved = writer
        .storage
        .reserve(Reserve {
            packs: 100,
            ordinals: 10,
        })
        .unwrap();
    location.pack_id = reserved.first_pack_id + 99;
    let publication = Publication {
        packs: vec![PublishedPack::seal(
            location.pack_id,
            pack.info().domain,
            std::sync::Arc::new(pack.body().to_vec()),
        )],
        objects: vec![location],
        ..Publication::default()
    };
    assert_eq!(
        writer.storage.publish(&publication).unwrap().lost,
        vec![object.id()]
    );
    let result = observations(&writer, false);
    assert!(
        !result.iter().any(|o| o.outcome == Outcome::Fail),
        "{result:?}"
    );
    assert!(
        result
            .iter()
            .any(|o| o.id == "store.packs.summary" && o.message.starts_with("2 packs")),
        "{result:?}"
    );
}

#[test]
fn unused_ordinal_and_advisory_blocks_are_not_hidden_by_reachability() {
    let temp = support::Temp::new("doctor-unused-blocks");
    let path = temp.join("store.sqlite");
    let writer =
        SqliteHandles::create(config(&path), StoragePolicy::frozen_default(), &authority())
            .unwrap();
    let raw = rusqlite::Connection::open(&path).unwrap();
    // Malformed fixture only: bypass source checks to exercise bounded reads
    // of a corrupt oversized row before its bytes enter the diagnostic process.
    raw.execute_batch("PRAGMA ignore_check_constraints=ON")
        .unwrap();
    raw.execute(
        "INSERT INTO inode_value_block(first_ordinal,last_ordinal,body) VALUES(1,1,?1)",
        [vec![0u8; 4001]],
    )
    .unwrap();
    raw.execute(
        "INSERT INTO binding_hint_block(grp,seq,body) VALUES(0,1,?1)",
        [vec![0u8; 9]],
    )
    .unwrap();
    let result = observations(&writer, false);
    assert!(
        result
            .iter()
            .any(|o| o.id == "store.ordinal_blocks" && o.outcome == Outcome::Fail),
        "{result:?}"
    );
    assert!(
        result
            .iter()
            .any(|o| o.id == "store.advisory.binding_hints" && o.outcome == Outcome::Fail),
        "{result:?}"
    );
}

#[test]
fn oversized_monolithic_pack_body_is_rejected_before_body_acquisition() {
    let temp = support::Temp::new("doctor-oversized-pack");
    let path = temp.join("store.sqlite");
    let writer = SqliteHandles::create(
        config(&path).with_sqlite_pack_layout(SqlitePackLayout::Monolithic),
        StoragePolicy::frozen_default(),
        &authority(),
    )
    .unwrap();
    put(&writer);
    let raw = rusqlite::Connection::open(&path).unwrap();
    let trigger: String = raw
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE type='trigger' AND name='pack_immutable_update'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    raw.execute_batch("PRAGMA ignore_check_constraints=ON; DROP TRIGGER pack_immutable_update; UPDATE pack SET body=CAST(body||zeroblob(1048576) AS BLOB)").unwrap();
    raw.execute_batch(&trigger).unwrap();
    let result = observations(&writer, false);
    assert!(
        result
            .iter()
            .any(|o| o.id == "store.packs" && o.outcome == Outcome::Fail),
        "{result:?}"
    );
}
