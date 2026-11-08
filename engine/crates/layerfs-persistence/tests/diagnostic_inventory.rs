//! B1 actual acquisition/auxiliary rows, including valid interrupted state.
mod support;
use layerfs_content::{filesystem::attributes::PortableMetadata, inode_leaf::InodeKind, ObjectId};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{
    diagnostic::{Observation, Options, Outcome},
    PersistenceConfig, SqliteAcquisitionSchema, SqliteHandles, SqlitePersistenceProfile,
};
use layerfs_storage::{
    port::acquisition::{
        Acquisition, Begin, EntryKey, HostIdentity, NewEntry, Owner, Phase, Placed,
    },
    StoragePolicy,
};
struct Fixture {
    temp: support::Temp,
    handles: SqliteHandles,
}
impl Fixture {
    fn new() -> Self {
        let temp = support::Temp::new("diagnostic-inventory");
        let handles = SqliteHandles::create(
            PersistenceConfig::sqlite(temp.join("store.sqlite"))
                .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
                .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
            StoragePolicy::frozen_default(),
            &HistoryCatalogConfig {
                binding_key: b"inventory-doctor".to_vec(),
                cursor_key: [9; 32],
                incarnation: 1,
            },
        )
        .unwrap();
        Self { temp, handles }
    }
    fn raw(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(self.temp.join("store.sqlite")).unwrap()
    }
    fn audit(&self, quick: bool) -> Vec<Observation> {
        let before = self.handles.diagnostics().unwrap();
        let mut out = Vec::new();
        self.handles
            .diagnose(
                Options {
                    quick,
                    ..Options::default()
                },
                &mut |o| out.push(o),
            )
            .unwrap();
        let after = self.handles.diagnostics().unwrap();
        assert_eq!(before.write_transactions, after.write_transactions);
        out
    }
    fn begin(&self) -> Owner {
        self.handles
            .acquisition
            .begin(&Begin {
                source_device: 1,
                source_inode: 2,
                stack: [3; 16],
                scope: ObjectId::for_bytes(b"unpublished scope"),
            })
            .unwrap()
    }
}
fn metadata(kind: InodeKind) -> PortableMetadata {
    PortableMetadata {
        mode: if kind == InodeKind::Directory {
            0o755
        } else if kind == InodeKind::Symlink {
            0o777
        } else {
            0o644
        },
        mtime_seconds: -1,
        mtime_nanoseconds: 123,
    }
}
fn entry(
    parent: Option<u64>,
    name: &[u8],
    position: Option<u64>,
    kind: InodeKind,
    host: Option<HostIdentity>,
) -> NewEntry {
    NewEntry {
        key: EntryKey {
            parent,
            name: name.to_vec(),
        },
        position,
        kind,
        metadata: metadata(kind),
        target_root: (kind == InodeKind::Symlink)
            .then(|| ObjectId::for_bytes(b"accepted but not published symlink")),
        host_path: (kind != InodeKind::Symlink).then(|| b"/source/path".to_vec()),
        host,
    }
}
fn populated(f: &Fixture) -> Owner {
    let owner = f.begin();
    let host = HostIdentity {
        device: 1,
        inode: 42,
        evidence: [7; 44],
    };
    f.handles
        .acquisition
        .put_entries(
            owner,
            &[
                entry(None, b"", Some(0), InodeKind::Directory, None),
                entry(
                    Some(0),
                    b"first",
                    Some(1),
                    InodeKind::RegularFile,
                    Some(host),
                ),
                entry(
                    Some(0),
                    b"alias",
                    Some(2),
                    InodeKind::RegularFile,
                    Some(host),
                ),
                entry(
                    Some(0),
                    b"pending",
                    None,
                    InodeKind::RegularFile,
                    Some(HostIdentity { inode: 44, ..host }),
                ),
                entry(Some(0), b"dangling", Some(3), InodeKind::Symlink, None),
            ],
        )
        .unwrap();
    f.handles
        .acquisition
        .complete_files(
            owner,
            &[(
                1,
                ObjectId::for_bytes(b"constructed but not published file"),
            )],
        )
        .unwrap();
    owner
}
fn clean(out: &[Observation]) {
    assert!(
        !out.iter()
            .any(|o| o.outcome == Outcome::Fail || o.outcome == Outcome::Unknown),
        "{out:?}"
    );
}
fn failed(out: &[Observation], id: &str) {
    assert!(
        out.iter().any(|o| o.id == id && o.outcome == Outcome::Fail),
        "missing{id}: {out:?}"
    );
}
#[test]
fn active_unpublished_and_interrupted_cleanup_are_valid() {
    let f = Fixture::new();
    clean(&f.audit(false));
    let owner = populated(&f);
    clean(&f.audit(false));
    clean(&f.audit(true));
    f.handles.acquisition.advance(owner, Phase::Failed).unwrap();
    f.handles.acquisition.discard(owner, 1).unwrap();
    clean(&f.audit(false));
}
#[test]
fn acquisition_rows_are_paged_without_finding_progress_retention() {
    let f = Fixture::new();
    let owner = f.begin();
    f.handles
        .acquisition
        .put_entries(
            owner,
            &[entry(None, b"", Some(0), InodeKind::Directory, None)],
        )
        .unwrap();
    for i in 0..140 {
        f.handles
            .acquisition
            .put_entries(
                owner,
                &[entry(
                    Some(0),
                    format!("f{i:03}").as_bytes(),
                    Some(i + 1),
                    InodeKind::Symlink,
                    None,
                )],
            )
            .unwrap();
    }
    let out = f.audit(true);
    clean(&out);
    assert_eq!(
        out.iter()
            .filter(|o| o.id == "store.acquisition.progress")
            .count(),
        3
    );
    assert!(
        out.iter()
            .any(|o| o.id == "store.acquisition.metadata.summary"
                && o.message.starts_with("141 Init"))
    );
}
#[test]
fn acquisition_authoritative_and_metadata_damage_are_attributable() {
    for(sql,id)in[
  ("UPDATE init_operation SET held_rows=held_rows+1,peak_rows=peak_rows+1","store.acquisition.charges"),
  ("UPDATE init_entry SET canonical_position=99 WHERE name=X'6669727374'","store.acquisition.canonical"),
  ("UPDATE init_host_file SET aliases=aliases+1","store.acquisition.aliases"),
  ("UPDATE init_entry SET parent_position=99 WHERE name=X'616c696173'","store.acquisition.parent"),
  ("UPDATE init_entry SET metadata_root=CAST(substr(metadata_root,1,31)||X'01' AS BLOB) WHERE name=X'6669727374'","store.acquisition.metadata"),
  ("PRAGMA foreign_keys=OFF; DELETE FROM init_operation","store.acquisition.owners"),
 ]{let f=Fixture::new();populated(&f);clean(&f.audit(true));f.raw().execute_batch(sql).unwrap();failed(&f.audit(true),id);}
}
#[test]
fn allocator_and_accounting_damage_are_not_hidden_by_empty_heads() {
    for(sql,id)in[
  ("UPDATE store_totals SET canonical=1","store.catalog.canonical_total"),
  ("UPDATE store_totals SET stored=1","store.catalog.stored_total"),
  ("UPDATE store_totals SET logical=1","store.catalog.logical_total"),
  ("INSERT INTO scope_allocator(scope_id,highwater,authority_id) VALUES(zeroblob(32),100,zeroblob(32))","store.catalog.scope"),
 ]{let f=Fixture::new();clean(&f.audit(true));f.raw().execute_batch(sql).unwrap();failed(&f.audit(true),id);}
}
#[test]
fn opaque_retained_merge_records_get_structural_owner_checks() {
    let f = Fixture::new();
    f.raw().execute_batch("INSERT INTO merge_operation(operation,token,touched) VALUES(1,zeroblob(16),0); INSERT INTO merge_record(operation,kind,key,value) VALUES(1,999,zeroblob(32),X'0102')").unwrap();
    clean(&f.audit(true));
    f.raw()
        .execute_batch("DELETE FROM merge_operation")
        .unwrap();
    failed(&f.audit(true), "store.merge.references");
}

#[test]
fn placing_pending_identity_and_every_cleanup_phase_preserve_valid_charges() {
    for phase in [Phase::Discarding, Phase::Failed, Phase::Uncertain] {
        let f = Fixture::new();
        let owner = populated(&f);
        f.handles
            .acquisition
            .place_children(
                owner,
                0,
                &[Placed {
                    name: b"pending".to_vec(),
                    position: 4,
                    host: Some((
                        HostIdentity {
                            device: 1,
                            inode: 44,
                            evidence: [7; 44],
                        },
                        b"/source/path".to_vec(),
                    )),
                }],
            )
            .unwrap();
        clean(&f.audit(true));
        f.handles.acquisition.advance(owner, phase).unwrap();
        f.handles.acquisition.discard(owner, 1).unwrap();
        clean(&f.audit(true));
    }
}
#[test]
fn deleted_init_owners_and_unused_allocation_gaps_remain_valid() {
    let f = Fixture::new();
    for _ in 0..3 {
        let owner = f.begin();
        f.handles.acquisition.release(owner).unwrap();
    }
    assert_eq!(
        f.raw()
            .query_row(
                "SELECT seq FROM sqlite_sequence WHERE name='init_operation'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        3
    );
    clean(&f.audit(true));
}
