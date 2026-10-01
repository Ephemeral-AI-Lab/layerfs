//! Genuine table/file/EOF proof and ordinary Canonical8 composition, no native fit claim.
mod support;
use layerfs_content::filesystem::{
    root::FilesystemRootId,
    rows::{
        BindingRows, PreparedBindingUpdate, RowSource, SmallFileBuilder, SpoolDeclaration,
        SpoolPreparation,
    },
    state::*,
    DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemResources, InodeUpdate,
    PathName,
};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::{ContentError, ObjectId};
use std::io::{self, Cursor};
use support::filesystem::{synthetic, value, with_objects, Session, TreeStore};
fn fixture(count: usize) -> (Session, Vec<u64>) {
    let mut session = Session::new(1).unwrap();
    let serials = (0..count).map(|_| session.allocate()).collect::<Vec<_>>();
    let directory = DirectoryUpdate {
        parent: 1,
        changes: serials
            .iter()
            .enumerate()
            .map(|(n, s)| (PathName::new(&format!("f{n:02}")).unwrap(), Some(*s)))
            .collect(),
    };
    let rows = serials
        .iter()
        .map(|s| InodeUpdate {
            serial: *s,
            value: value(
                InodeKind::RegularFile,
                synthetic("old content"),
                synthetic("old metadata"),
            ),
        })
        .collect::<Vec<_>>();
    session.apply(&[directory], &rows, &serials).unwrap();
    (session, serials)
}
fn preparation(count: usize) -> SpoolPreparation {
    SpoolPreparation::new(
        SpoolDeclaration {
            directories: 0,
            inodes: count,
            fresh: 0,
            bindings: 0,
            wire_name_bytes: 0,
        },
        1024,
    )
    .unwrap()
}
fn subject(session: &Session, p: &SpoolPreparation) -> GraphSubject {
    GraphSubject::new(
        p.source_id(),
        session.scope,
        Some(FilesystemRootId(session.root)),
        1,
        GraphCapacity::default(),
    )
    .unwrap()
}
#[test]
fn eight_existing_files_reproduce_frozen_legacy_root_through_real_canonical_coordinator() {
    let (session, serials) = fixture(8);
    let changes = serials
        .iter()
        .map(|s| InodeUpdate {
            serial: *s,
            value: value(
                InodeKind::RegularFile,
                synthetic("new content"),
                synthetic("new metadata"),
            ),
        })
        .collect::<Vec<_>>();
    // Expected root is computed before candidate output through the published
    // compatibility producer, not from the new small authority's own result.
    let mut legacy = session.store.clone();
    let expected = with_objects(&mut legacy, |objects| {
        layerfs_content::filesystem::update_filesystem(
            objects,
            &FilesystemInput {
                base: Some(FilesystemRootId(session.root)),
                scope: session.scope,
                root_serial: 1,
                directories: &[],
                inodes: &changes,
                new_inodes: &[],
                resources: FilesystemResources::default(),
            },
            None,
        )
    })
    .unwrap();
    let prep = preparation(8);
    let source = prep.source_id();
    let selected = subject(&session, &prep);
    let mut builder = SmallFileBuilder::new(prep, selected, &session.store).unwrap();
    for row in &changes {
        builder.push(&session.store, *row).unwrap();
    }
    let rows = builder.finish(&mut io::empty()).unwrap();
    assert_eq!(rows.source_id(), source);
    let mut state = VerifiedSmallFileState::new([81; 32], &rows).unwrap();
    let scopes = state.scopes().clone();
    let input = PreparedBindingUpdate {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        resources: FilesystemResources::default(),
        rows: &rows,
    };
    let mut output = TreeStore::new();
    let result = layerfs_telemetry::timer::Timing::disabled("small", |scope| {
        layerfs_content::filesystem::update::update_filesystem_binding_rows_with_canonical_state(
            &mut FilesystemObjects::new(&session.store, &mut output),
            &input,
            None,
            &mut state,
            &scopes,
            &layerfs_content::filesystem::FilesystemPhases::new(scope),
        )
    })
    .0
    .unwrap();
    assert_eq!(result.root, expected.root);
    assert_eq!(result.value, expected.value);
    assert!(state.completed());
    let physical = state.physical();
    assert_eq!(physical.native_binding, None);
    assert_eq!(
        (
            physical.native_files,
            physical.reserved_bytes,
            physical.allocated_bytes,
            physical.cleanup_events
        ),
        (0, 0, 0, 0)
    );
    assert!(state.working_bytes() <= GRAPH_WORKING_BYTES);
    let mut merged = session.store.clone();
    merged.absorb(&output);
    let mut read = layerfs_content::filesystem::FilesystemRead::new(&merged, result.root).unwrap();
    for serial in serials {
        let found = read.resolve_inode(serial).unwrap().value;
        assert_eq!(found.kind, InodeKind::RegularFile);
        assert_eq!(found.namespace_ref_count, 1);
        assert_eq!(found.content_root, synthetic("new content"));
    }
}

#[test]
fn cardinality_actual_kind_foreign_table_and_trailing_eof_refuse_without_native_effects() {
    let (session, serials) = fixture(1);
    let counted = support::filesystem::CountingProvider::new(&session.store);
    let prep = preparation(9);
    let selected = subject(&session, &prep);
    assert!(SmallFileBuilder::new(prep, selected, &counted).is_err());
    assert_eq!(counted.demands(), 0);
    let row = InodeUpdate {
        serial: serials[0],
        value: value(InodeKind::RegularFile, synthetic("new"), synthetic("meta")),
    };
    let prep = preparation(1);
    let selected = subject(&session, &prep);
    let mut builder = SmallFileBuilder::new(prep, selected, &session.store).unwrap();
    assert!(builder
        .push(
            &session.store,
            InodeUpdate {
                serial: row.serial,
                value: value(InodeKind::Symlink, synthetic("wrong"), synthetic("meta"))
            }
        )
        .is_err());
    builder.push(&session.store, row).unwrap();
    assert!(matches!(
        builder.finish(&mut Cursor::new([1])),
        Err(ContentError::TrailingBytes)
    ));
    let prep = preparation(1);
    let selected = subject(&session, &prep);
    let mut builder = SmallFileBuilder::new(prep, selected, &session.store).unwrap();
    builder.push(&session.store, row).unwrap();
    let rows = builder.finish(&mut io::empty()).unwrap();
    let mut state = VerifiedSmallFileState::new([82; 32], &rows).unwrap();
    let foreign = FactScope::new(
        state.selection().clone(),
        FactSubject::new(
            state.subject().clone(),
            Some(layerfs_content::filesystem::inode::read::InodeTable {
                root: ObjectId::for_bytes(b"foreign table"),
                root_serial: 1,
            }),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(state.fact_bind(&foreign).is_err());
    assert_eq!(state.physical().native_files, 0);
    assert!(state
        .graph_seed_batch(&state.scopes().graph().clone(), &[])
        .is_err());
    assert_eq!(rows.inode_rows(), 1);
    assert!(rows
        .directory_headers()
        .unwrap()
        .next_header()
        .unwrap()
        .is_none());
}

#[test]
fn authenticated_zero_reference_regular_file_uses_real_job_and_is_removed_like_original() {
    use layerfs_content::object::inode_leaf::{encode_inode_value, InodeLeaf};
    use layerfs_content::{AuthenticatedObjects, ObjectRole};
    let (mut session, serials) = fixture(1);
    let bytes = session
        .store
        .read_canonical(session.value.inode_table())
        .unwrap();
    let mut leaf = InodeLeaf::decode(&bytes).unwrap();
    let old = value(
        InodeKind::RegularFile,
        synthetic("old content"),
        synthetic("old metadata"),
    );
    leaf.rows
        .iter_mut()
        .find(|r| r.serial == serials[0])
        .unwrap()
        .value = encode_inode_value(old);
    let table = session
        .store
        .insert(ObjectRole::InodeLeaf, leaf.encode().unwrap());
    session.value = session.value.with_inode_table(table);
    session.root = session
        .store
        .insert(ObjectRole::FilesystemRoot, session.value.encode().unwrap());
    let change = InodeUpdate {
        serial: serials[0],
        value: value(
            InodeKind::RegularFile,
            synthetic("new content"),
            synthetic("new metadata"),
        ),
    };
    let mut legacy = session.store.clone();
    let expected = with_objects(&mut legacy, |objects| {
        layerfs_content::filesystem::update_filesystem(
            objects,
            &FilesystemInput {
                base: Some(FilesystemRootId(session.root)),
                scope: session.scope,
                root_serial: 1,
                directories: &[],
                inodes: &[change],
                new_inodes: &[],
                resources: FilesystemResources::default(),
            },
            None,
        )
    })
    .unwrap();
    let prep = preparation(1);
    let selected = subject(&session, &prep);
    let mut builder = SmallFileBuilder::new(prep, selected, &session.store).unwrap();
    builder.push(&session.store, change).unwrap();
    let rows = builder.finish(&mut io::empty()).unwrap();
    assert_eq!(
        rows.base(change.serial)
            .unwrap()
            .value
            .unwrap()
            .namespace_ref_count,
        0
    );
    let mut state = VerifiedSmallFileState::new([84; 32], &rows).unwrap();
    let scopes = state.scopes().clone();
    let input = PreparedBindingUpdate {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        resources: FilesystemResources::default(),
        rows: &rows,
    };
    let mut output = TreeStore::new();
    let result = layerfs_telemetry::timer::Timing::disabled("small-zero-job", |scope| {
        layerfs_content::filesystem::update::update_filesystem_binding_rows_with_canonical_state(
            &mut FilesystemObjects::new(&session.store, &mut output),
            &input,
            None,
            &mut state,
            &scopes,
            &layerfs_content::filesystem::FilesystemPhases::new(scope),
        )
    })
    .0
    .unwrap();
    assert_eq!((result.root, result.value), (expected.root, expected.value));
    assert!(state.completed());
    let mut merged = session.store.clone();
    merged.absorb(&output);
    let found = layerfs_content::filesystem::inode::read::lookup(
        &merged,
        layerfs_content::filesystem::inode::read::InodeTable {
            root: result.value.inode_table(),
            root_serial: 1,
        },
        change.serial,
        &mut Default::default(),
    )
    .unwrap();
    assert_eq!(found, None);
    assert_eq!(state.physical().native_files, 0);
}

#[test]
fn prospective_pending_credit_transfers_once_and_checked_split_keeps_exact_live_total() {
    use layerfs_content::filesystem::rows::PendingSmallFiles;
    let (session, serials) = fixture(1);
    let prep = preparation(1);
    let source = prep.source_id();
    let selected = subject(&session, &prep);
    let pending = PendingSmallFiles::new([85; 32], selected, prep).unwrap();
    let held = pending.working_bytes();
    assert!(held <= GRAPH_WORKING_BYTES);
    assert_eq!(pending.selector(), Some([85; 32]));
    let mut builder = SmallFileBuilder::from_pending(pending, &session.store).unwrap();
    builder
        .push(
            &session.store,
            InodeUpdate {
                serial: serials[0],
                value: value(InodeKind::RegularFile, synthetic("new"), synthetic("meta")),
            },
        )
        .unwrap();
    let rows = builder.finish(&mut io::empty()).unwrap();
    assert_eq!(rows.source_id(), source);
    assert_eq!(rows.memory().reserved_bytes(), held);
    assert!(VerifiedSmallFileState::new([86; 32], &rows).is_err());
    assert_eq!(rows.memory().reserved_bytes(), held);
    let state = VerifiedSmallFileState::from_rows(&rows).unwrap();
    assert!(
        state.working_bytes() < held,
        "only unused prospective window is released for actual page leases"
    );
    assert!(VerifiedSmallFileState::from_rows(&rows).is_err());
    let memory = rows.memory();
    drop(state);
    drop(rows);
    assert_eq!(memory.reserved_bytes(), GraphMemory::control_bytes());
    let memory = GraphMemory::new();
    let mut parent = memory.reserve(4096).unwrap();
    assert!(parent.split(0).is_err());
    assert!(parent.split(4097).is_err());
    assert_eq!(parent.bytes(), 4096);
    let child = parent.split(1024).unwrap();
    assert_eq!(parent.bytes() + child.bytes(), 4096);
    assert_eq!(memory.reserved_bytes(), GraphMemory::control_bytes() + 4096);
    drop(parent);
    assert_eq!(memory.reserved_bytes(), GraphMemory::control_bytes() + 1024);
    drop(child);
    assert_eq!(memory.reserved_bytes(), GraphMemory::control_bytes());
}

fn graph_retired(
    state: &mut VerifiedSmallFileState,
    table: layerfs_content::filesystem::inode::read::InodeTable,
) -> FactScope {
    let scopes = state.scopes().clone();
    let subject = FactSubject::new(state.subject().clone(), Some(table)).unwrap();
    let facts = FactScope::new(state.selection().clone(), subject.clone()).unwrap();
    let parents = FactScope::parents(state.selection().clone(), subject).unwrap();
    state.fact_bind(&facts).unwrap();
    state.parent_bind(&parents).unwrap();
    state.parent_close_declarations(&parents).unwrap();
    let parents = state.parent_seal(&parents).unwrap();
    state.parent_page(&parents, None, 128, 65536).unwrap();
    let births = SiteBirthLedger::new(scopes.sites().clone()).unwrap().seal();
    let members = state.site_close_membership(&births).unwrap();
    state.alias_begin(&members, None).unwrap();
    let aliases = state.alias_finish(&members).unwrap();
    state.alias_retire(&aliases).unwrap();
    let sites = state.site_final_seal(&members).unwrap();
    state
        .site_sealed_page(&sites, None, SitePageLimit::default())
        .unwrap();
    state.site_retire(&sites).unwrap();
    assert!(state.graph_unexpanded(scopes.graph()).unwrap().is_none());
    let adjacency = state.graph_seal(scopes.graph()).unwrap();
    state
        .graph_node_page(&adjacency, None, GraphPageLimit::default())
        .unwrap();
    state.graph_begin_scc(&adjacency).unwrap();
    state
        .graph_node_page(&adjacency, None, GraphPageLimit::default())
        .unwrap();
    let proof = state.graph_finish(&adjacency).unwrap();
    state
        .graph_proof_page(&proof, None, GraphPageLimit::default())
        .unwrap();
    state.graph_retire(&proof).unwrap();
    facts
}
#[test]
fn expected_proposed_eight_counts_and_slow_held_pages_refuse_growth_without_losing_credit() {
    use layerfs_content::filesystem::references::record::Row;
    let (session, serials) = fixture(8);
    let prep = preparation(8);
    let selected = subject(&session, &prep);
    let mut builder = SmallFileBuilder::new(prep, selected, &session.store).unwrap();
    for serial in serials {
        builder
            .push(
                &session.store,
                InodeUpdate {
                    serial,
                    value: value(InodeKind::RegularFile, synthetic("new"), synthetic("meta")),
                },
            )
            .unwrap();
    }
    let rows = builder.finish(&mut io::empty()).unwrap();
    let mut state = VerifiedSmallFileState::new([87; 32], &rows).unwrap();
    let facts = graph_retired(&mut state, rows.table());
    let counts =
        CanonicalScope::counts(state.selection().clone(), facts.subject().clone()).unwrap();
    state.count_begin(&counts).unwrap();
    assert!(state.count_begin(&counts).is_err());
    for n in 0..8 {
        let row = rows.row(n).unwrap();
        let after = CountRecord {
            row: Row::Effect {
                serial: row.serial,
                value: Some(row.value),
                delta: 0,
            },
            touched: true,
        };
        assert_eq!(state.count_cas(&counts, None, &after).unwrap(), after);
        assert!(state.count_cas(&counts, None, &after).is_err());
    }
    let bad = CountRecord {
        row: Row::Effect {
            serial: i64::MAX as u64,
            value: Some(rows.row(0).unwrap().value),
            delta: 0,
        },
        touched: true,
    };
    assert!(state.count_cas(&counts, None, &bad).is_err());
    assert!(state.fact_get(&facts, i64::MAX as u64).is_err());
    let seal = state.count_seal(&counts, CountEpoch::Effects).unwrap();
    assert_eq!(seal.records, 8);
    let before = state.working_bytes();
    let first = state.count_page(&seal, None, 128, 65536).unwrap();
    assert!(first.eof);
    assert_eq!(first.records().len(), 8);
    let one = state.working_bytes() - before;
    assert!(one >= std::mem::size_of::<CountPage>() + 8 * std::mem::size_of::<CountRecord>());
    let mut held = vec![first];
    loop {
        assert!(
            held.len() < 128,
            "fixed64KiB must refuse repeated real page holds"
        );
        match state.count_page(&seal, None, 128, 65536) {
            Ok(page) => held.push(page),
            Err(ContentError::ResourceUnavailable { .. }) => break,
            Err(other) => panic!("unexpected page error {other:?}"),
        }
    }
    let exhausted = state.working_bytes();
    assert!(state.count_page(&seal, None, 128, 65536).is_err());
    assert_eq!(state.working_bytes(), exhausted);
    assert!(held
        .iter()
        .all(|page| page.records().len() == 8 && page.seal == seal));
    drop(held);
    assert_eq!(state.working_bytes(), before);
    let again = state.count_page(&seal, None, 128, 65536).unwrap();
    assert_eq!(again.records().len(), 8);
    drop(again);
    let zeros = state.zero_seal(&seal).unwrap();
    let held_zero = state.zero_page(&zeros, None, 128, 65536).unwrap();
    assert!(held_zero.eof && held_zero.records().is_empty());
    let held_bytes = state.working_bytes();
    state.count_resume(&zeros).unwrap();
    drop(held_zero);
    assert!(state.working_bytes() < held_bytes);
    state.abandon();
    assert!(state
        .count_get(&counts, rows.row(0).unwrap().serial)
        .is_err());
}
