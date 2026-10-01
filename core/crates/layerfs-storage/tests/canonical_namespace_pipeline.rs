//! Actual C1 -> native profile8 C2 -> SaveHandoff with independent broad/deep oracles.
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod support;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod unix {
    use super::support::{create_store, disabled, open_store, TempDir};
    use layerfs_content::filesystem::attributes::build::build_attribute_tree;
    use layerfs_content::filesystem::inode::read::{lookup_many, InodeReadWork, InodeTable};
    use layerfs_content::filesystem::rows::{BindingRows, SliceBindingRows};
    use layerfs_content::filesystem::state::*;
    use layerfs_content::filesystem::{
        scope_for_seed, DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemPhases,
        FilesystemResources, FilesystemResult, InodeUpdate, PathName,
    };
    use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
    use layerfs_content::object::{CanonicalReadPermit, OwnedCanonicalBatch};
    use layerfs_content::{
        AuthenticatedObjects, ContentResult, FinalizedConsumer, FinalizedObject, ObjectId,
        ObjectRole,
    };
    use layerfs_storage::construction_state::{ScratchAuthority, ScratchSession};
    use layerfs_storage::{SaveHandoff, StorageError, Store, StoreProvider};
    use layerfs_telemetry::timer::TimingScope;
    use rusqlite::{Connection, OpenFlags};
    use std::cell::{Cell, RefCell};
    use std::collections::BTreeSet;
    use std::path::PathBuf;
    fn inspect(path: &std::path::Path) -> Connection {
        Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .unwrap()
    }
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct ReadPhase {
        counts: i64,
        release: i64,
    }
    #[derive(Clone, Debug)]
    struct Acquisition {
        phase: ReadPhase,
        fact: BaseFact,
    }
    #[derive(Clone, Debug, Default)]
    struct MemoTrace {
        scope: Option<Vec<u8>>,
        seen: BTreeSet<u64>,
        acquired: Vec<Acquisition>,
        last_owned: Option<ReadPhase>,
        owned_calls: Vec<ReadPhase>,
        final_known: Option<Vec<u64>>,
        current_known: Vec<u64>,
    }
    impl MemoTrace {
        fn serials(&self, phase: i64) -> Vec<u64> {
            self.acquired
                .iter()
                .filter(|a| a.phase.counts == phase)
                .map(|a| a.fact.serial)
                .collect()
        }
        fn cause(&self, counters: &layerfs_content::filesystem::FilesystemUpdateCounters) {
            let directory = self.serials(1).len() as u64;
            let effects = self.serials(2).len() as u64;
            let release = self.serials(3).len() as u64;
            let final_base = self.serials(4).len() as u64;
            assert_eq!(counters.references.base_records_read, directory + final_base,
                "reference base demands equal actual first memo insertions, including the directory parent");
            assert_eq!(
                counters.base_records_read, effects,
                "zero scan acquires only previously unknown base serials"
            );
            assert_eq!(
                counters.release.base_records, release,
                "descendants acquire only previously unknown base serials"
            );
            assert_eq!(
                counters.references.base_records_read
                    + counters.base_records_read
                    + counters.release.base_records,
                self.seen.len() as u64,
                "every counted BASE demand is a first acquisition; table-merge reads are separate"
            );
            assert_eq!(self.acquired.len(), self.seen.len());
            eprintln!("DIAGNOSTIC native memo first-acquisition cause: directory={:?} effects={:?} release={:?} final_base={:?}; owned_provider_call_phases={:?}; raw/final table-merge reads are separate", self.serials(1), self.serials(2), self.serials(3), self.serials(4), self.owned_calls);
        }
    }
    struct Probe<'a> {
        inner: StoreProvider<'a>,
        scratch: PathBuf,
        owned: Cell<u64>,
        selection: StateSelection,
        selected: GraphSubject,
        actual_table: Cell<Option<InodeTable>>,
        memo: RefCell<MemoTrace>,
        release_roots: RefCell<Vec<ObjectId>>,
    }
    impl Probe<'_> {
        fn memo_boundary(&self, db: &Connection) -> ReadPhase {
            let counts: i64 = db
                .query_row("SELECT stage FROM count_owner WHERE id=1", [], |r| r.get(0))
                .unwrap();
            let release: i64 = db
                .query_row("SELECT stage FROM release_owner WHERE id=1", [], |r| {
                    r.get(0)
                })
                .unwrap();
            let phase = ReadPhase { counts, release };
            let scope: Option<Vec<u8>> = db
                .query_row("SELECT scope FROM fact_owner WHERE table_id=17", [], |r| {
                    r.get(0)
                })
                .unwrap();
            if let Some(scope) = scope {
                assert_eq!(scope.len(), FACT_SCOPE_BYTES);
                let subject = FactSubject::decode(&self.selected, &scope[81..]).unwrap();
                assert_eq!(
                    subject.table(),
                    self.actual_table.get(),
                    "memo binds the actual authenticated selected table"
                );
                let expected = FactScope::new(self.selection.clone(), subject).unwrap();
                assert_eq!(scope.as_slice(), expected.encode().as_slice());
                let mut memo = self.memo.borrow_mut();
                if let Some(before) = &memo.scope {
                    assert_eq!(
                        before, &scope,
                        "same full SourceId/base/table scope through final EOF"
                    );
                } else {
                    memo.scope = Some(scope);
                }
                let mut query = db
                    .prepare("SELECT key,value FROM base_facts ORDER BY key")
                    .unwrap();
                let mut rows = query.query([]).unwrap();
                let mut current_known = Vec::new();
                while let Some(row) = rows.next().unwrap() {
                    let key: Vec<u8> = row.get(0).unwrap();
                    assert_eq!(key.len(), 25);
                    let serial = expected.serial(&key).unwrap();
                    current_known.push(serial);
                    let value: Vec<u8> = row.get(1).unwrap();
                    let fact = BaseFact::decode(serial, &value).unwrap();
                    // An insertion becomes visible after its owned read returns.
                    // Attribute new rows to that previous read's phase before
                    // recording any next phase's callback, including table merge.
                    if memo.seen.insert(serial) {
                        let acquisition = memo
                            .last_owned
                            .expect("new immutable fact follows a real owned base read");
                        memo.acquired.push(Acquisition {
                            phase: acquisition,
                            fact,
                        });
                    }
                }
                memo.current_known = current_known;
            }
            phase
        }
        fn consumer_boundary(&self, role: ObjectRole) {
            let db = inspect(&self.scratch);
            let phase = self.memo_boundary(&db);
            if matches!(role, ObjectRole::InodeLeaf | ObjectRole::InodeBranch) {
                assert_eq!(phase.counts, 4, "final consumer retains the same open memo");
                let base_stage: i64 = db
                    .query_row("SELECT stage FROM fact_owner WHERE table_id=17", [], |r| {
                        r.get(0)
                    })
                    .unwrap();
                assert_eq!(
                    base_stage, 1,
                    "BaseFacts stays open through final consumer EOF"
                );
                let mut memo = self.memo.borrow_mut();
                if let Some(known) = &memo.final_known {
                    assert_eq!(
                        known, &memo.current_known,
                        "actual memo rows persist through every final consumer event"
                    );
                } else {
                    memo.final_known = Some(memo.current_known.clone());
                }
            }
            // Every statement/row/connection is destroyed before C2 resumes.
        }
        fn observe(&self, ids: &[ObjectId]) {
            let db = inspect(&self.scratch);
            self.memo_boundary(&db);
            let stage: i64 = db
                .query_row("SELECT stage FROM release_owner WHERE id=1", [], |r| {
                    r.get(0)
                })
                .unwrap();
            if stage == 2 {
                let mut query = db
                    .prepare("SELECT value FROM release_frames ORDER BY key DESC LIMIT 1")
                    .unwrap();
                let mut rows = query.query([]).unwrap();
                if let Some(row) = rows.next().unwrap() {
                    let value: Vec<u8> = row.get(0).unwrap();
                    let root = ObjectId::from_bytes(&value[..32]).unwrap();
                    if ids.contains(&root) {
                        let mut seen = self.release_roots.borrow_mut();
                        if seen.last() != Some(&root) {
                            seen.push(root);
                        }
                    }
                }
            }
        }
    }
    impl AuthenticatedObjects for Probe<'_> {
        fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
            self.observe(ids);
            let result = self.inner.read_canonical_batch(ids)?;
            if let Some(base) = self.selected.base() {
                for (id, bytes) in ids.iter().zip(&result) {
                    if *id == base.0 {
                        let root = layerfs_content::filesystem::FilesystemRoot::decode(bytes)?;
                        assert_eq!(root.scope(), self.selected.namespace());
                        self.actual_table.set(Some(InodeTable {
                            root: root.inode_table(),
                            root_serial: root.root_inode().serial(),
                        }));
                    }
                }
            }
            Ok(result)
        }
        fn read_canonical_owned(
            &self,
            ids: &[ObjectId],
            permit: CanonicalReadPermit,
            scope: TimingScope<'_>,
        ) -> ContentResult<OwnedCanonicalBatch> {
            self.observe(ids);
            let db = inspect(&self.scratch);
            let phase = self.memo_boundary(&db);
            drop(db);
            {
                let mut memo = self.memo.borrow_mut();
                memo.last_owned = Some(phase);
                memo.owned_calls.push(phase);
            }
            self.owned.set(self.owned.get() + 1);
            self.inner.read_canonical_owned(ids, permit, scope)
        }
    }
    struct Audit<'a, 'b, 'p> {
        handoff: &'a mut SaveHandoff<'b>,
        authority: &'a ScratchAuthority,
        token: u64,
        probe: &'a Probe<'p>,
        events: Vec<ObjectRole>,
        first_inode_owned: Option<u64>,
        root_owned: Option<u64>,
    }
    impl FinalizedConsumer for Audit<'_, '_, '_> {
        fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
            self.probe.consumer_boundary(object.role());
            if matches!(
                object.role(),
                ObjectRole::InodeLeaf | ObjectRole::InodeBranch
            ) && self.first_inode_owned.is_none()
            {
                self.first_inode_owned = Some(self.probe.owned.get());
            }
            if object.role() == ObjectRole::FilesystemRoot {
                let status = self
                    .authority
                    .status()
                    .unwrap()
                    .into_iter()
                    .find(|s| s.token == self.token)
                    .unwrap();
                assert!(
                    status.known_clean,
                    "root emission requires genuine completed cleanup: {status:?}"
                );
                let db = inspect(&status.path);
                for table in [
                    "directory_roots",
                    "binding_sites",
                    "graph_nodes",
                    "graph_edges",
                    "alias_facts",
                    "alias_jobs",
                    "base_facts",
                    "parent_eligibility",
                    "canonical_counts",
                    "zero_seeds",
                    "release_jobs",
                    "release_frames",
                ] {
                    let records: i64 = db
                        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                        .unwrap();
                    assert_eq!(records, 0, "{table} before filesystem root");
                }
                let rows:i64=db.query_row("SELECT (SELECT COUNT(*) FROM session_owner)+(SELECT COUNT(*) FROM site_owner)+(SELECT COUNT(*) FROM graph_owner)+(SELECT COUNT(*) FROM solver_owner)+(SELECT COUNT(*) FROM alias_owner)+(SELECT COUNT(*) FROM fact_owner)+(SELECT COUNT(*) FROM count_owner)+(SELECT COUNT(*) FROM release_owner)",[],|r|r.get(0)).unwrap();
                assert_eq!(rows, 9);
                self.root_owned = Some(self.probe.owned.get());
            }
            self.events.push(object.role());
            self.handoff.accept(object)
        }
    }
    struct Run {
        result: FilesystemResult,
        release_roots: Vec<ObjectId>,
        first_inode_owned: u64,
        root_owned: u64,
        memo: MemoTrace,
    }
    fn run(store: &Store, authority: &ScratchAuthority, input: &FilesystemInput<'_>) -> Run {
        let source = SliceBindingRows::new(input).unwrap();
        let graph = GraphSubject::new(
            source.binding_source_id().unwrap(),
            input.scope,
            input.base,
            input.root_serial,
            GraphCapacity::default(),
        )
        .unwrap();
        let mut session: ScratchSession = authority
            .begin_canonical(
                [0x69; 32],
                input.directories.len() as u64,
                input
                    .directories
                    .iter()
                    .map(|d| d.changes.len() as u64)
                    .sum(),
                graph.clone(),
                AliasCapacity::new(
                    4096,
                    16 * 1024 * 1024,
                    input
                        .directories
                        .iter()
                        .map(|d| d.changes.len() as u64)
                        .sum(),
                )
                .unwrap(),
                FactCapacity::new(4096, 4096, 16 * 1024 * 1024).unwrap(),
                CanonicalCapacity::new(4096, 4096, 4096, 4096, 16 * 1024 * 1024).unwrap(),
            )
            .unwrap();
        let scopes = GraphConstructionScopes::new(session.selection().clone(), graph).unwrap();
        let status = authority
            .status()
            .unwrap()
            .into_iter()
            .find(|s| s.token == session.selection().token())
            .unwrap();
        let probe = Probe {
            inner: StoreProvider::new(store),
            scratch: status.path.clone(),
            owned: Cell::new(0),
            selection: session.selection().clone(),
            selected: scopes.graph().subject().clone(),
            actual_table: Cell::new(None),
            memo: RefCell::new(MemoTrace::default()),
            release_roots: RefCell::new(Vec::new()),
        };
        let run = disabled(|timing| -> Result<Run, StorageError> {
            let mut save = store.begin_save(timing.child("save"))?;
            let mut handoff = SaveHandoff::new(&mut save);
            let mut audit = Audit {
                handoff: &mut handoff,
                authority,
                token: status.token,
                probe: &probe,
                events: Vec::new(),
                first_inode_owned: None,
                root_owned: None,
            };
            let result = {
                let mut state = session.adapter();
                let mut objects = FilesystemObjects::new(&probe, &mut audit);
                let phases = FilesystemPhases::disabled();
                if input.base.is_none() {
                    layerfs_content::filesystem::update::build_filesystem_binding_rows_with_canonical_state(
                        &mut objects, &source, None, &mut state, &scopes, &phases,
                    )
                } else {
                    layerfs_content::filesystem::update::update_filesystem_binding_rows_with_canonical_state(
                        &mut objects, &source, None, &mut state, &scopes, &phases,
                    )
                }
            };
            if let Some(error) = session.take_failure() {
                return Err(error);
            }
            let result = result?;
            assert_eq!(audit.events.last(), Some(&ObjectRole::FilesystemRoot));
            let first_inode_owned = audit.first_inode_owned.unwrap();
            let root_owned = audit.root_owned.unwrap();
            drop(audit);
            if let Some(error) = handoff.take_failure() {
                return Err(error);
            }
            drop(handoff);
            save.finish(timing.child("finish"))?;
            Ok(Run {
                result,
                release_roots: probe.release_roots.borrow().clone(),
                first_inode_owned,
                root_owned,
                memo: probe.memo.borrow().clone(),
            })
        }).unwrap();
        assert!(authority.status().unwrap().iter().all(|s| s.known_clean));
        session.release().unwrap();
        assert!(!status.path.exists());
        assert_eq!(authority.reserved_bytes().unwrap(), 0);
        run
    }
    fn dependencies(store: &Store) -> (ObjectId, ObjectId) {
        disabled(|scope| -> Result<_, StorageError> {
            let mut save = store.begin_save(scope.child("save"))?;
            let mut handoff = SaveHandoff::new(&mut save);
            let policy = store.policy().construction();
            let file = layerfs_content::construct_bytes(
                policy,
                &policy.capacities(),
                b"native canonical cross-phase fixture",
                &mut handoff,
                scope.child("file"),
            )?
            .root;
            let metadata = {
                let reader = StoreProvider::new(store);
                let mut objects = FilesystemObjects::new(&reader, &mut handoff);
                build_attribute_tree(&mut objects, std::iter::empty())?.0
            };
            if let Some(error) = handoff.take_failure() {
                return Err(error);
            }
            drop(handoff);
            save.finish(scope.child("finish"))?;
            Ok((file, metadata))
        })
        .unwrap()
    }
    fn inode(serial: u64, kind: InodeKind, file: ObjectId, meta: ObjectId) -> InodeUpdate {
        InodeUpdate {
            serial,
            value: InodeValue {
                kind,
                namespace_ref_count: 0,
                content_root: file,
                metadata_root: meta,
            },
        }
    }
    #[test]
    fn actual_broad385_count_zero_final_population_saves_after_native9row_reset_and_reuses_facts() {
        let temp = TempDir::new("canonical_broad_save");
        let path = temp.store_path("objects");
        let store = create_store(&path);
        let scratch = TempDir::new("canonical_broad_scratch");
        let authority = ScratchAuthority::new(scratch.path(), 1).unwrap();
        let (file, meta) = dependencies(&store);
        let total = 385u64;
        let directory = [DirectoryUpdate {
            parent: 1,
            changes: (2..=total + 1)
                .map(|s| (PathName::new(&format!("f{s:04}")).unwrap(), Some(s)))
                .collect(),
        }];
        let values: Vec<_> = (1..=total + 1)
            .map(|s| {
                inode(
                    s,
                    if s == 1 {
                        InodeKind::Directory
                    } else {
                        InodeKind::RegularFile
                    },
                    file,
                    meta,
                )
            })
            .collect();
        let new: Vec<_> = (1..=total + 1).collect();
        let ns = scope_for_seed([0x2d; 32]);
        let initial = FilesystemInput {
            base: None,
            scope: ns,
            root_serial: 1,
            directories: &directory,
            inodes: &values,
            new_inodes: &new,
            resources: FilesystemResources::default(),
        };
        let base = run(&store, &authority, &initial).result;
        let remove = [DirectoryUpdate {
            parent: 1,
            changes: directory[0]
                .changes
                .iter()
                .map(|(name, _)| (name.clone(), None))
                .collect(),
        }];
        let update = FilesystemInput {
            base: Some(base.root),
            scope: ns,
            root_serial: 1,
            directories: &remove,
            inodes: &[],
            new_inodes: &[],
            resources: FilesystemResources::default(),
        };
        let changed = run(&store, &authority, &update);
        assert_eq!(changed.result.counters.references.final_removals, 385);
        changed.memo.cause(&changed.result.counters);
        eprintln!("DIAGNOSTIC owned provider callbacks at first inode={} and root={}; canonical table merging may still read", changed.first_inode_owned, changed.root_owned);
        assert_eq!(changed.memo.serials(0), Vec::<u64>::new());
        assert_eq!(
            changed.memo.serials(1),
            vec![1],
            "root is the first canonical directory base demand"
        );
        assert_eq!(changed.memo.serials(2), (2..=total + 1).collect::<Vec<_>>());
        assert_eq!(changed.memo.serials(3), Vec::<u64>::new());
        assert_eq!(
            changed.memo.serials(4),
            Vec::<u64>::new(),
            "final base demands reuse the still-open memo"
        );
        assert_eq!(changed.memo.final_known.as_ref(), Some(&new));
        for acquired in &changed.memo.acquired {
            assert_eq!(
                acquired.fact.value.unwrap().kind,
                if acquired.fact.serial == 1 {
                    InodeKind::Directory
                } else {
                    InodeKind::RegularFile
                }
            );
        }
        assert_eq!(changed.result.counters.references.base_records_read, 1);
        assert!(
            changed.release_roots.is_empty(),
            "regular seeds do not make directory frames"
        );
        drop(store);
        let reopened = open_store(&path);
        let found = lookup_many(
            &StoreProvider::new(&reopened),
            InodeTable {
                root: changed.result.value.inode_table(),
                root_serial: 1,
            },
            &new,
            &mut InodeReadWork::default(),
        )
        .unwrap();
        assert_eq!(found[0].unwrap().namespace_ref_count, 0);
        for row in &found[1..] {
            assert!(row.is_none());
        }
    }
    #[test]
    fn actual_deep180_release_matches_independent_chain_interpreter_and_preserves_outside_alias() {
        let temp = TempDir::new("canonical_deep_save");
        let path = temp.store_path("objects");
        let store = create_store(&path);
        let scratch = TempDir::new("canonical_deep_scratch");
        let authority = ScratchAuthority::new(scratch.path(), 1).unwrap();
        let (file, meta) = dependencies(&store);
        let depth = 180u64;
        let leaf = depth + 2;
        let mut directories = vec![DirectoryUpdate {
            parent: 1,
            changes: vec![
                (PathName::new("chain").unwrap(), Some(2)),
                (PathName::new("keep").unwrap(), Some(leaf)),
            ],
        }];
        for serial in 2..=depth + 1 {
            directories.push(DirectoryUpdate {
                parent: serial,
                changes: vec![(PathName::new("next").unwrap(), Some(serial + 1))],
            });
        }
        let values: Vec<_> = (1..=leaf)
            .map(|s| {
                inode(
                    s,
                    if s == leaf {
                        InodeKind::RegularFile
                    } else {
                        InodeKind::Directory
                    },
                    file,
                    meta,
                )
            })
            .collect();
        let new: Vec<_> = (1..=leaf).collect();
        let ns = scope_for_seed([0x3d; 32]);
        let initial = FilesystemInput {
            base: None,
            scope: ns,
            root_serial: 1,
            directories: &directories,
            inodes: &values,
            new_inodes: &new,
            resources: FilesystemResources::default(),
        };
        let base = run(&store, &authority, &initial).result;
        let before = lookup_many(
            &StoreProvider::new(&store),
            InodeTable {
                root: base.value.inode_table(),
                root_serial: 1,
            },
            &new,
            &mut InodeReadWork::default(),
        )
        .unwrap();
        let expected_frames: Vec<_> = before[1..depth as usize + 1]
            .iter()
            .map(|row| row.unwrap().content_root)
            .collect();
        let remove = [DirectoryUpdate {
            parent: 1,
            changes: vec![(PathName::new("chain").unwrap(), None)],
        }];
        let update = FilesystemInput {
            base: Some(base.root),
            scope: ns,
            root_serial: 1,
            directories: &remove,
            inodes: &[],
            new_inodes: &[],
            resources: FilesystemResources::default(),
        };
        let changed = run(&store, &authority, &update);
        assert_eq!(changed.result.counters.release.peak_depth, 180);
        assert_eq!(
            changed.release_roots, expected_frames,
            "native frame observations follow independently computed chain"
        );
        changed.memo.cause(&changed.result.counters);
        eprintln!("DIAGNOSTIC owned provider callbacks at first inode={} and root={}; canonical table merging may still read", changed.first_inode_owned, changed.root_owned);
        assert_eq!(changed.memo.serials(0), Vec::<u64>::new());
        assert_eq!(changed.memo.serials(1), vec![1]);
        assert_eq!(changed.memo.serials(2), vec![2]);
        assert_eq!(changed.memo.serials(3), (3..=leaf).collect::<Vec<_>>());
        assert!(changed
            .memo
            .acquired
            .iter()
            .filter(|a| a.phase.counts == 3)
            .all(|a| a.phase.release == 2));
        assert_eq!(changed.memo.serials(4), Vec::<u64>::new());
        assert_eq!(changed.memo.final_known.as_ref(), Some(&new));
        for acquired in &changed.memo.acquired {
            assert_eq!(
                acquired.fact.value.unwrap().kind,
                if acquired.fact.serial == leaf {
                    InodeKind::RegularFile
                } else {
                    InodeKind::Directory
                }
            );
        }
        assert_eq!(changed.result.counters.references.base_records_read, 1);
        drop(store);
        let reopened = open_store(&path);
        let found = lookup_many(
            &StoreProvider::new(&reopened),
            InodeTable {
                root: changed.result.value.inode_table(),
                root_serial: 1,
            },
            &new,
            &mut InodeReadWork::default(),
        )
        .unwrap();
        assert!(found[0].is_some());
        for row in &found[1..leaf as usize - 1] {
            assert!(row.is_none());
        }
        assert_eq!(found[leaf as usize - 1].unwrap().namespace_ref_count, 1);
    }
}
