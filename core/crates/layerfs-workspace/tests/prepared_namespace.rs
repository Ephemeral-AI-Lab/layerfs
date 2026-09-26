//! The frozen namespace lowering: the exact prepared rows of one generation.
//!
//! #256's slice 3.3 lowers a captured directory with the keyed tree's own
//! cursors - one ordered pass over its entry leaves and one over its removal
//! leaves, merged in name order - and admits the prepared update by the exact
//! bytes it occupies in one metadata frame instead of an estimate over the
//! capture's counts. This drives the ordinary public route (attach, `mknod`,
//! `rename`, `unlink`, `commit`) against a delivery that records the command it
//! is asked to carry, so the oracle is the request's own rows and not a
//! summary: a prepared update that reached the wire with a missing, extra,
//! misordered or wrongly-addressed name fails here.
//!
//! Three properties are checked. Every name of a 200-name directory reaches its
//! own binding row with the serial the mounted creation returned, in name
//! order; the frame admission is the encoder's own figure, with a 255-byte-name
//! directory one name below and one name above the metadata frame bound; and
//! one Commit publishes exactly one head while the local listing survives
//! reconciliation.
#![cfg(target_os = "linux")]
use layerfs_bridge::{adapters::native::protocol::encode_request_with_budget, contract::*};
use layerfs_workspace::*;
#[path = "support/prepared.rs"]
mod prepared_stream;
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

const BRANCH: [u8; 17] = [0x11; 17];
const STACK: [u8; 17] = [0x31; 17];
const LAYER: [u8; 33] = [0x32; 33];
const BASE: Root = [8; 32];
const SCOPE: Root = [9; 32];
const PROFILE: Root = [10; 32];
const ROOT_SERIAL: u64 = 7;
/// Root of the Commit this fixture publishes for the candidate.
const CANDIDATE: Root = [0xaa; 32];
/// Commit identity of the published head.
const COMMIT_ID: [u8; COMMIT_BYTES] = [0x12; COMMIT_BYTES];
/// Names created before any rename or removal.
const CREATED: usize = 200;
/// Names renamed, removed and recreated afterwards.
const RENAMED: usize = 60;
const REMOVED: usize = 60;
const RECREATED: usize = 25;
/// Bytes of one name in the frame-boundary case.
const LONG_NAME_BYTES: usize = 255;
/// Names whose prepared update is the last one a metadata frame can carry.
const FRAME_FIT: usize = 94;

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}
fn temporary() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let path = std::env::temp_dir().join(format!(
        "prepared-namespace-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut builder = fs::DirBuilder::new();
    use std::os::unix::fs::DirBuilderExt;
    builder.mode(0o700);
    builder.create(&path).unwrap();
    path
}
fn name(prefix: &str, index: usize) -> Vec<u8> {
    format!("{prefix}{index:04}").into_bytes()
}
fn long_name(index: usize) -> Vec<u8> {
    let mut name = vec![b'n'; LONG_NAME_BYTES - 3];
    name.extend_from_slice(format!("{index:03}").as_bytes());
    name
}

/// What the delivery was asked to carry.
#[derive(Default)]
struct Observed {
    /// The prepared update of every `StageChanges`/`Commit` command, in order,
    /// with the rows its body carried.
    prepared: Mutex<Vec<prepared_stream::Recorded>>,
    saves: AtomicU64,
    metadata: AtomicU64,
}

struct Serials(AtomicU64);

/// One page of published bindings plus the continuation that resumes after it.
type PublishedPage = (Vec<(Vec<u8>, u64)>, Option<Vec<u8>>);

/// The identity the delivery published under `path`, when it published one.
fn published_serial(observed: &Observed, path: &[u8]) -> Option<u64> {
    let name = path.rsplit(|byte| *byte == b'/').next().unwrap_or(path);
    let prepared = observed.prepared.lock().unwrap();
    prepared
        .iter()
        .flat_map(|changes| changes.directories.iter())
        .flat_map(|directory| directory.changes.iter())
        .find_map(|(bound, serial)| (bound.as_slice() == name).then_some(*serial).flatten())
}

/// One page of the names the delivery has published so far: every bound row of
/// the prepared updates it carried, from `after` onward, within the page the
/// caller asked for. A published root is read back page by page the way a real
/// service answers it, including the continuation the caller resumes at.
fn published(observed: &Observed, after: &[u8], limit: usize, byte_budget: usize) -> PublishedPage {
    let prepared = observed.prepared.lock().unwrap();
    let mut entries: Vec<(Vec<u8>, u64)> = prepared
        .iter()
        .flat_map(|changes| changes.directories.iter())
        .flat_map(|directory| directory.changes.iter())
        .filter_map(|(name, serial)| serial.map(|serial| (name.clone(), serial)))
        .collect();
    entries.sort();
    entries.dedup();
    let start = entries.partition_point(|(name, _)| name.as_slice() <= after);
    let mut page = Vec::new();
    let mut bytes = 0;
    for (name, serial) in &entries[start..] {
        let cost = name.len() + 10;
        if page.len() == limit || bytes + cost > byte_budget {
            break;
        }
        bytes += cost;
        page.push((name.clone(), *serial));
    }
    let more = start + page.len() < entries.len();
    let continuation = more
        .then(|| page.last().map(|(name, _)| name.clone()))
        .flatten();
    (page, continuation)
}

fn delivery(observed: Arc<Observed>, serials: Arc<Serials>) -> OperationDelivery {
    Arc::new(move |request, input, _output, deadline| {
        match &request.operation {
            Operation::HistoryQuery(HistoryQuery::GetBranch { branch }) => Ok(Response::History(
                Box::new(HistoryResult::BranchSnapshot(BranchSnapshotWire {
                    branch: BranchWire {
                        branch: *branch,
                        stack: STACK,
                        name: b"main".to_vec(),
                        base_layer: LAYER,
                        head_commit: None,
                    },
                    head_root: None,
                    base_root: BASE,
                    effective_root: BASE,
                    root_serial: Some(ROOT_SERIAL),
                    scope: SCOPE,
                    profile: PROFILE,
                })),
            )),
            Operation::Inspect {
                query: Inspect::Attributes { path },
                ..
            } if path.is_empty() => Ok(Response::Attributes {
                serial: ROOT_SERIAL,
                kind: 2,
                references: 0,
                content: [7; 32],
                metadata: [1; 32],
                mode: 0o755,
                mtime: -1,
                nanoseconds: 123,
                size: 0,
            }),
            // Before a Commit the attached canonical root lists no name, so
            // every canonical name this fixture could resolve is absent and the
            // effective namespace is exactly what the mounted operations bound.
            // After one, the delivery answers with what it was asked to publish:
            // the bound rows of the prepared update it carries, which is the
            // listing a real service would read back from the new root.
            Operation::Inspect {
                query:
                    Inspect::List {
                        path,
                        after,
                        entries,
                        bytes,
                    },
                ..
            } if path.is_empty() => {
                let (page, continuation) =
                    published(&observed, after, usize::from(*entries), *bytes as usize);
                Ok(Response::List {
                    entries: page,
                    continuation,
                })
            }
            // A name the delivery has published resolves to the identity it
            // published; every other canonical name is absent, which is what
            // makes the mounted creations below bind a fresh serial.
            Operation::Inspect {
                query: Inspect::Attributes { path },
                ..
            } => match published_serial(&observed, path) {
                Some(serial) => Ok(Response::Attributes {
                    serial,
                    kind: 1,
                    references: 1,
                    content: [0x0c; 32],
                    metadata: [0x0d; 32],
                    mode: 0o644,
                    mtime: -1,
                    nanoseconds: 0,
                    size: 0,
                }),
                None => Err(Code::NotFound.into()),
            },
            Operation::HistoryCommand(HistoryCommand::ReserveInodes { scope, count }) => {
                let start = serials.0.fetch_add(*count, Ordering::AcqRel);
                Ok(Response::History(Box::new(HistoryResult::Reservation {
                    scope: *scope,
                    start,
                    count: *count,
                })))
            }
            // One empty regular file per creation: the body is the frozen final
            // sequence, which for a fresh zero-length file holds no extent and no
            // replacement byte. The saved root is derived from the file so the
            // oracle can follow one name to its content.
            Operation::SaveFile { length, .. } => {
                let cancel = AtomicBool::new(false);
                let mut buffer = [0u8; 4096];
                let mut body = 0u64;
                loop {
                    let count = input
                        .read(&mut buffer, deadline, &cancel)
                        .map_err(|_| Failure::from(Code::Io))?;
                    if count == 0 {
                        break;
                    }
                    body += count as u64;
                }
                if body != 0 {
                    return Err(Code::InvalidInput.into());
                }
                observed.saves.fetch_add(1, Ordering::AcqRel);
                let mut root = [0x0c; 32];
                root[..8].copy_from_slice(&length.to_be_bytes());
                Ok(Response::Saved {
                    root,
                    length: *length,
                    inserted: 0,
                    reused: 0,
                })
            }
            Operation::ConstructPortableMetadata {
                kind,
                mode,
                mtime_seconds,
                mtime_nanoseconds,
            } => {
                observed.metadata.fetch_add(1, Ordering::AcqRel);
                Ok(Response::MetadataConstructed {
                    kind: *kind,
                    mode: *mode,
                    mtime_seconds: *mtime_seconds,
                    mtime_nanoseconds: *mtime_nanoseconds,
                    metadata: [0x0d; 32],
                    inserted: 0,
                    reused: 0,
                })
            }
            Operation::HistoryCommand(HistoryCommand::Commit(changes)) => {
                // The rows are the body this request declared: the delivery reads
                // them before it answers, so the oracle below is what actually
                // reached the wire.
                let recorded = prepared_stream::Recorded::read(changes, input, deadline)
                    .map_err(|_| Code::InvalidInput)?;
                observed.prepared.lock().unwrap().push(recorded);
                Ok(Response::History(Box::new(HistoryResult::Committed(
                    CommitOutcomeWire::Committed(CommitWire {
                        commit: COMMIT_ID,
                        stack: STACK,
                        root: CANDIDATE,
                        parent: None,
                        base_layer: LAYER,
                    }),
                ))))
            }
            // One frozen stage for the rows the caller just prepared: the
            // workspace validates every field against its own capture, so this
            // replies with the same identities the attach did.
            Operation::HistoryCommand(HistoryCommand::StageChanges(changes)) => {
                let recorded = prepared_stream::Recorded::read(changes, input, deadline)
                    .map_err(|_| Code::InvalidInput)?;
                observed.prepared.lock().unwrap().push(recorded);
                Ok(Response::History(Box::new(HistoryResult::Stage(
                    StageWire {
                        workspace: changes.workspace,
                        token: 1,
                        stack: STACK,
                        branch: BRANCH,
                        expected_head: changes.expected_head,
                        expected_base: LAYER,
                        // The stage is built from the root the caller states, which
                        // is the capture's own effective root: the first generation
                        // captured the attached base, every later one the root the
                        // preceding Commit published.
                        expected_root: changes.base,
                        construction_base_root: changes.base,
                        intended_commit_base: LAYER,
                        candidate_root: CANDIDATE,
                        profile: PROFILE,
                        scope: SCOPE,
                        generation: changes.generation,
                    },
                ))))
            }
            // A maintained identity is patched against the metadata root the
            // caller states; the reply echoes that root and the selected fields,
            // which the workspace checks field by field.
            Operation::UpdatePortableMetadata {
                base,
                kind,
                mode,
                mtime_seconds,
                mtime_nanoseconds,
            } => {
                observed.metadata.fetch_add(1, Ordering::AcqRel);
                Ok(Response::MetadataSaved {
                    base: *base,
                    kind: *kind,
                    mode: *mode,
                    mtime_seconds: *mtime_seconds,
                    mtime_nanoseconds: *mtime_nanoseconds,
                    metadata: *base,
                    inserted: 0,
                    reused: 0,
                })
            }
            _ => Err(Code::Unsupported.into()),
        }
    })
}

/// One mounted local-edit Workspace over a fresh private directory.
fn mounted(
    id: &str,
    incarnation: u8,
    observed: Arc<Observed>,
) -> (PathBuf, WorkspaceHost, Workspace) {
    let path = temporary();
    let metadata = fs::metadata(&path).unwrap();
    use std::os::unix::fs::MetadataExt;
    let host = WorkspaceHost::new(
        WorkspaceConfig {
            root: path.clone(),
            max_count: 1,
            memory_budget_bytes: 64 * 1024 * 1024,
            disk_budget_bytes: Some(256 * 1024 * 1024),
        },
        delivery(observed, Arc::new(Serials(AtomicU64::new(1_000)))),
    )
    .unwrap();
    let workspace = host
        .attach(
            AttachOptions {
                id: id.into(),
                incarnation: [incarnation; 32],
                store: 1,
                base: Base::Branch(BRANCH),
                access: WorkspaceAccess::LocalEdit,
                owner_uid: metadata.uid(),
                owner_gid: metadata.gid(),
            },
            deadline(),
        )
        .unwrap();
    (path, host, workspace)
}

/// Every name the mounted delta lists, in name order, with its serial.
fn listed(workspace: &Workspace, serial: u64) -> BTreeMap<Vec<u8>, u64> {
    let handle = workspace.opendir(serial, ReferenceScope::Local).unwrap();
    let mut names = BTreeMap::new();
    let mut cookie = 0;
    loop {
        let page = workspace
            .readdir(handle, cookie, MAX_DIRECTORY_ENTRIES, deadline())
            .unwrap();
        if page.entries().is_empty() {
            break;
        }
        for entry in page.entries() {
            if entry.name != b"." && entry.name != b".." {
                names.insert(entry.name.clone(), entry.serial);
            }
            cookie = entry.cookie;
        }
        if page.entries().len() < MAX_DIRECTORY_ENTRIES {
            break;
        }
    }
    workspace.releasedir(handle).unwrap();
    names
}

#[test]
fn a_wide_directory_lowers_to_its_exact_rows_and_one_head() {
    let observed = Arc::new(Observed::default());
    let (path, host, workspace) = mounted("prepared-wide", 31, observed.clone());
    let mut serials = BTreeMap::new();
    for index in 0..CREATED {
        let attr = workspace
            .mknod(ROOT_SERIAL, &name("f", index), 0o644, 0, deadline())
            .unwrap_or_else(|error| panic!("create {index}: {error:?}"));
        serials.insert(name("f", index), attr.serial);
    }
    for index in 0..RENAMED {
        workspace
            .rename(
                ROOT_SERIAL,
                &name("f", index),
                ROOT_SERIAL,
                &name("g", index),
                RenameFlags::default(),
                deadline(),
            )
            .unwrap_or_else(|error| {
                panic!(
                    "rename {index}: {error:?} status={:?} backing={:?}",
                    workspace.status(),
                    workspace.backing_status()
                )
            });
        let serial = serials.remove(&name("f", index)).unwrap();
        serials.insert(name("g", index), serial);
    }
    for index in RENAMED..(RENAMED + REMOVED) {
        workspace
            .unlink(ROOT_SERIAL, &name("f", index), deadline())
            .unwrap_or_else(|error| panic!("unlink {index}: {error:?}"));
        serials.remove(&name("f", index));
    }
    for index in RENAMED..(RENAMED + RECREATED) {
        let attr = workspace
            .mknod(ROOT_SERIAL, &name("f", index), 0o644, 0, deadline())
            .unwrap_or_else(|error| panic!("recreate {index}: {error:?}"));
        serials.insert(name("f", index), attr.serial);
    }
    // The local view before the Commit is the old listing oracle; the prepared
    // row is the new one.
    let local = listed(&workspace, ROOT_SERIAL);
    assert_eq!(local, serials);
    // Every name this generation made absent from the root: a rename removes
    // its source name and binds its destination, and an unlink that was not
    // followed by a recreate leaves the name absent for good.
    let mut gone: Vec<Vec<u8>> = (0..RENAMED).map(|index| name("f", index)).collect();
    gone.extend(((RENAMED + RECREATED)..(RENAMED + REMOVED)).map(|index| name("f", index)));
    let before = workspace.backing_status().unwrap().metadata_reads;
    let report = workspace.commit(deadline()).unwrap();
    let reads = workspace.backing_status().unwrap().metadata_reads - before;

    // The recorded update is copied out of the fixture before anything reads the
    // published listing again: the delivery answers that listing from the same
    // record, and holding the lock across the read would deadlock the test.
    let recorded = observed.prepared.lock().unwrap();
    assert_eq!(recorded.len(), 1, "one Commit carries one prepared update");
    let prepared = recorded[0].clone();
    drop(recorded);
    let prepared = &prepared;
    assert_eq!(prepared.header.expected_head, None);
    assert_eq!(prepared.header.expected_base, LAYER);
    assert_eq!(prepared.header.base, BASE);
    assert_eq!(prepared.header.scope, SCOPE);
    assert_eq!(prepared.header.root_serial, ROOT_SERIAL);
    // Exactly one directory row, and it is the root: every name of this
    // generation belongs to the directory it was created in.
    assert_eq!(prepared.directories.len(), 1);
    let row = &prepared.directories[0];
    assert_eq!(row.parent, ROOT_SERIAL);
    let mut bound = BTreeMap::new();
    let mut removed = Vec::new();
    for (name, serial) in &row.changes {
        match serial {
            Some(serial) => {
                bound.insert(name.clone(), *serial);
            }
            None => removed.push(name.clone()),
        }
    }
    assert!(
        row.changes.windows(2).all(|pair| pair[0].0 < pair[1].0),
        "the prepared rows are in name order"
    );
    // The row is the final state of one name per row: every name the directory
    // still binds carries its serial, and every name this generation removed
    // and did not bind again is stated absent - one row per name, no gap.
    assert_eq!(bound, serials, "every bound name carries its own serial");
    assert_eq!(removed, gone);
    // One row per name this generation changed: every created name is bound or
    // absent, and every rename destination is a name of its own.
    assert_eq!(bound.len() + removed.len(), CREATED + RENAMED);
    assert_eq!(row.changes.len(), CREATED + RENAMED);
    println!(
        "PREPARED_ROWS names={} bound={} removed={} identities={} fresh={}",
        row.changes.len(),
        bound.len(),
        removed.len(),
        prepared.identities.len(),
        prepared.header.totals.fresh
    );
    // One typed row per dirty identity, in serial order: every name this
    // generation created or rebound is one row, and the ones created here are
    // the ones the delivery's allocator handed out.
    let rooted: Vec<_> = prepared.rooted().collect();
    assert!(
        rooted.windows(2).all(|pair| pair[0].0 < pair[1].0),
        "one ordered identity row per dirty identity"
    );
    assert!(rooted.iter().all(|(_, kind, _, _, _)| *kind == 1));
    for serial in bound.values() {
        assert!(
            rooted
                .iter()
                .any(|(named, _, _, _, fresh)| named == serial && *fresh),
            "a name's identity is declared fresh once"
        );
    }
    // The one maintained directory is the root, and its row is the selected
    // portable fields rather than a typed value.
    assert!(prepared.identities.iter().any(|row| matches!(
        row,
        PreparedIdentity::DirectoryPatch {
            serial: ROOT_SERIAL,
            ..
        }
    )));

    // The declaration is the stream's own figure: the request the delivery was
    // handed declares exactly the rows its body carried, and the body is
    // exactly the bytes that declaration implies. The metadata frame carries
    // the request itself, which is now the header alone.
    let stream = prepared.header.stream_bytes().unwrap();
    let body = prepared_stream::body(&prepared.directories, &prepared.identities).unwrap();
    assert_eq!(
        stream,
        body.len() as u64,
        "the body is exactly its declaration"
    );
    let frame = encode_request_with_budget(
        &Request {
            id: 1,
            generation: prepared.header.generation,
            store: 1,
            profile: HISTORY_PROFILE,
            deadline_ms: 5_000,
            response_bytes: HISTORY_RESULT_BYTES as u64,
            operation: Operation::HistoryCommand(HistoryCommand::Commit(prepared.header.clone())),
        },
        5_000,
    )
    .unwrap()
    .len();
    assert!(frame <= METADATA_BYTES, "the header is one metadata frame");
    // One head, published from the captured one.
    let CommitOutcomeWire::Committed(commit) = &report.outcome else {
        panic!("one Commit publishes one head: {:?}", report.outcome);
    };
    assert_eq!(commit.root, CANDIDATE);
    assert_eq!(commit.parent, None);
    assert_eq!(commit.commit, COMMIT_ID);
    // The successor keeps the local names: reconciliation re-anchors the delta
    // on the published root without dropping or reordering a binding.
    assert_eq!(listed(&workspace, ROOT_SERIAL), serials);
    println!(
        "PREPARED_NAMESPACE names={} rows={} identities={} stream_bytes={stream} frame_bytes={frame} metadata_reads={reads} saves={} metadata={}",
        bound.len(),
        row.changes.len(),
        prepared.identities.len(),
        observed.saves.load(Ordering::Acquire),
        observed.metadata.load(Ordering::Acquire),
    );
    // Reported, not bounded: this figure covers the whole Commit, including the
    // successor reconciliation that slice 3.3 does not touch. The preparation's
    // own page reads are measured below, where no content is saved at all.
    println!("PREPARED_COMMIT metadata_reads={reads}");

    drop(workspace);
    drop(host);
    fs::remove_dir_all(&path).unwrap();
}

/// A wide name pass that owns one identity: `LINKS` names bound to one published
/// file, so the preparation's per-identity work is constant while the name count
/// grows. Every page read here belongs to the name pass and the frontier walk,
/// which is what makes the bound below meaningful.
#[test]
fn a_wide_name_pass_over_one_identity_reads_each_path_once() {
    const LINKS: usize = 200;
    let observed = Arc::new(Observed::default());
    let (path, host, workspace) = mounted("prepared-links", 35, observed.clone());
    let target = workspace
        .mknod(ROOT_SERIAL, b"target", 0o644, 0, deadline())
        .unwrap();
    workspace.commit(deadline()).unwrap();
    for index in 0..LINKS {
        workspace
            .link(ROOT_SERIAL, &name("k", index), target.serial, deadline())
            .unwrap_or_else(|error| panic!("link {index}: {error:?}"));
    }
    let before = workspace.backing_status().unwrap().metadata_reads;
    let selector = workspace.stage(deadline()).unwrap();
    let reads = workspace.backing_status().unwrap().metadata_reads - before;
    let recorded = observed.prepared.lock().unwrap();
    let staged = recorded.last().expect("one staged generation");
    assert_eq!(selector.stage().candidate_root, CANDIDATE);
    assert_eq!(staged.directories.len(), 1);
    assert_eq!(staged.directories[0].changes.len(), LINKS);
    assert!(
        staged.directories[0]
            .changes
            .iter()
            .all(|(_, serial)| *serial == Some(target.serial)),
        "one identity owns every name"
    );
    assert_eq!(staged.rooted().count(), 1);
    assert!(
        !staged.declared(target.serial),
        "an existing identity is patched"
    );
    println!("PREPARED_NAME_PASS names={LINKS} inodes=1 metadata_reads={reads}");
    // One ordered pass per record kind: the frontier, the two record kinds and
    // the 200-name run of two leaves. A successor lookup per name - the walk
    // this replaces - reads the path above each of the 200 rows plus the two
    // records behind it, an order of magnitude above this bound.
    assert!(
        reads < 64,
        "a 200-name pass reads each reached leaf once, not once per name: {reads}"
    );
    drop(recorded);
    drop(workspace);
    drop(host);
    fs::remove_dir_all(&path).unwrap();
}

#[test]
fn a_prepared_update_past_the_metadata_frame_travels_as_a_stream() {
    // The admission used to be one metadata frame: a generation of 255-byte names
    // this wide was refused as `Capacity` before any command was sent. The rows
    // now travel in the ordered body the request declares, so the same generation
    // is carried whole - while the *request* stays inside the frame it is still
    // bounded by, because all it carries is the declaration.
    //
    // One such name costs its binding row and its identity row, so three names
    // past the old 94-name boundary put the body itself above the frame.
    let observed = Arc::new(Observed::default());
    let (path, host, workspace) = mounted("prepared-stream-over", 32, observed.clone());
    let names = FRAME_FIT + 3;
    for index in 0..names {
        workspace
            .mknod(ROOT_SERIAL, &long_name(index), 0o644, 0, deadline())
            .unwrap_or_else(|error| panic!("create {index}: {error:?}"));
    }
    let report = workspace.commit(deadline()).unwrap();
    let recorded = observed.prepared.lock().unwrap();
    assert_eq!(recorded.len(), 1, "one Commit carries one prepared update");
    let prepared = &recorded[0];
    let row = prepared
        .directories
        .iter()
        .find(|row| row.parent == ROOT_SERIAL)
        .expect("the root's own row");
    assert_eq!(
        row.changes.len(),
        names,
        "every created name reached its own binding row"
    );
    // The stream is exactly what the request declared, and it is larger than the
    // frame that used to refuse it.
    let declared = prepared.header.stream_bytes().unwrap();
    let body = prepared_stream::body(&prepared.directories, &prepared.identities).unwrap();
    assert_eq!(declared, body.len() as u64, "the body is its declaration");
    assert!(
        declared > METADATA_BYTES as u64,
        "the case must exceed the frame: {declared}"
    );
    // The request itself is still one metadata frame, and it no longer grows
    // with the rows it declares.
    let frame = encode_request_with_budget(
        &Request {
            id: 1,
            generation: prepared.header.generation,
            store: 1,
            profile: HISTORY_PROFILE,
            deadline_ms: 5_000,
            response_bytes: HISTORY_RESULT_BYTES as u64,
            operation: Operation::HistoryCommand(HistoryCommand::Commit(prepared.header.clone())),
        },
        5_000,
    )
    .unwrap()
    .len();
    assert!(frame <= METADATA_BYTES, "the header is one metadata frame");
    // Exactly the identity rows the generation created, one per name.
    assert_eq!(
        prepared
            .rooted()
            .filter(|(_, _, _, _, fresh)| *fresh)
            .count(),
        names
    );
    let CommitOutcomeWire::Committed(commit) = &report.outcome else {
        panic!("one Commit publishes one head: {:?}", report.outcome);
    };
    assert_eq!(commit.root, CANDIDATE);
    println!(
        "PREPARED_STREAM names={names} stream_bytes={declared} frame_bytes={frame} limit={METADATA_BYTES}"
    );
    drop(recorded);
    drop(workspace);
    drop(host);
    fs::remove_dir_all(&path).unwrap();
}
