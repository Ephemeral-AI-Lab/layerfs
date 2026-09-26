//! Namespace work one directory wide: create, rename, unlink and recreate many
//! names through the ordinary public API.
//!
//! The names live in one generation of one mounted Workspace, so this is the
//! shape generalised from "many packages in a directory". It drives the same
//! route a mounted shell uses (`mkdir`/`mknod`/`rename`/`unlink`/`readdir`) with
//! a delivery that resolves every canonical name as absent, and reports the
//! counts the Workspace itself records so a refusal can be attributed.
#![cfg(target_os = "linux")]
use layerfs_bridge::contract::*;
use layerfs_workspace::*;
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const BRANCH: [u8; 17] = [0x11; 17];
const STACK: [u8; 17] = [0x31; 17];
const LAYER: [u8; 33] = [7; 33];
const BASE: Root = [8; 32];
const SCOPE: Root = [9; 32];
const PROFILE: Root = [10; 32];
const ROOT_SERIAL: u64 = 7;
/// Names created before any rename or removal.
const CREATED: usize = 200;
/// Names renamed, removed and recreated afterwards.
const RENAMED: usize = 60;
const REMOVED: usize = 60;
const RECREATED: usize = 25;

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(20)
}
fn temporary() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let path = std::env::temp_dir().join(format!(
        "wide-namespace-{}-{}",
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

struct Serials(AtomicU64);

fn delivery(serials: Arc<Serials>) -> OperationDelivery {
    Arc::new(
        move |request, input, _output, deadline| match &request.operation {
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
            // The attached canonical root lists no name, so every canonical
            // name this fixture could resolve is absent and the effective
            // namespace is exactly what the mounted operations bound.
            Operation::Inspect {
                query: Inspect::List { path, .. },
                ..
            } if path.is_empty() => Ok(Response::List {
                entries: Vec::new(),
                continuation: None,
            }),
            Operation::Inspect {
                query: Inspect::Attributes { .. },
                ..
            } => Err(Code::NotFound.into()),
            Operation::HistoryCommand(HistoryCommand::ReserveInodes { scope, count }) => {
                let start = serials.0.fetch_add(*count, Ordering::AcqRel);
                Ok(Response::History(Box::new(HistoryResult::Reservation {
                    scope: *scope,
                    start,
                    count: *count,
                })))
            }
            Operation::HistoryCommand(HistoryCommand::Commit(_)) => {
                let _ = input;
                Err(Code::Unsupported.into())
            }
            _ => Err(Code::Unsupported.into()),
        },
    )
}

/// One mounted local-edit Workspace over a fresh directory, with the fixture's
/// delivery. The memory budget is what the generation frontier is charged
/// against, so a case can select it.
fn mounted(
    id: &str,
    incarnation: u8,
    memory_budget_bytes: usize,
) -> (PathBuf, WorkspaceHost, Workspace) {
    let path = temporary();
    let metadata = fs::metadata(&path).unwrap();
    use std::os::unix::fs::MetadataExt;
    let serials = Arc::new(Serials(AtomicU64::new(1_000)));
    let host = WorkspaceHost::new(
        WorkspaceConfig {
            root: path.clone(),
            max_count: 1,
            memory_budget_bytes,
            disk_budget_bytes: Some(256 * 1024 * 1024),
        },
        delivery(serials),
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

fn listed(workspace: &Workspace, serial: u64) -> Vec<Vec<u8>> {
    let handle = workspace.opendir(serial, ReferenceScope::Local).unwrap();
    let mut names = Vec::new();
    // A readdir page names, for every entry it returns, the cookie that
    // resumes the walk after that entry. The cookie is that entry's own, not
    // the entry's position: a name this delta already listed keeps the cookie
    // it was given, so a walk that counted entries would resume in the wrong
    // place.
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
                names.push(entry.name.clone());
            }
            cookie = entry.cookie;
        }
        if page.entries().len() < MAX_DIRECTORY_ENTRIES {
            break;
        }
    }
    workspace.releasedir(handle).unwrap();
    names.sort();
    names
}

/// One directory wide enough to bind more names than one metadata page holds.
///
/// Pinned at `c30fe68a0` as `create 127: Capacity` with `dirty_inodes: 128`,
/// `revision: 127` and 1.9 MiB accounted. That refusal was
/// `State::frontier_bytes` refusing `dirty > 128 || names > 128` and then
/// bounding the prepared-namespace size it computed by
/// `layerfs_bridge::contract::METADATA_BYTES` (32 KiB) - two aggregate
/// admissions rather than charged resources. #256's slices 3.1 and 3.2 replaced
/// them with path-local binding and tombstone mutation over a keyed page tree
/// plus a frontier charged from the actual counts, so this case passes
/// unchanged: it was written before the change and has not been edited since
/// except to drop the `#[ignore]` that pinned the refusal.
#[test]
fn one_directory_accepts_many_names_and_survives_rename_and_removal() {
    let path = temporary();
    let metadata = fs::metadata(&path).unwrap();
    use std::os::unix::fs::MetadataExt;
    let serials = Arc::new(Serials(AtomicU64::new(1_000)));
    let host = WorkspaceHost::new(
        WorkspaceConfig {
            root: path.clone(),
            max_count: 1,
            memory_budget_bytes: 64 * 1024 * 1024,
            disk_budget_bytes: Some(256 * 1024 * 1024),
        },
        delivery(serials),
    )
    .unwrap();
    let workspace = host
        .attach(
            AttachOptions {
                id: "wide".into(),
                incarnation: [23; 32],
                store: 1,
                base: Base::Branch(BRANCH),
                access: WorkspaceAccess::LocalEdit,
                owner_uid: metadata.uid(),
                owner_gid: metadata.gid(),
            },
            deadline(),
        )
        .unwrap();
    for index in 0..CREATED {
        workspace
            .mknod(ROOT_SERIAL, &name("f", index), 0o644, 0, deadline())
            .unwrap_or_else(|error| {
                panic!(
                    "create {index}: {error:?} status={:?}",
                    workspace.status().unwrap()
                )
            });
    }
    let created: Vec<Vec<u8>> = (0..CREATED).map(|i| name("f", i)).collect();
    assert_eq!(listed(&workspace, ROOT_SERIAL), created);
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
                    "rename {index}: {error:?} status={:?}",
                    workspace.status().unwrap()
                )
            });
    }
    for index in RENAMED..(RENAMED + REMOVED) {
        workspace
            .unlink(ROOT_SERIAL, &name("f", index), deadline())
            .unwrap_or_else(|error| {
                panic!(
                    "unlink {index}: {error:?} status={:?}",
                    workspace.status().unwrap()
                )
            });
    }
    for index in RENAMED..(RENAMED + RECREATED) {
        workspace
            .mknod(ROOT_SERIAL, &name("f", index), 0o644, 0, deadline())
            .unwrap_or_else(|error| {
                panic!(
                    "recreate {index}: {error:?} status={:?}",
                    workspace.status().unwrap()
                )
            });
    }
    // The names this generation still binds. The first `RENAMED` names are the
    // rename sources, so they are gone and their destinations `g` are here
    // instead. Of the removal range, the names the recreate loop bound again
    // are back. Every one of those three facts is part of the workload above,
    // so all three belong in the oracle.
    let mut expected: Vec<Vec<u8>> = (RENAMED..CREATED)
        .filter(|index| !((RENAMED + RECREATED)..(RENAMED + REMOVED)).contains(index))
        .map(|index| name("f", index))
        .collect();
    expected.extend((0..RENAMED).map(|index| name("g", index)));
    expected.sort();
    let status = workspace.status().unwrap();
    println!(
        "WIDE_NAMESPACE names={} dirty_inodes={} revision={}",
        expected.len(),
        status.dirty_inodes,
        status.revision
    );
    assert_eq!(listed(&workspace, ROOT_SERIAL), expected);
    drop(workspace);
    drop(host);
    fs::remove_dir_all(&path).unwrap();
}

/// One unlink of a wide directory rewrites one path.
///
/// #256 replaced the whole-directory rebuild with a point mutation: the leaf
/// that holds the name is copied along its own path, and it is rewritten with a
/// neighbour only when the removal leaves it under the declared minimum body.
/// The rebuild this replaces walked every name the page still held - one
/// successor lookup per name, each of them a read of the path above it - and
/// then built the page again. The name count here is past one page's 128-cell
/// budget, so the directory owns two leaves and one unlink touches one of them.
#[test]
fn one_unlink_of_a_wide_directory_rewrites_one_path() {
    const WIDE: usize = 130;
    let path = temporary();
    let metadata = fs::metadata(&path).unwrap();
    use std::os::unix::fs::MetadataExt;
    let serials = Arc::new(Serials(AtomicU64::new(1_000)));
    let host = WorkspaceHost::new(
        WorkspaceConfig {
            root: path.clone(),
            max_count: 1,
            memory_budget_bytes: 64 * 1024 * 1024,
            disk_budget_bytes: Some(256 * 1024 * 1024),
        },
        delivery(serials),
    )
    .unwrap();
    let workspace = host
        .attach(
            AttachOptions {
                id: "wide-unlink".into(),
                incarnation: [24; 32],
                store: 1,
                base: Base::Branch(BRANCH),
                access: WorkspaceAccess::LocalEdit,
                owner_uid: metadata.uid(),
                owner_gid: metadata.gid(),
            },
            deadline(),
        )
        .unwrap();
    for index in 0..WIDE {
        workspace
            .mknod(ROOT_SERIAL, &name("f", index), 0o644, 0, deadline())
            .unwrap_or_else(|error| panic!("create {index}: {error:?}"));
    }
    let before = workspace.backing_status().unwrap().metadata_reads;
    workspace
        .unlink(ROOT_SERIAL, &name("f", 0), deadline())
        .unwrap();
    let reads = workspace.backing_status().unwrap().metadata_reads - before;
    println!("UNLINK_PATH names={WIDE} metadata_reads={reads}");
    // The whole-page rebuild this replaces made at least one successor lookup
    // per remaining name of the page, so its own count for one unlink of a
    // directory this wide is above a hundred.
    assert!(
        reads <= 48,
        "one unlink of {WIDE} names read {reads} metadata pages"
    );
    let listed = listed(&workspace, ROOT_SERIAL);
    assert_eq!(listed.len(), WIDE - 1);
    assert!(!listed.contains(&name("f", 0)));
    drop(workspace);
    drop(host);
    fs::remove_dir_all(&path).unwrap();
}

/// A generation admits far more identities than the two aggregate admissions
/// it replaced, and the next fixed count is named.
///
/// #256's own count for this property is 1,025 changed files and names. This
/// case is `#[ignore]`d because that shape costs 44.7 s through the mounted
/// route on the phase-3 machine - each create copies a page path and writes a
/// name, and the run is one test command, over the 30 s rule - not because the
/// property is unproven: run it on demand,
///
/// `TMPDIR=/src/tmp cargo test --release --locked --manifest-path core/Cargo.toml
/// -p layerfs-workspace --test wide_namespace -- --ignored --nocapture`
///
/// All 1,025 creations are admitted. The complete listing that follows is what
/// refuses, and the refusal is the fixed 1,024-entry cookie table
/// (`runtime::state::COOKIE_LIMIT`), not the generation frontier: making a
/// wide readdir walk bounded by charged resources is the same #256 list as the
/// `NODES`/`COOKIES` constants, and this case pins where it stands.
#[test]
#[ignore = "#256: the 1,025-identity shape costs 44.7 s through the mounted route, over the 30 s command rule"]
fn a_generation_admits_many_identities_while_its_budget_has_room() {
    const MANY: usize = 1_025;
    let (path, host, workspace) = mounted("many", 25, 64 * 1024 * 1024);
    for index in 0..MANY {
        workspace
            .mknod(ROOT_SERIAL, &name("f", index), 0o644, 0, deadline())
            .unwrap_or_else(|error| {
                panic!(
                    "create {index}: {error:?} status={:?}",
                    workspace.status().unwrap()
                )
            });
    }
    let status = workspace.status().unwrap();
    println!(
        "FRONTIER_ADMITTED names={MANY} dirty_inodes={} revision={}",
        status.dirty_inodes, status.revision
    );
    // One dirty identity per created file, plus the directory they were
    // created in: the parent's own record moves with its first new name.
    assert_eq!(status.dirty_inodes, MANY + 1);
    assert_eq!(status.revision, MANY as u64);
    // Every creation above was admitted; the walk below is what refuses, and
    // what it refuses with is named rather than reported as an unnamed miss.
    let handle = workspace
        .opendir(ROOT_SERIAL, ReferenceScope::Local)
        .unwrap();
    let mut cookie = 0;
    let mut listed = 0;
    let refusal = loop {
        match workspace.readdir(handle, cookie, MAX_DIRECTORY_ENTRIES, deadline()) {
            Ok(page) if page.entries().is_empty() => break None,
            Ok(page) => {
                for entry in page.entries() {
                    if entry.name != b"." && entry.name != b".." {
                        listed += 1;
                    }
                    cookie = entry.cookie;
                }
            }
            Err(error) => break Some(error),
        }
    };
    println!("FRONTIER_LISTING listed={listed} refusal={refusal:?}");
    assert_eq!(listed, 1_022, "the walk stops at the cookie table");
    assert!(
        matches!(refusal, Some(WorkspaceError::Capacity)),
        "a complete listing of {MANY} names refuses at the cookie table"
    );
    drop(workspace);
    drop(host);
    fs::remove_dir_all(&path).unwrap();
}

/// A spent budget refuses precisely and publishes nothing.
///
/// The same mutation path, on a budget that runs out: the refusal is
/// `Capacity`, the generation's revision and listing do not move, and the name
/// the refused creation would have bound is absent afterwards. The count it
/// stops at is charged work, not a constant: the live-node table grows by
/// charged chunks against this same budget, and the frontier is charged from
/// the generation's own counts, so 4 MiB admits 511 identities where the two
/// former aggregate admissions stopped the same route at 127.
#[test]
fn a_spent_budget_refuses_precisely_and_publishes_nothing() {
    const ATTEMPT: usize = 4_096;
    let (path, host, workspace) = mounted("spent", 26, 4 * 1024 * 1024);
    let mut refused = None;
    for index in 0..ATTEMPT {
        if let Err(error) = workspace.mknod(ROOT_SERIAL, &name("f", index), 0o644, 0, deadline()) {
            refused = Some((index, error));
            break;
        }
    }
    let (index, error) = refused.expect("a 4 MiB budget cannot admit every name of this attempt");
    assert!(
        index > 256,
        "the live-node table is charged, so it admits more than one chunk"
    );
    assert!(
        matches!(error, WorkspaceError::Capacity),
        "the refusal is the budget's, got {error:?}"
    );
    let status = workspace.status().unwrap();
    let listed = listed(&workspace, ROOT_SERIAL);
    println!(
        "FRONTIER_REFUSED index={index} revision={} listed={}",
        status.revision,
        listed.len()
    );
    assert_eq!(status.revision, index as u64);
    assert_eq!(listed.len(), index);
    assert!(!listed.contains(&name("f", index)));
    // The generation is still usable: the same name is refused, and the
    // workspace still answers its own state rather than a torn one.
    let again = workspace
        .mknod(ROOT_SERIAL, &name("f", index), 0o644, 0, deadline())
        .unwrap_err();
    assert!(matches!(again, WorkspaceError::Capacity));
    assert_eq!(workspace.status().unwrap().revision, index as u64);
    drop(workspace);
    drop(host);
    fs::remove_dir_all(&path).unwrap();
}
