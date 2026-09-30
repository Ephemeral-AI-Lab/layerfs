//! Real StoreProvider validation request/count and requested-System observations.
//!
//! Fixture setup and external trace bookkeeping are outside capture. The declared
//! capture is the complete check_with_claims call: memo, real canonical decoding,
//! borrowed input cursors and scratch-port Rust owners are included. It does not
//! measure SQLite/zstd native heap, OS cache, RSS, global admission or speed.

#[path = "support/allocation_observer.rs"]
mod allocation_observer;
#[path = "support/prefetch_counts.rs"]
mod counts;
#[path = "support/prefetch_namespace.rs"]
mod namespace;
mod support;

use std::cell::RefCell;
use std::collections::BTreeMap;

use layerfs_content::filesystem::rows::SliceBindingRows;
use layerfs_content::filesystem::state::ConstructionScopes;
use layerfs_content::filesystem::validate::{check_bindings, check_with_claims, ValidationWork};
use layerfs_content::filesystem::{
    update_filesystem_binding_rows_with_construction_state, DirectoryUpdate, FilesystemInput,
    FilesystemObjects, FilesystemPhases, FilesystemRead, FilesystemResources, FilesystemRootId,
    InodeScope, InodeUpdate, PathName,
};
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedObject, ObjectId,
};
use layerfs_storage::construction_state::{ScratchAuthority, ScratchDisposition};
use layerfs_storage::{Store, StoreProvider};
use layerfs_telemetry::timer::TimingScope;

use allocation_observer::{Observation, Pause};
use namespace::{Fixture, DIRECTORIES, NEW_FILE, PAYLOAD};
use support::{create_store, disabled, Collected, TempDir};

#[global_allocator]
static ALLOCATOR: allocation_observer::ObservedSystem = allocation_observer::ObservedSystem;

struct ObservedProvider<'a> {
    inner: StoreProvider<'a>,
    fixture: &'a Fixture,
    requests: RefCell<Vec<Vec<ObjectId>>>,
}

impl<'a> ObservedProvider<'a> {
    fn new(store: &'a Store, fixture: &'a Fixture) -> Self {
        Self {
            inner: StoreProvider::new(store),
            fixture,
            requests: RefCell::new(Vec::with_capacity(2048)),
        }
    }

    fn observe(
        &self,
        ids: &[ObjectId],
        result: ContentResult<Vec<Vec<u8>>>,
    ) -> ContentResult<Vec<Vec<u8>>> {
        let _pause = Pause::new();
        self.requests.borrow_mut().push(ids.to_vec());
        if ids.iter().all(|id| self.fixture.is_inode(*id)) {
            assert!(ids.len() <= 64, "actual immutable inode page request bound");
        }
        if let Ok(values) = &result {
            assert_eq!(values.len(), ids.len());
            for (id, bytes) in ids.iter().zip(values) {
                assert_eq!(bytes, self.fixture.canonical(*id));
            }
        }
        result
    }
}

impl AuthenticatedObjects for ObservedProvider<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.observe(ids, self.inner.read_canonical_batch(ids))
    }
    fn read_canonical_batch_scoped(
        &self,
        ids: &[ObjectId],
        scope: TimingScope<'_>,
    ) -> ContentResult<Vec<Vec<u8>>> {
        self.observe(ids, self.inner.read_canonical_batch_scoped(ids, scope))
    }
}

fn save_fixture(store: &Store, fixture: &Fixture) {
    disabled(|scope| {
        let mut save = store.begin_save(scope.child("storage.begin"))?;
        for id in &fixture.order {
            let object = fixture.objects.get(id).unwrap();
            let finalized = FinalizedObject::new(object.role, object.canonical.clone())
                .unwrap()
                .with_references(object.references.clone());
            assert_eq!(finalized.id(), *id, "independent object identity");
            save.accept(finalized)?;
        }
        save.finish(scope.child("storage.finish"))
    })
    .unwrap();
}

fn updates(fixture: &Fixture) -> Vec<DirectoryUpdate> {
    fixture
        .directory_names
        .range(2..)
        .map(|(parent, entries)| DirectoryUpdate {
            parent: *parent,
            changes: entries
                .iter()
                .map(|(name, serial)| (PathName::from_bytes(name).unwrap(), Some(*serial)))
                .collect(),
        })
        .collect()
}

fn demand_rows(updates: &[DirectoryUpdate]) -> Vec<(u64, Vec<u64>)> {
    updates
        .iter()
        .map(|row| {
            (
                row.parent,
                row.changes
                    .iter()
                    .filter_map(|(_, serial)| *serial)
                    .collect(),
            )
        })
        .collect()
}

fn assert_work(work: ValidationWork, expected: &counts::Model) {
    let got = work.prefetch;
    let want = expected.prefetch;
    assert_eq!(
        (
            got.examined_occurrences,
            got.memo_hits,
            got.local_duplicates,
            got.submitted_serials,
            got.lookup_calls,
            got.max_pending,
            got.max_missing,
            got.max_answers
        ),
        (
            want.examined_occurrences,
            want.memo_hits,
            want.local_duplicates,
            want.submitted_serials,
            want.lookup_calls,
            want.max_pending,
            want.max_missing,
            want.max_answers
        )
    );
    assert_eq!(
        got.examined_occurrences,
        got.memo_hits + got.local_duplicates + got.submitted_serials
    );
    assert!(got.max_pending <= 64 && got.max_missing <= 64 && got.max_answers <= 64);
    let sites = work.inode_pages_by_site;
    assert_eq!(
        (
            sites.allocation,
            sites.prefetch,
            sites.bindings,
            sites.cycles
        ),
        (
            expected.allocation_pages,
            expected.prefetch_pages,
            expected.binding_pages,
            expected.cycle_pages
        )
    );
    assert_eq!(sites.aliases + sites.reachability, 0);
    assert_eq!(
        work.inode_pages_read,
        sites.allocation
            + sites.prefetch
            + sites.bindings
            + sites.aliases
            + sites.cycles
            + sites.reachability
    );
    assert_eq!(work.read_waves, expected.read_waves);
    assert_eq!(work.inode_demands, expected.logical_demands);
}

#[test]
fn real_store_wide_duplicate_absence_and_memo_pressure_keep_exact_requests_and_roots() {
    for (aliases, absent, ordering_bytes) in [
        (1, false, 1_048_576),
        (8, false, 1_048_576),
        (1, false, 1040),
        (8, true, 1_048_576),
    ] {
        let temp = TempDir::new("prefetch_real_store");
        let base = Fixture::new(aliases, false, false);
        let final_expected = Fixture::new(aliases, absent, false);
        let store = create_store(&temp.store_path("base"));
        save_fixture(&store, &base);
        let rows = updates(&final_expected);
        let new: Vec<_> = if absent {
            (NEW_FILE..NEW_FILE + 8).collect()
        } else {
            Vec::new()
        };
        let inodes: Vec<_> = new
            .iter()
            .map(|serial| {
                let mut value = final_expected.values[serial];
                value.namespace_ref_count = 0;
                InodeUpdate {
                    serial: *serial,
                    value,
                }
            })
            .collect();
        let resources = FilesystemResources {
            ordering_bytes,
            ..FilesystemResources::default()
        };
        let input = FilesystemInput {
            base: Some(FilesystemRootId(base.root)),
            scope: InodeScope::from_object(base.scope),
            root_serial: 1,
            directories: &rows,
            inodes: &inodes,
            new_inodes: &new,
            resources,
        };
        let source = SliceBindingRows::new(&input).unwrap();
        let expected = counts::Model::new(
            &base,
            &demand_rows(&rows),
            &new,
            (ordering_bytes / 1024) as usize,
        );
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let mut state = authority
            .begin_phased(
                [0x81; 32],
                DIRECTORIES,
                rows.iter().map(|row| row.changes.len() as u64).sum(),
            )
            .unwrap();
        let selected = ConstructionScopes::new(state.selection().clone()).unwrap();
        let provider = ObservedProvider::new(&store, &base);
        let observation = Observation::start(
            [
                base.canonical(base.leaves[0]).len(),
                base.canonical(base.leaves[1]).len(),
            ],
            [0, 0],
        );
        let mut work = ValidationWork::default();
        {
            let mut port = state.adapter();
            let checked = check_with_claims(
                &provider,
                &source,
                &BTreeMap::new(),
                &mut work,
                &mut port,
                selected.claims(),
            )
            .unwrap();
            assert_eq!(checked.topology.table.unwrap().root, base.inode_root);
        }
        let snapshot = observation.stop();
        assert!(!snapshot.overflow, "real allocation ledger incomplete");
        assert_work(work, &expected);
        assert_eq!(
            *provider.requests.borrow(),
            expected.batches,
            "actual grouped demand order"
        );
        assert!(provider
            .requests
            .borrow()
            .iter()
            .all(|batch| batch.len() <= 64));
        assert_eq!(provider.inner.connection_opens(), 1);
        let physical = (
            provider.inner.group_decodes(),
            provider.inner.pooled_read_counters(),
        );
        eprintln!("prefetch64 count/System diagnostic aliases={aliases} absent={absent} memo_limit={}: prefetch={:?}, sites={:?}, Store ordinary_decodes={}, pooled={:?}, Rust_requested_snapshot={snapshot:?}; fixtures/traces excluded, live memo/read/decode/status owners included; no native/global/RSS/speed claim", ordering_bytes / 1024, work.prefetch, work.inode_pages_by_site, physical.0, physical.1);
        state.release().unwrap();
        drop(provider);
        let released = observation.released();
        assert_eq!(
            released.live_total(),
            0,
            "captured provider/status owners actually dropped"
        );

        // Legacy public result is a separate explicit route with a fresh real
        // reader. It supplies no expected canonical pins for the new route.
        let legacy_provider = StoreProvider::new(&store);
        let checked = check_bindings(
            &legacy_provider,
            &source,
            &BTreeMap::new(),
            &mut ValidationWork::default(),
        )
        .unwrap();
        let expected_map: BTreeMap<_, _> = rows
            .iter()
            .flat_map(|row| {
                row.changes
                    .iter()
                    .filter_map(|(_, serial)| serial.map(|serial| (serial, 0)))
            })
            .collect();
        assert_eq!(checked.additions, expected_map);

        // A complete canonical update has its own fresh selected owner. The
        // expected inherited65/right-leaf partition is independently encoded.
        let mut native = authority
            .begin_phased(
                [0x82; 32],
                DIRECTORIES,
                rows.iter().map(|row| row.changes.len() as u64).sum(),
            )
            .unwrap();
        let selected = ConstructionScopes::new(native.selection().clone()).unwrap();
        let reader = StoreProvider::new(&store);
        let mut output = Collected::new();
        let result = {
            let mut objects = FilesystemObjects::new(&reader, &mut output);
            update_filesystem_binding_rows_with_construction_state(
                &mut objects,
                &source,
                None,
                &mut native.adapter(),
                &selected,
                &FilesystemPhases::disabled(),
            )
            .unwrap()
        };
        assert_eq!(result.root.0, final_expected.root, "independent v1 root");
        for (id, role, bytes, _) in output.objects() {
            assert_eq!(bytes, final_expected.canonical(*id));
            assert_eq!(*role, final_expected.objects[id].role);
        }
        native.release().unwrap();
        let mut save = disabled(|scope| store.begin_save(scope.child("save"))).unwrap();
        for object in output.finalized() {
            save.accept(object).unwrap();
        }
        disabled(|scope| save.finish(scope.child("finish"))).unwrap();
        let reader = StoreProvider::new(&store);
        let mut filesystem = FilesystemRead::new(&reader, result.root).unwrap();
        for row in &rows {
            for (name, serial) in &row.changes {
                let resolved = filesystem.resolve_child(row.parent, name).unwrap();
                assert_eq!(resolved.serial, serial.unwrap());
                assert_eq!(resolved.value, final_expected.values[&resolved.serial]);
            }
        }
        let mut bytes = Vec::new();
        disabled(|scope| {
            layerfs_content::read_all(&reader, base.file_root, &mut bytes, scope.child("read"))
        })
        .unwrap();
        assert_eq!(bytes, PAYLOAD);
        let scoped = ObservedProvider::new(&store, &base);
        let values = disabled(|scope| {
            scoped.read_canonical_batch_scoped(
                &[base.inode_root, base.leaves[0]],
                scope.child("provider"),
            )
        })
        .unwrap();
        assert_eq!(values[0], base.canonical(base.inode_root));
        assert_eq!(scoped.inner.connection_opens(), 1);
    }
}

#[test]
fn late_declared_name_refusal_precedes_all_initial_prefetch_reads() {
    let temp = TempDir::new("prefetch_late_count");
    let fixture = Fixture::new(8, false, false);
    let store = create_store(&temp.store_path("base"));
    save_fixture(&store, &fixture);
    let rows = updates(&fixture);
    let input = FilesystemInput {
        base: Some(FilesystemRootId(fixture.root)),
        scope: InodeScope::from_object(fixture.scope),
        root_serial: 1,
        directories: &rows,
        inodes: &[],
        new_inodes: &[],
        resources: FilesystemResources {
            ordering_bytes: 1040,
            ..FilesystemResources::default()
        },
    };
    let source = SliceBindingRows::new(&input).unwrap();
    let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
    let mut native = authority
        .begin_phased([0x83; 32], DIRECTORIES, 520)
        .unwrap();
    let selected = ConstructionScopes::new(native.selection().clone()).unwrap();
    let provider = ObservedProvider::new(&store, &fixture);
    let mut work = ValidationWork::default();
    assert!(matches!(
        check_with_claims(
            &provider,
            &source,
            &BTreeMap::new(),
            &mut work,
            &mut native.adapter(),
            selected.claims()
        ),
        Err(ContentError::ObjectLimitExceeded { limit: 65, .. })
    ));
    assert_eq!(*provider.requests.borrow(), vec![vec![fixture.root]]);
    assert_eq!(work.prefetch.lookup_calls, 0);
    assert_eq!(work.inode_pages_by_site.prefetch, 0);
    assert_eq!(
        authority.status().unwrap()[0].disposition,
        ScratchDisposition::Failed
    );
    assert!(native.capacity(selected.roots()).is_err());
    native.release().unwrap();
}

#[test]
fn real_corrupt_value_group_preserves_partial_prefetch_pages_and_original_provider_failure() {
    let temp = TempDir::new("prefetch_actual_corrupt_group");
    let fixture = Fixture::new(1, false, false);
    let path = temp.store_path("base");
    let store = create_store(&path);
    save_fixture(&store, &fixture);
    drop(store);
    // Offline corruption touches only this owned fixture's exact catalogue
    // digest. Parent ordinary pages remain authentic; a fresh real provider
    // must authenticate the pooled value body against this selected row.
    let connection = rusqlite::Connection::open(&path).unwrap();
    let (ordinal, original): (i64, Vec<u8>) = connection
        .query_row(
            "SELECT first_ordinal,digest FROM metadata_value_groups ORDER BY first_ordinal LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(original.len(), 32);
    let mut corrupt = original.clone();
    corrupt[0] ^= 1;
    assert_eq!(
        connection
            .execute(
                "UPDATE metadata_value_groups SET digest=?1 WHERE first_ordinal=?2 AND digest=?3",
                rusqlite::params![corrupt, ordinal, original]
            )
            .unwrap(),
        1
    );
    drop(connection);
    let store = support::open_store(&path);
    let rows = updates(&fixture);
    let input = FilesystemInput {
        base: Some(FilesystemRootId(fixture.root)),
        scope: InodeScope::from_object(fixture.scope),
        root_serial: 1,
        directories: &rows,
        inodes: &[],
        new_inodes: &[],
        resources: FilesystemResources::default(),
    };
    let source = SliceBindingRows::new(&input).unwrap();
    let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
    let mut native = authority.begin_phased([0x84; 32], DIRECTORIES, 65).unwrap();
    let selected = ConstructionScopes::new(native.selection().clone()).unwrap();
    let provider = ObservedProvider::new(&store, &fixture);
    let mut work = ValidationWork::default();
    assert!(matches!(
        check_with_claims(
            &provider,
            &source,
            &BTreeMap::new(),
            &mut work,
            &mut native.adapter(),
            selected.claims()
        ),
        Err(ContentError::ProviderFailure {
            what: "value group identity"
        })
    ));
    assert_eq!(
        *provider.requests.borrow(),
        vec![
            vec![fixture.root],
            vec![fixture.inode_root],
            fixture.leaves.to_vec()
        ]
    );
    assert_eq!(
        (
            work.prefetch.lookup_calls,
            work.prefetch.submitted_serials,
            work.prefetch.max_answers
        ),
        (1, 64, 0)
    );
    assert_eq!(
        (
            work.inode_pages_read,
            work.inode_pages_by_site.prefetch,
            work.read_waves
        ),
        (1, 1, 1)
    );
    assert_eq!(
        authority.status().unwrap()[0].disposition,
        ScratchDisposition::Failed
    );
    assert!(native.capacity(selected.roots()).is_err());
    native.release().unwrap();
}
