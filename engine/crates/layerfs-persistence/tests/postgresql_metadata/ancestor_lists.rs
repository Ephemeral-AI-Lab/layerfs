//! #758 phase 2: PostgreSQL ancestor-location lists. Each stored row's list
//! is the chain its record headers state, for bases inside and outside the
//! publishing batch and for a base the batch lost to another writer; FULL
//! rows hold `''`; Stores without chains cost the port's default calls;
//! concurrent writers leave no row unrecorded; a cold read of a delta chain
//! costs one chain locate (two statements) and its group reads; a corrupted
//! entry is a counted stale hint, refused when strict; and a pre-2.0 Store is
//! refused at open. Every reader here is strict unless a test says otherwise.
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "../support/pg/chains.rs"]
mod chains;

use chains::*;
use layerfs_content::{FinalizedObject, ObjectId};
use layerfs_persistence::{PostgresCheck, PostgresHandles, PostgresStorage};
use layerfs_storage::{
    location::{InodeValueRow, LocatedObject, SignatureRow},
    port::*,
    Storage, StorageError, StoragePolicy,
};
use std::sync::{Arc, Mutex};

/// The port's default chain behaviour over the same Store: every call but
/// the chain locate, the group read and the batching knob is forwarded.
struct Plain(Arc<PostgresStorage>);

impl PackPersistence for Plain {
    fn policy(&self) -> Result<StoragePolicy, PersistenceError> {
        self.0.policy()
    }
    fn locate(
        &self,
        ids: &[ObjectId],
        out: &mut Vec<LocatedObject>,
    ) -> Result<(), PersistenceError> {
        self.0.locate(ids, out)
    }
    fn read_packs(
        &self,
        ids: &[i64],
        out: &mut Vec<PersistedPack>,
    ) -> Result<(), PersistenceError> {
        self.0.read_packs(ids, out)
    }
    fn read_pack_selection(
        &self,
        id: i64,
        plan: &mut dyn PackReadPlan,
    ) -> Result<PersistedPackRead, PersistenceError> {
        self.0.read_pack_selection(id, plan)
    }
    fn read_scoped_pack(
        &self,
        id: i64,
        plan: &mut dyn PackReadPlan,
    ) -> Result<AcquiredPackRead, PersistenceError> {
        self.0.read_scoped_pack(id, plan)
    }
    fn inode_values(
        &self,
        ordinals: &[u32],
        out: &mut Vec<InodeValueRow>,
    ) -> Result<(), PersistenceError> {
        self.0.inode_values(ordinals, out)
    }
    fn signatures(&self, out: &mut Vec<SignatureRow>) -> Result<(), PersistenceError> {
        self.0.signatures(out)
    }
    fn reserve(&self, request: Reserve) -> Result<Reserved, PersistenceError> {
        self.0.reserve(request)
    }
    fn whole_pack_bytes(&self) -> usize {
        self.0.whole_pack_bytes()
    }
    fn publication_bytes(&self) -> u64 {
        self.0.publication_bytes()
    }
    fn publication_pack_cost(
        &self,
        pack: &PublishedPack,
    ) -> Result<(usize, u64), PersistenceError> {
        self.0.publication_pack_cost(pack)
    }
    fn publish(&self, batch: &Publication) -> Result<Published, PersistenceError> {
        self.0.publish(batch)
    }
}

/// Forwards to a Store session; before its first publication, runs another
/// writer's step to completion, as a second daemon would between this Save's
/// locate and its publication.
struct Interleaved {
    inner: Arc<PostgresStorage>,
    before_publish: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl PackPersistence for Interleaved {
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
    fn locate_chains(
        &self,
        roots: &[ObjectId],
        rows: &mut Vec<LocatedObject>,
        chains: &mut layerfs_storage::location::LocatedChains,
    ) -> Result<(), PersistenceError> {
        self.inner.locate_chains(roots, rows, chains)
    }
    fn read_groups(
        &self,
        reads: &[GroupRead],
        out: &mut Vec<GroupReadAnswer>,
    ) -> Result<(), PersistenceError> {
        self.inner.read_groups(reads, out)
    }
    fn batched_chain_reads(&self) -> bool {
        self.inner.batched_chain_reads()
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
    fn inode_values(
        &self,
        ordinals: &[u32],
        out: &mut Vec<InodeValueRow>,
    ) -> Result<(), PersistenceError> {
        self.inner.inode_values(ordinals, out)
    }
    fn signatures(&self, out: &mut Vec<SignatureRow>) -> Result<(), PersistenceError> {
        self.inner.signatures(out)
    }
    fn reserve(&self, request: Reserve) -> Result<Reserved, PersistenceError> {
        self.inner.reserve(request)
    }
    fn whole_pack_bytes(&self) -> usize {
        self.inner.whole_pack_bytes()
    }
    fn publication_bytes(&self) -> u64 {
        self.inner.publication_bytes()
    }
    fn publication_pack_cost(
        &self,
        pack: &PublishedPack,
    ) -> Result<(usize, u64), PersistenceError> {
        self.inner.publication_pack_cost(pack)
    }
    fn publish(&self, batch: &Publication) -> Result<Published, PersistenceError> {
        if let Some(other) = self.before_publish.lock().unwrap().take() {
            other();
        }
        self.inner.publish(batch)
    }
}

#[test]
fn lists_are_the_record_header_chains_for_bases_in_and_out_of_the_batch() {
    let Some(url) = url() else { return };
    let store = Store::new(&url, "exact");
    let storage = storage(&store.handles);
    let mut files = Files::new(4, 24_000, 1);
    for _ in 0..6 {
        files.step(&storage, false);
    }
    // One batch whose bases are new in it: X, Y on X, Z on Y, plus the next
    // version of the files, whose bases are stored.
    let x = whole(&noise(77, 30_000));
    let mut edited = x.canonical().to_vec();
    edited[5_000..5_008].copy_from_slice(b"758-y001");
    let y = hinted(FinalizedObject::new(x.role(), edited.clone()).unwrap(), &x);
    edited[9_000..9_008].copy_from_slice(b"758-z001");
    let z = hinted(FinalizedObject::new(x.role(), edited).unwrap(), &y);
    let mut objects = files.emit(&storage, false);
    objects.extend([x.clone(), y.clone(), z.clone()]);
    save(&storage, objects);

    let deltas = assert_lists_exact(&store);
    assert!(deltas > 0, "the history stores deltas");
    let z_row = row(&store, z.id());
    let z_list = z_row.ancestors.clone().unwrap();
    let entries = decode(z_row.location.pack_id, &z_list);
    assert_eq!(entries.len(), 2, "Z on Y on FULL X: {entries:?}");
    let (y_row, x_row) = (row(&store, y.id()), row(&store, x.id()));
    assert_eq!(
        x_row.ancestors.as_deref(),
        Some(&[][..]),
        "a FULL row holds ''"
    );
    let y_location = y_row.location;
    assert_eq!(
        entries[0],
        (
            y_location.pack_id,
            y_location.group_number,
            y_location.record_number,
            y_location.canonical_length
        ),
        "entry 0 is the in-batch base's own row"
    );
    // A child's list is entry(base) || base's list, verbatim.
    let mut expected = Vec::new();
    entry(z_row.location.pack_id, &y_location, &mut expected);
    expected.extend_from_slice(y_row.ancestors.as_deref().unwrap());
    assert_eq!(z_list, expected);
    // Out-of-batch bases: the newest file versions' lists reach back into
    // earlier Saves, at least two levels deep.
    let deepest = files
        .latest()
        .iter()
        .map(|id| {
            let row = row(&store, *id);
            decode(row.location.pack_id, row.ancestors.as_deref().unwrap()).len()
        })
        .max()
        .unwrap();
    assert!(deepest >= 2, "out-of-batch chains {deepest}");
    assert_eq!(
        store.scalar("SELECT count(*) FROM object_location WHERE ancestors IS NULL"),
        0
    );
}

#[test]
fn a_lost_in_batch_base_gets_the_winners_location() {
    let Some(url) = url() else { return };
    let store = Store::new(&url, "lost");
    // The other writer stores W, then X as a delta on W.
    let w = whole(&noise(91, 28_000));
    let mut x_bytes = w.canonical().to_vec();
    x_bytes[1_000..1_008].copy_from_slice(b"758-x001");
    let x_canonical = x_bytes.clone();
    let x_on_w = hinted(FinalizedObject::new(w.role(), x_bytes.clone()).unwrap(), &w);
    let other = store.open();
    save(&storage(&other), vec![w.clone()]);
    // This Save holds X as a new FULL record and Y on X, both new when it
    // locates; the other writer publishes X first.
    x_bytes[20_000..20_008].copy_from_slice(b"758-y001");
    let x = FinalizedObject::new(w.role(), x_canonical).unwrap();
    let y = hinted(FinalizedObject::new(w.role(), x_bytes).unwrap(), &x);
    let other_storage = storage(&other);
    let racing = x_on_w.clone();
    let port = Arc::new(Interleaved {
        inner: store.handles.storage.clone(),
        before_publish: Mutex::new(Some(Box::new(move || {
            save(&other_storage, vec![racing]);
        }))),
    });
    let writer = Storage::new(port).unwrap();
    writer.set_strict_hints(true);
    save(&writer, vec![x.clone(), y.clone()]);

    let x_row = row(&store, x.id());
    let w_row = row(&store, w.id());
    let x_entries = decode(x_row.location.pack_id, x_row.ancestors.as_deref().unwrap());
    assert_eq!(x_entries.len(), 1, "the winner stored X on W");
    assert_eq!(x_entries[0].0, w_row.location.pack_id);
    let y_row = row(&store, y.id());
    let y_entries = decode(y_row.location.pack_id, y_row.ancestors.as_deref().unwrap());
    let winner = x_row.location;
    assert_eq!(
        y_entries,
        vec![
            (
                winner.pack_id,
                winner.group_number,
                winner.record_number,
                winner.canonical_length
            ),
            x_entries[0]
        ],
        "Y's list names the winning X, then X's list"
    );
    assert_lists_exact(&store);
    let (read, cost) = cold_read(&store.open(), &[y.id(), x.id()], true);
    assert_eq!(
        read.unwrap(),
        vec![y.canonical().to_vec(), x.canonical().to_vec()]
    );
    assert_eq!(cost.stale, 0);
}

#[test]
fn full_rewrites_hold_empty_lists_and_cost_the_default_calls() {
    let Some(url) = url() else { return };
    let (chained, plain) = (Store::new(&url, "full_c"), Store::new(&url, "full_p"));
    let (a, b) = (storage(&chained.handles), {
        let s = Storage::new(Arc::new(Plain(plain.handles.storage.clone()))).unwrap();
        s.set_strict_hints(true);
        s
    });
    let (mut files_a, mut files_b) = (Files::new(6, 20_000, 3), Files::new(6, 20_000, 3));
    let mut saves = Vec::new();
    for _ in 0..4 {
        let before = (
            chained.handles.diagnostics().unwrap(),
            plain.handles.diagnostics().unwrap(),
        );
        files_a.step(&a, true);
        files_b.step(&b, true);
        let after = (
            chained.handles.diagnostics().unwrap(),
            plain.handles.diagnostics().unwrap(),
        );
        saves.push((
            (
                after.0.statements - before.0.statements,
                after.0.round_trips - before.0.round_trips,
            ),
            (
                after.1.statements - before.1.statements,
                after.1.round_trips - before.1.round_trips,
            ),
        ));
    }
    for (chain, default) in &saves {
        assert_eq!(
            chain, default,
            "a full-rewrite Save costs the default statements: {saves:?}"
        );
    }
    assert_eq!(
        chained.scalar(
            "SELECT count(*) FROM object_location WHERE ancestors IS DISTINCT FROM ''::bytea"
        ),
        0,
        "every row is FULL"
    );
    assert_lists_exact(&chained);
    // The same reads through both: one statement per wave, no member packs.
    let roots = files_a.latest();
    let (warm_a, warm_b) = (chained.open(), plain.open());
    let _ = cold_read(&warm_a, &roots, true);
    let _ = cold_read(&warm_b, &roots, true);
    let (got, chain) = cold_read(&warm_a, &roots, true);
    let (want, default) = cold_read(&warm_b, &roots, true);
    assert_eq!(got.unwrap(), want.unwrap());
    assert_eq!(
        (chain.statements, chain.round_trips, chain.prepares),
        (default.statements, default.round_trips, default.prepares),
        "{chain:?} vs {default:?}"
    );
    assert_eq!((chain.stale, chain.hinted), (0, 0));
}

#[test]
fn eight_concurrent_writers_leave_every_row_recorded() {
    let Some(url) = url() else { return };
    let store = Store::new(&url, "eight");
    let roots: Vec<Vec<ObjectId>> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..8)
            .map(|writer| {
                let handles = store.open();
                scope.spawn(move || {
                    let storage = storage(&handles);
                    // Two content families shared by every writer, so the
                    // writers race for the same ids and bases.
                    let mut files = Files::new(3, 18_000, 1 + writer % 2);
                    for _ in 0..6 {
                        files.step(&storage, false);
                    }
                    files.latest()
                })
            })
            .collect();
        workers.into_iter().map(|w| w.join().unwrap()).collect()
    });
    assert_eq!(
        store.scalar("SELECT count(*) FROM object_location WHERE ancestors IS NULL"),
        0,
        "no row without a list"
    );
    let deltas = assert_lists_exact(&store);
    assert!(deltas > 0);
    let ids: Vec<ObjectId> = roots.concat();
    let (strict, cost) = cold_read(&store.open(), &ids, true);
    let (plain, _) = cold_read(&store.open(), &ids, false);
    assert_eq!(strict.unwrap(), plain.unwrap());
    assert_eq!(cost.stale, 0, "{cost:?}");
}

/// A depth-d chain read cold: one chain locate (statement 1 and the member
/// packs' statement 2) and its group read, whatever d is; the session keeps
/// the member packs' rows, so its next cold read skips statement 2.
#[test]
fn a_cold_read_of_a_delta_chain_costs_one_chain_locate() {
    let Some(url) = url() else { return };
    let store = Store::new(&url, "cold");
    let storage = storage(&store.handles);
    let mut files = Files::new(1, 26_000, 5);
    for _ in 0..10 {
        files.step(&storage, false);
    }
    let root = files.latest()[0];
    let depth = {
        let row = row(&store, root);
        decode(row.location.pack_id, row.ancestors.as_deref().unwrap()).len()
    };
    assert!(depth >= 5, "fixture depth {depth}");
    let handles = store.open();
    let (read, cost) = cold_read(&handles, &[root], true);
    let (plain, _) = cold_read(&handles, &[root], false);
    assert_eq!(read.unwrap(), plain.unwrap());
    assert_eq!(cost.waves, 1, "{cost:?}");
    assert_eq!(
        cost.hinted, depth as u64,
        "every base bound from its member"
    );
    assert_eq!(cost.stale, 0);
    // statement 1, statement 2 and one group read of under 2 MiB, each
    // prepared once on this fresh session
    assert!(cost.read_bytes < 2 * 1024 * 1024);
    assert_eq!((cost.statements, cost.prepares), (3, 3), "{cost:?}");
    assert_eq!(cost.round_trips, cost.statements + cost.prepares);
    assert!(cost.statements <= cost.a8_budget());
    // Again on the same session: statement 1 and the group read.
    let (_, again) = cold_read(&handles, &[root], true);
    assert_eq!(
        (again.waves, again.stale, again.prepares),
        (1, 0, 0),
        "{again:?}"
    );
    assert_eq!(again.round_trips, 2, "{again:?}");
    assert!(again.round_trips <= again.a8_budget());
    // the default port pays a locate per level
    let plain_port = Storage::new(Arc::new(Plain(handles.storage.clone()))).unwrap();
    let _ = plain_port.reader().unwrap().read_objects(&[root]).unwrap();
    assert_eq!(plain_port.diagnostics().locate, depth as u64 + 1);
}

#[test]
fn a_corrupted_entry_is_a_counted_stale_hint_and_strict_refuses_it() {
    let Some(url) = url() else { return };
    let store = Store::new(&url, "corrupt");
    let storage = storage(&store.handles);
    let mut files = Files::new(2, 22_000, 7);
    for _ in 0..5 {
        files.step(&storage, false);
    }
    let root = files.latest()[0];
    let original = row(&store, root);
    let entries = decode(
        original.location.pack_id,
        original.ancestors.as_deref().unwrap(),
    );
    assert!(entries.len() >= 2);
    // Entry 0 names another stored record (the root itself), so the member
    // the reader binds is not the base the root's record names.
    let mut list = Vec::new();
    entry(original.location.pack_id, &original.location, &mut list);
    let mut previous = original.location.pack_id;
    for (pack, group, record, length) in &entries[1..] {
        let at = layerfs_storage::location::ObjectLocation {
            pack_id: *pack,
            group_number: *group,
            record_number: *record,
            canonical_length: *length,
            ..original.location
        };
        entry(previous, &at, &mut list);
        previous = *pack;
    }
    store
        .admin()
        .execute(
            "UPDATE object_location SET ancestors=$1 WHERE object_id=$2",
            &[&list, &root.as_bytes().as_slice()],
        )
        .unwrap();
    let handles = store.open();
    let (expected, _) = cold_read(&handles, &[files.latest()[1]], false);
    expected.unwrap();
    let (lenient, cost) = cold_read(&handles, &[root], false);
    let (plain, _) = {
        let port = Storage::new(Arc::new(Plain(handles.storage.clone()))).unwrap();
        (port.reader().unwrap().read_objects(&[root]), ())
    };
    assert_eq!(lenient.unwrap(), plain.unwrap(), "the read stays correct");
    assert_eq!(cost.stale, 1, "{cost:?}");
    let (strict, _) = cold_read(&handles, &[root], true);
    assert!(
        matches!(strict, Err(StorageError::Integrity("delta chain hint"))),
        "{strict:?}"
    );
}

#[test]
fn a_pre_2_0_store_is_refused_at_open() {
    let Some(url) = url() else { return };
    let store = Store::new(&url, "pre20");
    store
        .admin()
        .batch_execute(
            "ALTER TABLE history_meta DROP CONSTRAINT history_meta_schema_version_check; \
             ALTER TABLE store_policy DROP CONSTRAINT store_policy_schema_version_check",
        )
        .unwrap();
    // Version 3 (2.0 before each Commit recorded its attempt, #776 R1) is
    // refused by its own name too.
    for version in [2, 3] {
        store
            .admin()
            .batch_execute(&format!(
                "UPDATE history_meta SET schema_version={version}; \
                 UPDATE store_policy SET schema_version={version}"
            ))
            .unwrap();
        let name = format!("PostgreSQL Store schema version {version}");
        for check in [PostgresCheck::Full, PostgresCheck::Light] {
            let selection = store.selection.clone().with_check(check);
            for opened in [
                PostgresHandles::open_read_only(&selection, BINDING, CURSOR).err(),
                PostgresHandles::open_writable(&selection, BINDING, CURSOR).err(),
            ] {
                assert!(
                    matches!(&opened, Some(PersistenceError::Refused { status })
                        if status.starts_with(&name)),
                    "{version} {check:?}: {opened:?}"
                );
            }
        }
    }
}

/// A version 2 Store written before T7.1 has no `kept_node` table. A
/// writable open migrates nothing (#776): it is refused by name like any
/// version 2 Store, not `Integrity` from an in-place migration that could
/// never match a version 2 schema source.
#[test]
fn a_pre_2_0_store_missing_later_tables_is_refused_by_name() {
    let Some(url) = url() else { return };
    let store = Store::new(&url, "pre20kept");
    store
        .admin()
        .batch_execute(
            "DROP TABLE kept_node; \
             ALTER TABLE history_meta DROP CONSTRAINT history_meta_schema_version_check; \
             ALTER TABLE store_policy DROP CONSTRAINT store_policy_schema_version_check; \
             UPDATE history_meta SET schema_version=2; UPDATE store_policy SET schema_version=2",
        )
        .unwrap();
    for check in [PostgresCheck::Full, PostgresCheck::Light] {
        let selection = store.selection.clone().with_check(check);
        let opened = PostgresHandles::open_writable(&selection, BINDING, CURSOR).err();
        assert!(
            matches!(&opened, Some(PersistenceError::Refused { status })
                if status.starts_with("PostgreSQL Store schema version 2")),
            "{check:?}: {opened:?}"
        );
    }
}

/// A version 2 Store written before T1.2a has no `store_policy.trace_salt`.
/// The open check selected that column by name, so the statement itself
/// failed and the Store was refused `Integrity`/`Malformed` instead of by its
/// version (#776). Both checks and both opens now name it.
#[test]
fn a_pre_2_0_store_without_the_trace_salt_is_refused_by_name() {
    let Some(url) = url() else { return };
    let store = Store::new(&url, "pre20salt");
    store
        .admin()
        .batch_execute(
            "ALTER TABLE store_policy DROP COLUMN trace_salt; \
             ALTER TABLE history_meta DROP CONSTRAINT history_meta_schema_version_check; \
             ALTER TABLE store_policy DROP CONSTRAINT store_policy_schema_version_check; \
             UPDATE history_meta SET schema_version=2; UPDATE store_policy SET schema_version=2",
        )
        .unwrap();
    for check in [PostgresCheck::Full, PostgresCheck::Light] {
        let selection = store.selection.clone().with_check(check);
        for opened in [
            PostgresHandles::open_read_only(&selection, BINDING, CURSOR).err(),
            PostgresHandles::open_writable(&selection, BINDING, CURSOR).err(),
        ] {
            assert!(
                matches!(&opened, Some(PersistenceError::Refused { status })
                    if status.starts_with("PostgreSQL Store schema version 2")),
                "{check:?}: {opened:?}"
            );
        }
    }
}
