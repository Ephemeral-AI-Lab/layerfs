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
            // Every canonical name this fixture could resolve is absent.
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

fn listed(workspace: &Workspace, serial: u64) -> Vec<Vec<u8>> {
    let handle = workspace.opendir(serial, ReferenceScope::Local).unwrap();
    let mut names = Vec::new();
    let mut cookie = 0;
    loop {
        let page = workspace
            .readdir(handle, cookie, MAX_DIRECTORY_ENTRIES, deadline())
            .unwrap();
        if page.entries().is_empty() {
            break;
        }
        for entry in page.entries() {
            names.push(entry.name.clone());
            cookie += 1;
        }
        if page.entries().len() < MAX_DIRECTORY_ENTRIES {
            break;
        }
    }
    workspace.releasedir(handle).unwrap();
    names.sort();
    names
}

/// Pins the first admission a wide directory meets, so #256's work starts from a
/// measured refusal rather than a guess.
///
/// Run it on demand:
/// `TMPDIR=/src/tmp cargo test --release --locked --manifest-path core/Cargo.toml
/// -p layerfs-workspace --test wide_namespace -- --ignored --nocapture`
///
/// The refusal is the 128th name of one generation: `create 127: Capacity` with
/// `dirty_inodes: 128`, `revision: 127` and 1.9 MiB accounted. It comes from
/// `State::frontier_bytes`, which refuses `dirty > 128 || names > 128` and then
/// bounds the prepared-namespace size it computes by
/// `layerfs_bridge::contract::METADATA_BYTES` (32 KiB). Both are aggregate
/// admissions rather than charged resources, and #256 replaces them with a
/// charged frontier plus a streamed prepared namespace; this case must pass
/// unchanged when that lands.
#[test]
#[ignore = "#256: a generation refuses its 128th dirty identity until the frontier is charged"]
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
    let mut expected: Vec<Vec<u8>> = (0..CREATED)
        .filter(|index| !(RENAMED..(RENAMED + REMOVED)).contains(index))
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
