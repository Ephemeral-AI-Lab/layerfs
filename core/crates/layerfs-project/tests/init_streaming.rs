//! Public-port proofs for bounded file completions and final-root publication.
#![cfg(unix)]
mod support;

use layerfs_content::{read_all, FilesystemRead, LogicalPath, ObjectId, ObjectRole};
use layerfs_history::{HistoryCatalog, HistoryName, LayerStackId};
use layerfs_project::{init, InitRequest, Initialized, ProjectError};
use layerfs_storage::{
    location::{LocatedObject, SignatureRow},
    port::{
        AcquiredPackRead, PackPersistence, PackReadPlan, PersistedPack, PersistedPackRead,
        PersistenceError, Publication, Published, Reserve, Reserved, ValueGroupQuery, ValueGroups,
    },
    Storage, StorageError, StoragePolicy,
};
use layerfs_telemetry::timer::Timing;
use std::{
    fs,
    os::unix::fs::{symlink, MetadataExt, PermissionsExt},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use support::{
    memory_acquisition::MemoryAcquisition, memory_history::MemoryHistory,
    memory_metadata::MemoryMetadata, Fixture,
};

const SMALL_FILES: usize = 1300;

enum Action {
    Observe,
    Uncertain,
    ReplaceAliasDirectory { current: PathBuf, parked: PathBuf },
    RemoveLaterCanonical { path: PathBuf },
}

#[derive(Default)]
struct Observations {
    payload_publications: usize,
    filesystem_roots: usize,
    acted: bool,
}

/// Observes the actual immutable publication boundary. It neither redirects
/// constructors nor adds product hooks; failures are public provider outcomes.
struct ObservedPersistence {
    inner: MemoryMetadata,
    acquisition: Arc<MemoryAcquisition>,
    action: Action,
    observed: Mutex<Observations>,
}

impl ObservedPersistence {
    fn new(acquisition: Arc<MemoryAcquisition>, action: Action) -> Self {
        Self {
            inner: MemoryMetadata::default(),
            acquisition,
            action,
            observed: Mutex::new(Observations::default()),
        }
    }
}

impl PackPersistence for ObservedPersistence {
    fn policy(&self) -> Result<StoragePolicy, PersistenceError> {
        self.inner.policy()
    }

    fn locate(
        &self,
        ids: &[ObjectId],
        out: &mut Vec<LocatedObject>,
    ) -> Result<(), PersistenceError> {
        self.inner.locate(ids, out)
    }

    fn read_packs(
        &self,
        ids: &[i64],
        out: &mut Vec<PersistedPack>,
    ) -> Result<(), PersistenceError> {
        self.inner.read_packs(ids, out)
    }

    fn read_pack_selection(
        &self,
        id: i64,
        plan: &mut dyn PackReadPlan,
    ) -> Result<PersistedPackRead, PersistenceError> {
        self.inner.read_pack_selection(id, plan)
    }

    fn read_scoped_pack(
        &self,
        id: i64,
        plan: &mut dyn PackReadPlan,
    ) -> Result<AcquiredPackRead, PersistenceError> {
        self.inner.read_scoped_pack(id, plan)
    }

    fn value_groups(&self, query: ValueGroupQuery<'_>) -> Result<ValueGroups, PersistenceError> {
        self.inner.value_groups(query)
    }

    fn signatures(&self, out: &mut Vec<SignatureRow>) -> Result<(), PersistenceError> {
        self.inner.signatures(out)
    }

    fn reserve(&self, request: Reserve) -> Result<Reserved, PersistenceError> {
        self.inner.reserve(request)
    }

    fn publish(&self, batch: &Publication) -> Result<Published, PersistenceError> {
        let roots = batch
            .objects
            .iter()
            .filter(|object| object.role == ObjectRole::FilesystemRoot)
            .count();
        if roots != 0 {
            assert!(
                self.acquisition.operations().is_empty(),
                "a filesystem root reached publication before acquisition release"
            );
        }
        // Portable attribute values use Chunk objects too. WholeFile identifies
        // a regular file here, so this seam cannot run during prerequisite Save.
        let payload = batch
            .objects
            .iter()
            .any(|object| object.role == ObjectRole::WholeFile);
        let act = {
            let mut observed = self.observed.lock().unwrap();
            observed.filesystem_roots += roots;
            observed.payload_publications += usize::from(payload);
            let act = payload && !observed.acted;
            observed.acted |= payload;
            act
        };
        if act {
            if !matches!(self.action, Action::Observe) {
                assert!(
                    !self.acquisition.operations().is_empty(),
                    "the injected file publication must precede acquisition cleanup"
                );
            }
            match &self.action {
                Action::Observe => {}
                Action::Uncertain => return Err(PersistenceError::Uncertain),
                Action::ReplaceAliasDirectory { current, parked } => {
                    // Move the directory, preserving the original regular inode
                    // and its link count, then give the old alias spelling a
                    // distinct inode. The later-path recheck must reject it.
                    fs::rename(current, parked).unwrap();
                    fs::create_dir(current).unwrap();
                    fs::write(current.join("alias"), b"replacement inode").unwrap();
                }
                Action::RemoveLaterCanonical { path } => fs::remove_file(path).unwrap(),
            }
        }
        self.inner.publish(batch)
    }
}

fn stack() -> LayerStackId {
    LayerStackId::from_authority([19; 16])
}

/// Enough distinct incompressible whole-file bytes to fill ordinary pack/wave
/// bounds before final Save finish. Tiny eight-byte bodies defer this lane's
/// first physical publication until finish and cannot exercise live workers.
fn small_body(index: usize) -> Vec<u8> {
    let mut state = (index as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    (0..4096)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 19) as u8
        })
        .collect()
}

fn large_first_fixture() -> (Fixture, Vec<u8>) {
    let fixture = Fixture::new(0);
    let large: Vec<_> = (0..2 * 1024 * 1024 + 137)
        .map(|n| ((n * 31 + n / 4096) % 251) as u8)
        .collect();
    let first = fixture.source.join("a-first");
    fs::write(&first, &large).unwrap();
    fs::set_permissions(&first, fs::Permissions::from_mode(0o640)).unwrap();
    for n in (0..SMALL_FILES).rev() {
        fs::write(fixture.source.join(format!("b{n:05}")), small_body(n)).unwrap();
    }
    fs::create_dir(fixture.source.join("empty")).unwrap();
    fs::create_dir(fixture.source.join("z-late")).unwrap();
    fs::hard_link(&first, fixture.source.join("z-late/alias")).unwrap();
    // This host link does not contribute to the acquired namespace refcount.
    fs::hard_link(&first, fixture.path.join("outside-alias")).unwrap();
    symlink("../absent", fixture.source.join("link")).unwrap();
    (fixture, large)
}

fn attempt(
    fixture: &Fixture,
    store: &Storage,
    history: &MemoryHistory,
    acquisition: &MemoryAcquisition,
) -> Result<Initialized, ProjectError> {
    Timing::disabled("streaming.init", |timer| {
        init(
            store,
            history,
            InitRequest {
                source: &fixture.source,
                acquisition,
                stack: stack(),
                name: HistoryName::new("main").unwrap(),
                scope_seed: [29; 32],
                deadline: Instant::now() + Duration::from_secs(20),
            },
            timer,
        )
    })
    .0
}

#[test]
fn a_large_first_file_and_more_than_one_admission_window_publish_the_complete_root() {
    let (fixture, large) = large_first_fixture();
    let acquisition = Arc::new(MemoryAcquisition::default());
    let persistence = Arc::new(ObservedPersistence::new(
        acquisition.clone(),
        Action::Observe,
    ));
    let store = support::storage(persistence.clone());
    let history = MemoryHistory::default();
    let initialized = attempt(&fixture, &store, &history, &acquisition).unwrap();
    assert_eq!(initialized.entries, SMALL_FILES as u64 + 6);
    assert_eq!(initialized.namespace_work.unique_files, SMALL_FILES + 1);
    assert_eq!(initialized.namespace_work.regular_aliases, 1);
    assert!(initialized.namespace_work.file_admission_rows > 0);
    assert!(initialized.namespace_work.file_admission_rows <= 512);
    assert!(initialized.namespace_work.file_completed_rows > 0);
    assert!(initialized.namespace_work.file_completed_rows <= 512);
    assert!(initialized.namespace_work.file_output_batches > 0);
    assert_eq!(acquisition.calls("complete_files"), 0);
    assert_eq!(acquisition.calls("file_roots"), 0);
    assert!(acquisition.operations().is_empty());
    assert!(history.layer_stack(stack()).unwrap().is_some());
    let observed = persistence.observed.lock().unwrap();
    assert!(observed.payload_publications > 0);
    assert_eq!(observed.filesystem_roots, 1);
    drop(observed);

    let reader = store.reader().unwrap();
    let mut view = FilesystemRead::new(
        &reader,
        layerfs_content::filesystem::FilesystemRootId(initialized.root),
    )
    .unwrap();
    let first = view.resolve(&LogicalPath::new("a-first").unwrap()).unwrap();
    let alias = view
        .resolve(&LogicalPath::new("z-late/alias").unwrap())
        .unwrap();
    assert_eq!(first.serial, initialized.root_serial + 1);
    assert_eq!(first.serial, alias.serial);
    assert_eq!(first.value, alias.value);
    assert_eq!(first.value.namespace_ref_count, 2);
    let portable = layerfs_content::filesystem::attributes::read::read_portable(
        &reader,
        first.value.metadata_root,
        first.value.kind,
        &mut layerfs_content::filesystem::attributes::read::AttributeReadWork::default(),
    )
    .unwrap();
    let native = fs::metadata(fixture.source.join("a-first")).unwrap();
    assert_eq!(portable.mode, 0o640);
    assert_eq!(portable.mtime_seconds, native.mtime());
    assert_eq!(portable.mtime_nanoseconds as i64, native.mtime_nsec());
    let mut body = Vec::new();
    Timing::disabled("streaming.read", |timer| {
        read_all(
            &reader,
            first.value.content_root,
            &mut body,
            timer.child("file"),
        )
    })
    .0
    .unwrap();
    assert_eq!(body, large);
    for n in 0..SMALL_FILES {
        let file = view
            .resolve(&LogicalPath::new(&format!("b{n:05}")).unwrap())
            .unwrap();
        assert_eq!(file.value.namespace_ref_count, 1);
        body.clear();
        Timing::disabled("streaming.read", |timer| {
            read_all(
                &reader,
                file.value.content_root,
                &mut body,
                timer.child("file"),
            )
        })
        .0
        .unwrap();
        assert_eq!(body, small_body(n));
    }
    let empty = view.resolve(&LogicalPath::new("empty").unwrap()).unwrap();
    assert!(view
        .list_inode(empty.serial, None, 8, 1024)
        .unwrap()
        .entries
        .is_empty());
    assert_eq!(
        view.readlink(&LogicalPath::new("link").unwrap())
            .unwrap()
            .as_bytes(),
        b"../absent"
    );
}

#[test]
fn an_uncertain_payload_publication_stops_workers_and_retains_acquisition_custody() {
    let (fixture, _) = large_first_fixture();
    let acquisition = Arc::new(MemoryAcquisition::default());
    let persistence = Arc::new(ObservedPersistence::new(
        acquisition.clone(),
        Action::Uncertain,
    ));
    let store = support::storage(persistence.clone());
    let history = MemoryHistory::default();
    let outcome = attempt(&fixture, &store, &history, &acquisition);
    let Err(ProjectError::Uncertain { cause, retained }) = &outcome else {
        panic!("expected retained uncertain publication, got {outcome:?}");
    };
    let ProjectError::Storage(StorageError::UnknownOutcome { original }) = cause.as_ref() else {
        panic!("the publication's original uncertain outcome was lost: {cause:?}");
    };
    let StorageError::Io(original) = original.as_ref() else {
        panic!("the original provider outcome was lost: {original:?}");
    };
    assert_eq!(
        original
            .get_ref()
            .and_then(|error| error.downcast_ref::<PersistenceError>()),
        Some(&PersistenceError::Uncertain)
    );
    assert_eq!((retained.owner.operation, retained.owner.epoch), (1, 1));
    assert!(
        retained.work.is_none(),
        "uncertainty cannot read back charges"
    );
    assert_eq!(acquisition.calls("advance"), 0);
    assert_eq!(acquisition.calls("discard"), 0);
    assert_eq!(acquisition.calls("release"), 0);
    assert_eq!(acquisition.calls("work"), 0);
    assert_eq!(acquisition.operations().len(), 1);
    assert!(acquisition.operations()[0].2 > 0);
    assert!(history.layer_stack(stack()).unwrap().is_none());
    let observed = persistence.observed.lock().unwrap();
    assert!(observed.acted);
    assert_eq!(observed.payload_publications, 1, "no publication replay");
    assert_eq!(observed.filesystem_roots, 0);
}

#[test]
fn disposal_failure_leaves_the_final_filesystem_root_unpublished() {
    let (fixture, _) = large_first_fixture();
    let acquisition = Arc::new(MemoryAcquisition::default());
    acquisition.fail(
        "discard",
        1,
        PersistenceError::Refused {
            status: "cleanup refused".into(),
        }
        .into(),
    );
    let persistence = Arc::new(ObservedPersistence::new(
        acquisition.clone(),
        Action::Observe,
    ));
    let store = support::storage(persistence.clone());
    let history = MemoryHistory::default();
    let outcome = attempt(&fixture, &store, &history, &acquisition);
    assert!(
        matches!(outcome, Err(ProjectError::Cleanup { .. })),
        "{outcome:?}"
    );
    assert!(history.layer_stack(stack()).unwrap().is_none());
    assert_eq!(acquisition.calls("release"), 0);
    let observed = persistence.observed.lock().unwrap();
    assert!(observed.payload_publications > 0);
    assert_eq!(observed.filesystem_roots, 0);
    assert!(acquisition.operations()[0].2 > 0);
}

#[test]
fn a_later_alias_path_replaced_during_publication_is_rejected() {
    let (fixture, _) = large_first_fixture();
    let first = fixture.source.join("a-first");
    let before = fs::metadata(&first).unwrap();
    let acquisition = Arc::new(MemoryAcquisition::default());
    let persistence = Arc::new(ObservedPersistence::new(
        acquisition.clone(),
        Action::ReplaceAliasDirectory {
            current: fixture.source.join("z-late"),
            parked: fixture.path.join("parked-alias-directory"),
        },
    ));
    let store = support::storage(persistence.clone());
    let history = MemoryHistory::default();
    let outcome = attempt(&fixture, &store, &history, &acquisition);
    assert!(
        matches!(outcome, Err(ProjectError::InvalidInput)),
        "{outcome:?}"
    );
    let after = fs::metadata(first).unwrap();
    assert_eq!(
        (
            before.dev(),
            before.ino(),
            before.len(),
            before.ctime(),
            before.ctime_nsec()
        ),
        (
            after.dev(),
            after.ino(),
            after.len(),
            after.ctime(),
            after.ctime_nsec()
        ),
        "the first path's native evidence must remain unchanged"
    );
    assert!(persistence.observed.lock().unwrap().acted);
    assert!(acquisition.operations().is_empty());
    assert!(history.layer_stack(stack()).unwrap().is_none());
    assert_eq!(persistence.observed.lock().unwrap().filesystem_roots, 0);
}

#[test]
fn a_later_constructor_error_remains_visible_after_coalesced_successes() {
    let (fixture, _) = large_first_fixture();
    let acquisition = Arc::new(MemoryAcquisition::default());
    let persistence = Arc::new(ObservedPersistence::new(
        acquisition.clone(),
        Action::RemoveLaterCanonical {
            path: fixture.source.join(format!("b{:05}", SMALL_FILES - 1)),
        },
    ));
    let store = support::storage(persistence.clone());
    let history = MemoryHistory::default();
    let outcome = attempt(&fixture, &store, &history, &acquisition);
    let Err(ProjectError::Io(error)) = &outcome else {
        panic!("the constructor's original source failure was lost: {outcome:?}");
    };
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert!(persistence.observed.lock().unwrap().acted);
    assert_eq!(acquisition.calls("advance"), 1);
    assert!(acquisition.operations().is_empty());
    assert!(history.layer_stack(stack()).unwrap().is_none());
    assert_eq!(persistence.observed.lock().unwrap().filesystem_roots, 0);
}

#[test]
fn admission_bursts_close_empty_partial_and_exact_window_inputs() {
    for (count, expected_units) in [(0, 0), (1, 1), (512, 1), (513, 2), (768, 2), (769, 3)] {
        let fixture = Fixture::new(count);
        let acquisition = MemoryAcquisition::default();
        let store = support::storage(Arc::new(MemoryMetadata::default()));
        let history = MemoryHistory::default();
        let initialized = attempt(&fixture, &store, &history, &acquisition).unwrap();
        assert_eq!(initialized.namespace_work.unique_files, count);
        assert_eq!(
            initialized.namespace_work.file_admission_units,
            expected_units
        );
        assert!(initialized.namespace_work.file_admission_rows <= 512);
        assert!(initialized.namespace_work.file_completed_rows <= 512);
        assert!(acquisition.operations().is_empty());
        assert_eq!(
            initialized.entries as usize,
            count + count.min(10) + 1,
            "every scanned entry remains in the completed namespace"
        );
        let stack = history.layer_stack(stack()).unwrap().unwrap();
        assert_eq!(
            history.layer(stack.head_layer).unwrap().unwrap().root,
            initialized.root
        );

        let reader = store.reader().unwrap();
        let mut view = FilesystemRead::new(
            &reader,
            layerfs_content::filesystem::FilesystemRootId(initialized.root),
        )
        .unwrap();
        let root = view.resolve(&LogicalPath::new("").unwrap()).unwrap();
        assert_eq!(root.serial, initialized.root_serial);
        let mut body = Vec::new();
        for (path, expected) in &fixture.expected {
            let file = view.resolve(&LogicalPath::new(path).unwrap()).unwrap();
            body.clear();
            Timing::disabled("streaming.boundary.read", |timer| {
                read_all(
                    &reader,
                    file.value.content_root,
                    &mut body,
                    timer.child("file"),
                )
            })
            .0
            .unwrap();
            assert_eq!(&body, expected, "{count}-file input: {path}");
        }
        if count == 0 {
            assert!(view
                .list_inode(root.serial, None, 8, 1024)
                .unwrap()
                .entries
                .is_empty());
            assert_eq!(initialized.namespace_work.file_output_batches, 0);
        }
    }
}
