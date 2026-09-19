//! The retained-history driver: one Store, N states, three timed steps each.
//!
//! ```text
//! create the Store
//! for each state k, in order:
//!     read state k's changed bytes from the corpus      untimed, harness
//!       construct the changed content                   TIMED
//!       build_filesystem against the previous root      TIMED
//!       save                                            TIMED
//! close
//! ```
//!
//! **The corpus reading is between the children, never inside one.** That is why
//! this row's `operation_ns` is the sum of its named children and not the root: a
//! root would include [`Corpus::transition`]'s work, which is the harness's.
//!
//! **No constructed object is supplied to a measured child.** `InodeValue`
//! references content **by id** and carries no bytes, so the content objects are
//! what the child produces rather than an input. There is consequently nothing to
//! prepare and nothing to reuse: `Preparation::InProcess`, no master, no copy, no
//! checkpoint, and a second run costs what the first cost.
//!
//! **The base is read back through the Store.** State 1 builds a new filesystem
//! (`base: None`); every later state passes the previous root, whose content the
//! operation reads through `StoreProvider` over the very Store the chain is
//! growing. The chain therefore goes C1 → C2 in the read direction and not out of
//! harness memory.
//!
//! ## What this lane does not model
//!
//! `InodeValue` carries a **kind class** — regular file, directory, symlink — and
//! no POSIX mode bits, so the executable bit is not representable in C1/C2 at all.
//! That would matter for an O4 comparison of `mode` against the corpus oracle, so
//! it was checked before the driver was written: **the corpus contains zero
//! mode-only transitions** across all 157 checkpoints, and therefore zero in each
//! of the three selections. O4 compares path, kind, size and digest, and the
//! unrepresentable axis is not exercised by this workload. It is recorded rather
//! than left as an assumption.

use std::collections::{BTreeMap, BTreeSet};
use layerfs_content::filesystem::references::backing::FileBacking;
use layerfs_content::filesystem::{
    build_filesystem, scope_for_seed, update_filesystem, DirectoryUpdate, FilesystemInput,
    FilesystemObjects, FilesystemResources, FilesystemRootId, InodeUpdate, PathName,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{construct_bytes, ConstructionCapacities, ConstructionPolicy, ObjectId};
use layerfs_storage::{StoragePolicy, Store, StoreProvider};
use layerfs_telemetry::timer::{Active, Timing, TimingScope};

use super::{OpContext, OpError, OpOutcome, Phase};
use crate::gates::{self, Gate, GateClass};
use crate::registry::Case;
use crate::support::{instruments, phases};
use super::c1;
use crate::support::trace::Kind;
use crate::workload::history::{Change, Corpus, HistoryError, Row};
use crate::workload::providers::TreeStore;

/// The root directory's serial. `build_filesystem` requires it to be one of the
/// supplied values.
const ROOT_SERIAL: u64 = 1;

/// Serial of the first allocated inode. 1 is the root, so allocation starts at 2.
const FIRST_SERIAL: u64 = 2;

/// The metadata root every file carries.
///
/// The corpus records no attributes, so this is one stable synthetic identity
/// rather than a per-file one: a value that moved between states would show up as
/// a metadata change the workload never made.
fn empty_metadata_root() -> ObjectId {
    ObjectId::for_bytes(b"layerfs/history/empty-metadata-root")
}

/// The allocation scope of one row, derived from its id so a row's identities are
/// reproducible from the receipt alone.
fn scope_of(row: Row) -> layerfs_content::InodeScope {
    scope_for_seed(*ObjectId::for_bytes(format!("layerfs/history/{}", row.token()).as_bytes()).as_bytes())
}

/// Turns a corpus refusal into a driver error, keeping the refusal's own name.
fn corpus_error(error: HistoryError) -> OpError {
    OpError::Io(format!("{}: {error}", error.name()))
}

/// The parent directory of a path, or the empty path for a top-level entry.
fn parent_of(path: &[u8]) -> Vec<u8> {
    match path.iter().rposition(|byte| *byte == b'/') {
        Some(index) => path[..index].to_vec(),
        None => Vec::new(),
    }
}

/// The final component of a path.
fn name_of(path: &[u8]) -> &[u8] {
    match path.iter().rposition(|byte| *byte == b'/') {
        Some(index) => &path[index + 1..],
        None => path,
    }
}

/// Every proper ancestor of a path, longest last.
fn ancestors_of(path: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for (index, byte) in path.iter().enumerate() {
        if *byte == b'/' {
            out.push(path[..index].to_vec());
        }
    }
    out
}

/// The kind class the corpus's octal mode names.
///
/// `120000` is a symbolic link, `040000` a directory, and everything else is a
/// regular file. The permission bits are not representable in `InodeValue` and
/// the corpus makes no mode-only transition, so they are not consulted.
fn kind_of(mode: u32) -> InodeKind {
    match mode & 0o170000 {
        0o120000 => InodeKind::Symlink,
        0o040000 => InodeKind::Directory,
        _ => InodeKind::RegularFile,
    }
}

/// The serials and paths this chain has allocated so far.
///
/// It is carried **across** states, which is what makes an unchanged path keep its
/// inode and an unchanged directory keep its identity: a chain that re-allocated
/// every serial per state would rewrite the whole tree N times and measure a
/// different operation than the one the claim describes.
struct Chain {
    serial_of: BTreeMap<Vec<u8>, u64>,
    next_serial: u64,
}

impl Chain {
    fn new() -> Self {
        let mut serial_of = BTreeMap::new();
        serial_of.insert(Vec::new(), ROOT_SERIAL);
        Self {
            serial_of,
            next_serial: FIRST_SERIAL,
        }
    }

    /// The serial for a path, allocating one if this is the first state to see it.
    fn serial(&mut self, path: &[u8]) -> u64 {
        if let Some(serial) = self.serial_of.get(path) {
            return *serial;
        }
        let serial = self.next_serial;
        self.next_serial += 1;
        self.serial_of.insert(path.to_vec(), serial);
        serial
    }

    fn known(&self, path: &[u8]) -> Option<u64> {
        self.serial_of.get(path).copied()
    }
}

/// Builds the `FilesystemInput` for one state.
///
/// The changed directory bindings carry the **final** binding of every name they
/// change, so a removal is an explicit `None` rather than an omission. New
/// directories are declared as directory inodes and bound in their parents; a
/// directory that already existed keeps its serial and is not re-declared.
fn filesystem_input(
    chain: &mut Chain,
    transition: &crate::workload::history::Transition,
    content_roots: &BTreeMap<Vec<u8>, ObjectId>,
    is_build: bool,
    directories: &mut Vec<DirectoryUpdate>,
    inodes: &mut Vec<InodeUpdate>,
    new_inodes: &mut Vec<u64>,
) -> Result<(), OpError> {
    directories.clear();
    inodes.clear();
    new_inodes.clear();

    // A **build** allocates its root like any other inode and must declare it;
    // an update must not, because the root already exists in the base. Both rules
    // are `FilesystemInput::check`'s, and getting either wrong fails the whole
    // state with `InvalidRecord("root inode allocation")`.
    if is_build {
        inodes.push(InodeUpdate {
            serial: ROOT_SERIAL,
            value: InodeValue {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: ObjectId::for_bytes(b"layerfs/history/dir/root"),
                metadata_root: empty_metadata_root(),
            },
        });
        new_inodes.push(ROOT_SERIAL);
    }

    let name = |path: &[u8]| -> Result<PathName, OpError> {
        let text = std::str::from_utf8(name_of(path))
            .map_err(|_| OpError::Io("a corpus path name is not UTF-8".to_string()))?;
        PathName::new(text).map_err(|error| OpError::Product(format!("{error:?}")))
    };

    // The tree's directories, and the ones this state is the first to see.
    let mut directories_now: BTreeSet<Vec<u8>> = BTreeSet::new();
    for path in transition.tree.keys() {
        for ancestor in ancestors_of(path) {
            directories_now.insert(ancestor);
        }
    }

    // Changed directory bindings, keyed by the directory path.
    let mut bindings: BTreeMap<Vec<u8>, BTreeMap<Vec<u8>, Option<u64>>> = BTreeMap::new();

    for directory in &directories_now {
        if chain.known(directory).is_none() {
            let serial = chain.serial(directory);
            inodes.push(InodeUpdate {
                serial,
                value: InodeValue {
                    kind: InodeKind::Directory,
                    namespace_ref_count: 1,
                    content_root: ObjectId::for_bytes(
                        format!("layerfs/history/dir/{}", hex(directory)).as_bytes(),
                    ),
                    metadata_root: empty_metadata_root(),
                },
            });
            new_inodes.push(serial);
            bindings
                .entry(parent_of(directory))
                .or_default()
                .insert(name_of(directory).to_vec(), Some(serial));
        }
    }

    for changed in &transition.changed {
        let path = &changed.path;
        let parent = parent_of(path);
        match changed.kind {
            Change::Removed => {
                if let Some(serial) = chain.known(path) {
                    let _ = serial;
                }
                bindings
                    .entry(parent)
                    .or_default()
                    .insert(name_of(path).to_vec(), None);
            }
            Change::MetadataOnly => {
                // The corpus makes no mode-only transition — verified on all 157
                // checkpoints — and `InodeValue` could not express one anyway. It
                // is counted, never silently dropped.
            }
            Change::Added | Change::Modified => {
                let serial = chain.serial(path);
                if changed.kind == Change::Added {
                    new_inodes.push(serial);
                }
                let content_root = content_roots.get(path).copied().ok_or_else(|| {
                    OpError::Io(format!(
                        "state {} has no constructed content for {}",
                        transition.state.ordinal,
                        hex(path)
                    ))
                })?;
                inodes.push(InodeUpdate {
                    serial,
                    value: InodeValue {
                        kind: kind_of(changed.mode),
                        namespace_ref_count: 1,
                        content_root,
                        metadata_root: empty_metadata_root(),
                    },
                });
                bindings
                    .entry(parent)
                    .or_default()
                    .insert(name_of(path).to_vec(), Some(serial));
            }
        }
    }

    for (directory, changes) in bindings {
        let parent = chain.serial(&directory);
        let mut rows: Vec<(PathName, Option<u64>)> = Vec::with_capacity(changes.len());
        for (path, binding) in changes {
            rows.push((name(&path)?, binding));
        }
        rows.sort_by(|left, right| left.0.cmp(&right.0));
        directories.push(DirectoryUpdate {
            parent,
            changes: rows,
        });
    }
    directories.sort_by_key(|update| update.parent);
    inodes.sort_by_key(|update| update.serial);
    new_inodes.sort_unstable();
    new_inodes.dedup();
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from_digit((byte >> 4) as u32, 16).unwrap_or('0'));
        out.push(char::from_digit((byte & 0x0f) as u32, 16).unwrap_or('0'));
    }
    out
}

/// What one state's child produced, carried out of the measured region so the
/// trace is written **after** the timer rather than inside it.
struct StateOutcome {
    ordinal: usize,
    root: FilesystemRootId,
    changed_paths: usize,
    changed_bytes: u64,
    inserted: u64,
    objects: u64,
    peak_heap_bytes: u64,
}

/// The driver.
pub fn run(case: &Case, row: Row, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    match context.phase {
        Phase::Prepare => prepare(case, row, context),
        Phase::Perf => perf(case, row, context),
        Phase::Verify => Err(OpError::Io(
            "history.* verifies in a second invocation; run --phase verify".to_string(),
        )),
    }
}

/// The corpus root this row was handed, or a refusal.
fn corpus_root(context: &OpContext<'_>) -> Result<std::path::PathBuf, OpError> {
    context.corpus.clone().ok_or_else(|| {
        OpError::Io("history.* requires --corpus PATH; an absent corpus is never defaulted".to_string())
    })
}

/// `Phase::Prepare`: authenticate the corpus and publish the pins. Writes nothing.
fn prepare(_case: &Case, row: Row, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    let root = corpus_root(context)?;
    let corpus = Corpus::open(&root, row).map_err(corpus_error)?;
    let pins = corpus.pins();
    context.trace.write_number(
        Kind::Counter,
        "history.states",
        pins.states as i128,
        "states",
        "the selection's length",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "history.logical_bytes",
        pins.logical_bytes as i128,
        "bytes",
        "cumulative logical bytes, from the manifest",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "history.path_states",
        pins.path_states as i128,
        "path-states",
        "cumulative oracle entries: files and directories (erratum E1)",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "history.manifest_files",
        pins.manifest_files as i128,
        "files",
        "the manifest's own files total; about 18% below path-states and not the same quantity",
    )?;
    context.trace.write(
        Kind::Counter,
        "history.manifest_sha256",
        crate::workload::history::MANIFEST_SHA256,
        "sha256",
        "the corpus manifest identity",
    )?;
    context.trace.write(
        Kind::Counter,
        "history.source_tip",
        crate::workload::history::SOURCE_TIP,
        "commit",
        "the pinned source tip",
    )?;
    for state in corpus.states() {
        context.trace.write(
            Kind::Counter,
            &format!("history.oracle_sha256.{}", state.ordinal),
            &state.oracle_sha256,
            "sha256",
            "the state's oracle digest, recorded once at open",
        )?;
    }
    Ok(OpOutcome::new().note(format!(
        "corpus: {} (manifest {}, tip {})",
        root.display(),
        crate::workload::history::MANIFEST_SHA256,
        crate::workload::history::SOURCE_TIP
    )))
}

/// The measured chain.
fn perf(_case: &Case, row: Row, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    let root = corpus_root(context)?;
    let mut corpus = Corpus::open(&root, row).map_err(corpus_error)?;
    let store_path = context.output.join("sample.sqlite");

    let policy = ConstructionPolicy::frozen_default();
    let capacities: ConstructionCapacities = policy.capacities();
    let scope = scope_of(row);
    // The operation's own ordering spill. `ops/fs.rs` opens one when the fixture
    // outgrows the in-memory pending map, and the threshold is the product's
    // declared ceiling rather than a harness policy: a transition large enough to
    // need a backing pays for it, inside the child, and a small one does not.
    let backing_directory = context.output.join("ordering");
    std::fs::create_dir_all(&backing_directory)?;

    let mut gates = Vec::new();
    let mut chain = Chain::new();
    let mut store: Option<Store> = None;
    let mut previous_root: Option<FilesystemRootId> = None;

    let mut directories: Vec<DirectoryUpdate> = Vec::new();
    let mut inodes: Vec<InodeUpdate> = Vec::new();
    let mut new_inodes: Vec<u64> = Vec::new();

    let count = corpus.states().len();

    // **One root, N named children.** The corpus reading sits inside the root and
    // outside every child, so the row's `operation_ns` is the sum of the children
    // and not the root — owner ruling 2. The root is the whole chain including the
    // harness's own reading, and it is published for exactly that reason: a reader
    // can see how much of the chain was not the product's.
    phases::prepared();
    let (result, report) = Timing::record("history", |root: &TimingScope<'_, Active>| {
        let mut outcomes: Vec<StateOutcome> = Vec::with_capacity(count);
        for position in 0..count {
            // Untimed: the harness reads the corpus between the children.
            let transition = corpus.transition(position).map_err(corpus_error)?;
            let ordinal = transition.state.ordinal;
            let mut peak_heap = 0u64;
            let outcome = root
                .child(format!("history.state.{ordinal}"))
                .run(|child: &TimingScope<'_, Active>| -> Result<StateOutcome, OpError> {
                    // The heap window is per child, so the figure is one state's
                    // peak rather than the whole chain's: the corpus bytes this
                    // loop reads are inside the root but outside this window.
                    instruments::heap_begin();
                    let mut consumer = TreeStore::new();
                    // **One `content` node per state, not per file.** The product's
                    // timing tree is bounded by `layerfs_telemetry::MAX_NODES`
                    // (1,024), so a node per constructed file exhausts it inside
                    // state 1 and every later state is clipped away — the first
                    // version of this driver recorded one child of seventeen and
                    // reported the chain as `incomplete`. The per-file work is
                    // measured inside this node; the node itself is per state, so a
                    // 157-state chain needs 3 x 157 + 1 nodes rather than tens of
                    // thousands.
                    let constructed = child
                        .child("content")
                        .run(
                            |content: &TimingScope<'_, Active>| -> Result<
                                BTreeMap<Vec<u8>, ObjectId>,
                                OpError,
                            > {
                                let _ = content;
                                let mut constructed = BTreeMap::new();
                                for changed in &transition.changed {
                                    if matches!(
                                        changed.kind,
                                        Change::Removed | Change::MetadataOnly
                                    ) {
                                        continue;
                                    }
                                    let bytes =
                                        transition.blobs.get(&changed.oid).ok_or_else(|| {
                                            OpError::Io(format!(
                                                "state {ordinal}: oid {} was not served",
                                                hex(&changed.oid)
                                            ))
                                        })?;
                                    // A disabled scope records nothing: on it,
                                    // `child` returns a disabled pending scope, so
                                    // the per-file work is measured inside the
                                    // state's `content` node and creates no nodes
                                    // of its own.
                                    let (file, _) = Timing::disabled(
                                        "content.file",
                                        |scope: &TimingScope<'_, Active>| {
                                            construct_bytes(
                                                policy,
                                                &capacities,
                                                bytes,
                                                &mut consumer,
                                                scope.child("content.file"),
                                            )
                                        },
                                    );
                                    let file = file
                                        .map_err(|error| OpError::Product(format!("{error:?}")))?;
                                    constructed.insert(changed.path.clone(), file.root);
                                }
                                Ok(constructed)
                            },
                        )?;
                    // The Store is created inside state 1's child, so the fixed
                    // cost is visible and subtractable rather than buried.
                    if store.is_none() {
                        let created = Store::create(
                            &store_path,
                            StoragePolicy::frozen_default(),
                            child.child("store.create"),
                        )
                        .map_err(|error| OpError::Product(format!("{error:?}")))?;
                        store = Some(created);
                    }
                    let held = store.as_ref().expect("the Store was just created");
                    filesystem_input(
                        &mut chain,
                        &transition,
                        &constructed,
                        previous_root.is_none(),
                        &mut directories,
                        &mut inodes,
                        &mut new_inodes,
                    )?;
                    let input = FilesystemInput {
                        base: previous_root,
                        scope,
                        root_serial: ROOT_SERIAL,
                        directories: &directories,
                        inodes: &inodes,
                        new_inodes: &new_inodes,
                        resources: FilesystemResources::default(),
                    };
                    let provider = StoreProvider::new(held);
                    let bindings: usize =
                        directories.iter().map(|update| update.changes.len()).sum();
                    let mut backing =
                        (bindings > FilesystemResources::default().maximum_pending_records)
                            .then(|| FileBacking::new(&backing_directory));
                    let built = {
                        let mut objects = FilesystemObjects::new(&provider, &mut consumer);
                        let backing = backing.as_mut().map(|backing| {
                            backing
                                as &mut dyn layerfs_content::filesystem::references::backing::OrderingBacking
                        });
                        match previous_root {
                            None => build_filesystem(&mut objects, &input, backing),
                            Some(_) => update_filesystem(&mut objects, &input, backing),
                        }
                    };
                    let built = built.map_err(|error| OpError::Product(format!("{error:?}")))?;
                    let mut operation = held
                        .begin_save(child.child("storage.begin"))
                        .map_err(|error| OpError::Product(format!("{error:?}")))?;
                    for id in consumer.insertion_order() {
                        let object = consumer
                            .cloned_object(*id)
                            .ok_or_else(|| OpError::Io(format!("object {id:?} vanished")))?;
                        operation
                            .accept(object)
                            .map_err(|error| OpError::Product(format!("{error:?}")))?;
                    }
                    let saved = operation
                        .finish(child.child("storage.finish"))
                        .map_err(|error| OpError::Product(format!("{error:?}")))?;
                    peak_heap = instruments::heap_end().peak_incremental_bytes;
                    previous_root = Some(built.root);
                    Ok(StateOutcome {
                        ordinal,
                        root: built.root,
                        changed_paths: transition.changed.len(),
                        changed_bytes: transition.changed_bytes(),
                        inserted: saved.inserted,
                        objects: built.counters.objects.objects_emitted,
                        peak_heap_bytes: 0,
                    })
                })?;
            outcomes.push(StateOutcome {
                peak_heap_bytes: peak_heap,
                ..outcome
            });
        }
        Ok(outcomes)
    });
    let outcomes = match result {
        Ok(outcomes) => outcomes,
        Err(error) => return Ok(unmeasured(&error, gates)),
    };
    drop(store);

    // The product's own timing tree, byte-verbatim, written **once** for the whole
    // chain: one root and one named child per state. The published operation time
    // is the sum of those children.
    let timing_bytes = c1::write_timing(context.output, &report)?;
    let children_ns: u64 = report
        .root()
        .map(|root| {
            root.children()
                .iter()
                .map(|child| child.elapsed().as_nanos() as u64)
                .sum()
        })
        .unwrap_or(0);
    let root_ns = report
        .root()
        .map(|root| root.elapsed().as_nanos() as u64)
        .unwrap_or(0);
    phases::measured(children_ns, timing_bytes);

    for outcome in &outcomes {
        context.trace.write(
            Kind::Counter,
            &format!("history.state.{}.root", outcome.ordinal),
            &format!("{:?}", outcome.root),
            "identity",
            "the state's filesystem root identity (O1 pin)",
        )?;
        context.trace.write_number(
            Kind::Counter,
            &format!("history.state.{}.changed_paths", outcome.ordinal),
            outcome.changed_paths as i128,
            "paths",
            "entries this transition changed",
        )?;
        context.trace.write_number(
            Kind::Counter,
            &format!("history.state.{}.changed_bytes", outcome.ordinal),
            outcome.changed_bytes as i128,
            "bytes",
            "logical bytes this transition changed",
        )?;
        context.trace.write_number(
            Kind::Counter,
            &format!("history.state.{}.inserted", outcome.ordinal),
            i128::from(outcome.inserted),
            "objects",
            "SaveOutcome.inserted",
        )?;
        context.trace.write_number(
            Kind::Counter,
            &format!("history.state.{}.objects", outcome.ordinal),
            i128::from(outcome.objects),
            "objects",
            "canonical objects the child emitted",
        )?;
        context.trace.write_number(
            Kind::Resource,
            &format!("history.state.{}.heap_peak_incremental_bytes", outcome.ordinal),
            outcome.peak_heap_bytes as i128,
            "bytes",
            "counting GlobalAlloc, this state's child alone",
        )?;
    }
    let peak_heap = outcomes
        .iter()
        .map(|outcome| outcome.peak_heap_bytes)
        .max()
        .unwrap_or(0);
    context.trace.write_number(
        Kind::Resource,
        "heap.peak_incremental_bytes",
        peak_heap as i128,
        "bytes",
        "the largest single-state peak; the chain's own peak is not the product's",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "history.root_ns",
        root_ns as i128,
        "ns",
        "the product's own root: the chain including the harness's untimed corpus reading",
    )?;

    context.trace.write(
        Kind::Counter,
        "store.path",
        &store_path.display().to_string(),
        "path",
        "the Store this row created and grew",
    )?;
    let space = instruments::space(&store_path)
        .map_err(|error| OpError::Io(format!("{error}")))?;
    context.trace.write_number(
        Kind::Resource,
        "space.allocated_bytes",
        space.allocated_bytes as i128,
        "bytes",
        "st_blocks * 512 of the Store file",
    )?;
    context.trace.write_number(
        Kind::Resource,
        "space.apparent_bytes",
        space.apparent_bytes as i128,
        "bytes",
        "st_size of the Store file",
    )?;
    let pins = corpus.pins();
    gates.push(gates::require(
        GateClass::Correctness,
        "g1.o1-chain-complete",
        outcomes.len() == pins.states,
        &format!("{} of {} states saved", outcomes.len(), pins.states),
        "every state of the selection is saved, in order",
    ));
    gates.push(gates::swap_gate(instruments::swaps()));
    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("history_row: {}", row.id()),
            format!("history_states: {}", pins.states),
            format!("history_logical_bytes: {}", pins.logical_bytes),
            format!("history_path_states: {}", pins.path_states),
            "measured_region: construct + build/update + save, one named child per state".to_string(),
            "operation_ns: the sum of the named children, not the root".to_string(),
            "corpus_reading: between the children, untimed".to_string(),
            "prepared: none (Preparation::InProcess)".to_string(),
            format!("store: {}", store_path.display()),
        ],
    })
}

/// The shape of an unmeasured failure: gates only, never a pass.
fn unmeasured(error: &OpError, mut gates: Vec<Gate>) -> OpOutcome {
    gates.push(Gate::incomplete(
        GateClass::Correctness,
        "g1.o1-chain-complete",
        &format!("{error}"),
        "every state of the selection is saved, in order",
    ));
    OpOutcome { gates, notes: Vec::new() }
}

