//! TEMPORARY DIAGNOSTIC (not a registered benchmark selection, not a gate).
//!
//! Question: does the product's per-Commit cost grow with history depth?
//! Route: host placement + materialized projection (no Docker, no FUSE), the
//! public SDK route used by `issue116_capacity.rs`.
//!
//! Select explicitly:
//!   LAYERFS_DIAG_COMMITS=2000 cargo +1.85.1 test -p layerfs-workspace --release \
//!     --test deep_history_diagnostic -- --ignored --nocapture --test-threads=1
//!
//! One observation, uncontrolled OS cache, one host. Diagnostics only.

use layerfs_layerstack_store::{
    EntityName, LayerStackInitialization, LayerStackStore, LocalForkSource,
};
use layerfs_workspace::{
    CreateWorkspaceSession, EndWorkspaceMode, WorkspaceFileRangeEdit, WorkspaceFileReplacement,
    WorkspacePlacement, WorkspaceProjection, WorkspaceResult, Workspaces,
};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn file_bytes() -> usize {
    env_usize("LAYERFS_DIAG_FILE_BYTES", 1024)
}

fn change_bytes() -> usize {
    env_usize("LAYERFS_DIAG_CHANGE_BYTES", 4).min(file_bytes())
}

fn root_dir() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "layerfs-deep-history-diag-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn store_bytes(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn edit_one(
    workspaces: &Workspaces,
    session: layerfs_workspace::WorkspaceId,
    path: &str,
    index: u32,
) -> WorkspaceResult<()> {
    let change = change_bytes();
    // Both a rolling small change and a whole-prefix overwrite write a value
    // that never equals the file's original fill byte (3): an identical write
    // would be UpToDate and would not be a Commit at all.
    let offset = if change < file_bytes() {
        ((index as usize) % (file_bytes() / change) * change) as u64
    } else {
        0
    };
    let mut value = (index % 253 + 1) as u8;
    if value == 3 {
        value = 7;
    }
    // Uniform fill dedupes across Commits; a unique pseudo-random payload per
    // Commit is what makes a large-Store arm meaningful.
    let payload: Vec<u8> = if std::env::var("LAYERFS_DIAG_UNIQUE").as_deref() == Ok("1") {
        (0..change)
            .map(|byte| {
                let mixed = (index as u32)
                    .wrapping_mul(2_654_435_761)
                    .wrapping_add((byte as u32).wrapping_mul(2_246_822_519));
                (mixed >> 19) as u8
            })
            .collect()
    } else {
        vec![value; change]
    };
    workspaces.edit_workspace_file_range(WorkspaceFileRangeEdit {
        workspace_id: session,
        path: path.into(),
        start: offset,
        delete_len: change as u64,
        replacement: WorkspaceFileReplacement::Inline(payload),
    })
}

struct Stats {
    samples: Vec<u128>,
}

impl Stats {
    fn new() -> Self {
        Stats { samples: Vec::new() }
    }
    fn push(&mut self, ns: u128) {
        self.samples.push(ns);
    }
    fn report(&self, label: &str) {
        if self.samples.is_empty() {
            println!("DIAG {label} n=0");
            return;
        }
        let mut sorted = self.samples.clone();
        sorted.sort_unstable();
        let n = sorted.len();
        let sum: u128 = sorted.iter().sum();
        let mean = sum / n as u128;
        let median = sorted[n / 2];
        let p95 = sorted[(n * 95 / 100).min(n - 1)];
        let max = sorted[n - 1];
        println!(
            "DIAG {label} n={n} mean_ms={:.3} median_ms={:.3} p95_ms={:.3} max_ms={:.3} sum_ms={:.3}",
            mean as f64 / 1e6,
            median as f64 / 1e6,
            p95 as f64 / 1e6,
            max as f64 / 1e6,
            sum as f64 / 1e6
        );
    }
}

fn commit_edit(
    workspaces: &Workspaces,
    session: layerfs_workspace::WorkspaceId,
    index: u32,
    claimed: &mut u32,
    up_to_date: &mut u32,
) -> u128 {
    edit_one(workspaces, session, "f0", index).unwrap();
    let started = Instant::now();
    let result = workspaces.commit_workspace_session(session).unwrap();
    let elapsed = started.elapsed().as_nanos();
    match result {
        layerfs_workspace::WorkspaceCommitResult::Created { .. } => *claimed += 1,
        layerfs_workspace::WorkspaceCommitResult::UpToDate { .. } => *up_to_date += 1,
        other => panic!("unexpected commit result: {other:?}"),
    }
    elapsed
}

/// One complete process-lifetime cycle: connect the Store, open a session,
/// edit, Commit, end, drop. This is the shape a CLI invocation has.
fn reopen_cycle(
    root: &Path,
    store_path: &Path,
    branch: layerfs_layerstack_store::BranchId,
    label: &str,
    index: u32,
) -> (u128, u128, u128, bool) {
    let connect_started = Instant::now();
    let store = LayerStackStore::connect(store_path).unwrap();
    let connect_ns = connect_started.elapsed().as_nanos();
    let workspaces =
        Workspaces::new(root.join(format!("runtime-{label}")), store).unwrap();
    let session_started = Instant::now();
    let session = workspaces
        .create_workspace_session(CreateWorkspaceSession {
            branch_id: branch,
            placement: WorkspacePlacement::Host {
                root: root.join(format!("mount-{label}")),
            },
            projection: Some(WorkspaceProjection::Materialize),
        })
        .unwrap();
    let session_ns = session_started.elapsed().as_nanos();
    let mut claimed = 0;
    let mut up_to_date = 0;
    let commit_ns = commit_edit(&workspaces, session.id, index, &mut claimed, &mut up_to_date);
    let written = std::fs::read(root.join(format!("mount-{label}")).join("f0"))
        .map(|bytes| format!("len={} b0={} b4={} b8={} b12={}", bytes.len(), bytes[0], bytes[4], bytes[8], bytes[12]))
        .unwrap_or_else(|error| format!("unreadable: {error}"));
    println!(
        "DIAG cycle label={label} index={index} claimed={claimed} up_to_date={up_to_date} mount_f0={written}"
    );
    workspaces
        .end_workspace_session(session.id, EndWorkspaceMode::Clean)
        .unwrap();
    drop(workspaces);
    (connect_ns, session_ns, commit_ns, claimed == 1 && up_to_date == 0)
}

#[test]
#[ignore = "temporary deep-history diagnostic; not a registered selection"]
fn deep_history_commit_scaling() {
    let total: u32 = std::env::var("LAYERFS_DIAG_COMMITS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(2_000);
    let root = root_dir();
    let source = root.join("source");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("f0"), vec![3u8; file_bytes()]).unwrap();
    let store_path = root.join("store.sqlite");
    let mut index = 0u32;
    let mut claimed = 0u32;
    let mut up_to_date = 0u32;

    println!(
        "DIAG root={} store={} total={total} file_bytes={} change_bytes={}",
        root.display(),
        store_path.display(),
        file_bytes(),
        change_bytes()
    );

    // Phase 0: one session-level Commit on a fresh Store (depth 1).
    let (branch, first) = {
        let store = LayerStackStore::create(&store_path).unwrap();
        let layer = store
            .initialize_layerstack(
                EntityName::new("project").unwrap(),
                LayerStackInitialization::Directory(source.clone()),
            )
            .unwrap()
            .genesis_layer_id;
        let branch = store
            .fork_branch(
                EntityName::new("main").unwrap(),
                LocalForkSource::Layer { layer_id: layer },
            )
            .unwrap();
        let workspaces = Workspaces::new(root.join("runtime"), store.clone()).unwrap();
        let session = workspaces
            .create_workspace_session(CreateWorkspaceSession {
                branch_id: branch,
                placement: WorkspacePlacement::Host {
                    root: root.join("mount"),
                },
                projection: Some(WorkspaceProjection::Materialize),
            })
            .unwrap();
        let commit_ns = commit_edit(&workspaces, session.id, index, &mut claimed, &mut up_to_date);
        workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Clean)
            .unwrap();
        drop(workspaces);
        (branch, commit_ns)
    };
    println!(
        "DIAG shallow_session_commit_ms={:.3} committed={claimed} up_to_date={up_to_date} store_mib={:.1}",
        first as f64 / 1e6,
        store_bytes(&store_path) as f64 / (1024.0 * 1024.0)
    );

    // Phase 1: shallow reopen cycles (connect + session + one Commit).
    let mut shallow_connect = Stats::new();
    let mut shallow_session = Stats::new();
    let mut shallow_commit = Stats::new();
    for cycle_index in 0..3 {
        index += 1;
        let label = format!("shallow-{cycle_index}");
        let cycle = reopen_cycle(&root, &store_path, branch, &label, index);
        assert!(cycle.3, "shallow reopen cycle must create exactly one Commit");
        shallow_connect.push(cycle.0);
        shallow_session.push(cycle.1);
        shallow_commit.push(cycle.2);
    }
    shallow_connect.report("shallow.connect");
    shallow_session.report("shallow.session_create");
    shallow_commit.report("shallow.commit");
    println!(
        "DIAG shallow_after_reopens depth={index} store_mib={:.1}",
        store_bytes(&store_path) as f64 / (1024.0 * 1024.0)
    );

    // Phase 2: one Store handle and one session, `total` edit+Commit cycles.
    let connect_started = Instant::now();
    let store = LayerStackStore::connect(&store_path).unwrap();
    let deep_connect_ns = connect_started.elapsed().as_nanos();
    let workspaces = Workspaces::new(root.join("runtime-deep"), store.clone()).unwrap();
    let session_started = Instant::now();
    let session = workspaces
        .create_workspace_session(CreateWorkspaceSession {
            branch_id: branch,
            placement: WorkspacePlacement::Host {
                root: root.join("mount-deep"),
            },
            projection: Some(WorkspaceProjection::Materialize),
        })
        .unwrap();
    let deep_session_ns = session_started.elapsed().as_nanos();
    println!(
        "DIAG deep.connect_ms={:.3} deep.session_create_ms={:.3} depth_at_connect={index}",
        deep_connect_ns as f64 / 1e6,
        deep_session_ns as f64 / 1e6
    );
    let mut claimed = 0u32;
    let mut up_to_date = 0u32;
    let mut batch = Stats::new();
    let loop_started = Instant::now();
    let mut last_mark = Instant::now();
    for step in 0..total {
        index += 1;
        let commit_ns = commit_edit(&workspaces, session.id, index, &mut claimed, &mut up_to_date);
        batch.push(commit_ns);
        if (step + 1) % 250 == 0 {
            let mut sorted = batch.samples.clone();
            sorted.sort_unstable();
            let n = sorted.len();
            println!(
                "DIAG depth={} batch={} batch_ms={:.1} median_ms={:.3} p95_ms={:.3} store_mib={:.1} wall_ms={}",
                index,
                n,
                batch.samples.iter().sum::<u128>() as f64 / 1e6,
                sorted[n / 2] as f64 / 1e6,
                sorted[(n * 95 / 100).min(n - 1)] as f64 / 1e6,
                store_bytes(&store_path) as f64 / (1024.0 * 1024.0),
                last_mark.elapsed().as_millis()
            );
            batch = Stats::new();
            last_mark = Instant::now();
        }
    }
    println!(
        "DIAG deep_loop total_commits={total} created={claimed} up_to_date={up_to_date} wall_s={:.1} store_mib={:.1}",
        loop_started.elapsed().as_secs_f64(),
        store_bytes(&store_path) as f64 / (1024.0 * 1024.0)
    );
    assert_eq!(claimed, total, "every edit must produce a Created Commit");
    assert_eq!(up_to_date, 0);
    workspaces
        .end_workspace_session(session.id, EndWorkspaceMode::Clean)
        .unwrap();
    drop(workspaces);
    drop(store);

    // Phase 3: reopen cycles at the final depth.
    let mut deep_connect = Stats::new();
    let mut deep_session = Stats::new();
    let mut deep_commit = Stats::new();
    for cycle_index in 0..3 {
        index += 1;
        let label = format!("deep-{cycle_index}");
        let cycle = reopen_cycle(&root, &store_path, branch, &label, index);
        assert!(cycle.3, "deep reopen cycle must create exactly one Commit");
        deep_connect.push(cycle.0);
        deep_session.push(cycle.1);
        deep_commit.push(cycle.2);
    }
    deep_connect.report("deep.connect");
    deep_session.report("deep.session_create");
    deep_commit.report("deep.commit");
    println!(
        "DIAG artefact root={} store={} store_mib={:.1} final_depth={index}",
        root.display(),
        store_path.display(),
        store_bytes(&store_path) as f64 / (1024.0 * 1024.0)
    );
}

/// Diagnostic: product `connect` (Store open) on an existing Store copy.
///
/// `LAYERFS_DIAG_STORE_PATH=/tmp/.../store.sqlite` — never the retained original.
#[test]
#[ignore = "temporary store-open diagnostic; not a registered selection"]
fn store_connect_only() {
    let path = std::env::var("LAYERFS_DIAG_STORE_PATH").expect("LAYERFS_DIAG_STORE_PATH");
    for _ in 0..3 {
        let started = Instant::now();
        let store = LayerStackStore::connect(&path).unwrap();
        println!(
            "DIAG connect_only path={path} ms={:.3}",
            started.elapsed().as_secs_f64() * 1000.0
        );
        drop(store);
    }
}

/// Diagnostic: on an existing Store copy, `connect` then one edit + first Commit
/// after the reopen, through a fresh materialized host session.
///
/// `LAYERFS_DIAG_STORE_PATH`, `LAYERFS_DIAG_BRANCH_HEX`, `LAYERFS_DIAG_ROOT`.
#[test]
#[ignore = "temporary reopen-commit diagnostic; not a registered selection"]
fn store_reopen_commit() {
    let path = std::env::var("LAYERFS_DIAG_STORE_PATH").expect("LAYERFS_DIAG_STORE_PATH");
    let branch: layerfs_layerstack_store::BranchId = std::env::var("LAYERFS_DIAG_BRANCH_HEX")
        .expect("LAYERFS_DIAG_BRANCH_HEX")
        .parse()
        .unwrap();
    let root = PathBuf::from(std::env::var("LAYERFS_DIAG_ROOT").expect("LAYERFS_DIAG_ROOT"));
    std::fs::create_dir_all(&root).unwrap();
    let store = LayerStackStore::connect(&path).unwrap();
    let workspaces = Workspaces::new(root.join("runtime-reopen"), store).unwrap();
    let mount = root.join("mount-reopen");
    let started = Instant::now();
    let session = workspaces
        .create_workspace_session(CreateWorkspaceSession {
            branch_id: branch,
            placement: WorkspacePlacement::Host { root: mount.clone() },
            projection: Some(WorkspaceProjection::Materialize),
        })
        .unwrap();
    println!(
        "DIAG reopen_commit session_create_ms={:.3}",
        started.elapsed().as_secs_f64() * 1000.0
    );
    let mut stack = vec![mount.clone()];
    let mut target = None;
    while let Some(directory) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&directory)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .collect();
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            let entry_path = entry.path();
            let metadata = std::fs::symlink_metadata(&entry_path).unwrap();
            if metadata.is_dir() {
                stack.push(entry_path);
            } else if metadata.is_file() && metadata.len() >= 1 {
                target = Some(entry_path);
                break;
            }
        }
        if target.is_some() {
            break;
        }
    }
    let target = target.expect("a regular file in the materialized tree");
    let relative = target
        .strip_prefix(&mount)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    // The benchmark harness normalizes fixture metadata before an edit; this
    // diagnostic does the same for the single file it touches.
    let _ = std::process::Command::new("chmod")
        .arg("644")
        .arg(&target)
        .status();
    let _ = std::process::Command::new("touch")
        .arg("-t")
        .arg("202001010000.00")
        .arg(&target)
        .status();
    let before = std::fs::read(&target).unwrap();
    let mut replacement = before[0];
    replacement ^= 0xff;
    println!(
        "DIAG reopen_commit target={relative} bytes={}",
        before.len()
    );
    workspaces
        .edit_workspace_file_range(WorkspaceFileRangeEdit {
            workspace_id: session.id,
            path: relative.as_str().into(),
            start: 0,
            delete_len: 1,
            replacement: WorkspaceFileReplacement::Inline(vec![replacement]),
        })
        .unwrap();
    let started = Instant::now();
    let result = workspaces.commit_workspace_session(session.id).unwrap();
    println!(
        "DIAG reopen_commit first_commit_ms={:.3} result={result:?}",
        started.elapsed().as_secs_f64() * 1000.0
    );
    let started = Instant::now();
    let second = workspaces.commit_workspace_session(session.id).unwrap();
    println!(
        "DIAG reopen_commit second_commit_ms={:.3} result={second:?}",
        started.elapsed().as_secs_f64() * 1000.0
    );
    workspaces
        .end_workspace_session(session.id, EndWorkspaceMode::Clean)
        .unwrap();
}

fn tree_bytes(root: &Path) -> (u64, u64) {
    let mut files = 0u64;
    let mut bytes = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.filter_map(|entry| entry.ok()) {
            let path = entry.path();
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.is_dir() {
                stack.push(path);
            } else if metadata.is_file() {
                files += 1;
                bytes += metadata.len();
            }
        }
    }
    (files, bytes)
}

/// Diagnostic: does opening a second workspace cost a fraction of the first?
///
/// `LAYERFS_DIAG_STORE_PATH`, `LAYERFS_DIAG_BRANCH_HEX`, `LAYERFS_DIAG_ROOT`,
/// optional `LAYERFS_DIAG_SESSIONS` (default 3).
#[test]
#[ignore = "temporary workspace-open diagnostic; not a registered selection"]
fn workspace_open_scaling() {
    let path = std::env::var("LAYERFS_DIAG_STORE_PATH").expect("LAYERFS_DIAG_STORE_PATH");
    let branch: layerfs_layerstack_store::BranchId = std::env::var("LAYERFS_DIAG_BRANCH_HEX")
        .expect("LAYERFS_DIAG_BRANCH_HEX")
        .parse()
        .unwrap();
    let root = PathBuf::from(std::env::var("LAYERFS_DIAG_ROOT").expect("LAYERFS_DIAG_ROOT"));
    let sessions = env_usize("LAYERFS_DIAG_SESSIONS", 3);
    std::fs::create_dir_all(&root).unwrap();
    println!(
        "DIAG open store_bytes={} root={}",
        store_bytes(Path::new(&path)),
        root.display()
    );

    // A. Store connect, repeated in one process (each is a new connection).
    for step in 1..=3 {
        let started = Instant::now();
        let store = LayerStackStore::connect(&path).unwrap();
        println!(
            "DIAG open connect_{step}_ms={:.3}",
            started.elapsed().as_secs_f64() * 1000.0
        );
        drop(store);
    }

    // B. Sequential workspace opens: one Store handle, one Workspaces owner,
    //    each session ended before the next begins.
    let store = LayerStackStore::connect(&path).unwrap();
    let workspaces = Workspaces::new(root.join("runtime-open"), store).unwrap();
    for step in 1..=sessions {
        let mount = root.join(format!("mount-open-{step}"));
        let started = Instant::now();
        let session = workspaces
            .create_workspace_session(CreateWorkspaceSession {
                branch_id: branch,
                placement: WorkspacePlacement::Host { root: mount.clone() },
                projection: Some(WorkspaceProjection::Materialize),
            })
            .unwrap();
        let create_ms = started.elapsed().as_secs_f64() * 1000.0;
        let (files, bytes) = tree_bytes(&mount);
        println!(
            "DIAG open session_{step}_create_ms={create_ms:.3} files={files} tree_bytes={bytes}"
        );
        let ended = workspaces.end_workspace_session(session.id, EndWorkspaceMode::Discard);
        println!("DIAG open session_{step}_end={ended:?}");
    }

    // C. Concurrent opens: two live sessions on one branch.
    let mut live = Vec::new();
    for step in 1..=2 {
        let mount = root.join(format!("mount-live-{step}"));
        let started = Instant::now();
        let session = workspaces
            .create_workspace_session(CreateWorkspaceSession {
                branch_id: branch,
                placement: WorkspacePlacement::Host { root: mount.clone() },
                projection: Some(WorkspaceProjection::Materialize),
            })
            .unwrap();
        println!(
            "DIAG open concurrent_{step}_create_ms={:.3}",
            started.elapsed().as_secs_f64() * 1000.0
        );
        live.push(session);
    }
    for session in live {
        let ended = workspaces.end_workspace_session(session.id, EndWorkspaceMode::Discard);
        println!("DIAG open concurrent_end={ended:?}");
    }
}
