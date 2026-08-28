use super::control::{control_paths, execute_splice_control};
use super::encoding::{hex, json};
use super::launch::{
    branch_id, hash_file, move_mount, number, parse_arguments, prepare_public_mount, required_path,
};
use super::path_validation::{
    canonical_target, required_integrity, source_identity, ValidatedPaths, SOURCE_COMMIT,
    SOURCE_TREE,
};
use fuser::{Config, INodeNo, MountOption};
use layerfs_mount::workspace::MountedWorkspace;
use layerfs_mount::{LayerFuse, LayerFuseEvent, MountDriver};
use layerfs_workspace::{
    CommitResult, IntegrityMode, OperationWorkspace, Presentation, WorkingStore, WorkspacePaths,
};
use signal_hook::consts::signal::{SIGHUP, SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use std::ffi::OsStr;
use std::fs::OpenOptions;
use std::io::Write;
use std::time::Instant;
pub fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let complete_started = Instant::now();
    let arguments = parse_arguments()?;
    let integrity = required_integrity(&arguments)?;
    let (source_commit, source_tree) =
        source_identity(cfg!(debug_assertions), SOURCE_COMMIT, SOURCE_TREE)?;
    let store = required_path(&arguments, "store")?;
    let published_mount = required_path(&arguments, "mount")?;
    let receipt = required_path(&arguments, "receipt")?;
    let receipt = canonical_target(&receipt)?;
    if receipt.exists() {
        return Err(format!("receipt already exists: {}", receipt.display()).into());
    }
    let branch_id = branch_id(
        arguments
            .get("branch")
            .ok_or("missing --branch (64 lowercase hexadecimal characters)")?,
    )?;
    let uid = number(&arguments, "uid", 0)?;
    let gid = number(&arguments, "gid", 0)?;
    let executable = std::env::current_exe()?;
    let executable_hash = hash_file(&executable)?;
    let working = WorkingStore::open(&store, integrity)?;
    let store = working.root().to_owned();
    let published_mount = prepare_public_mount(&published_mount, &store, &receipt)?;
    let head = working
        .branch_head(branch_id)?
        .ok_or("Working Branch does not exist")?;
    let (admission, ticket) =
        layerfs_workspace::begin_operation(&working, head, Presentation::Mount)?;
    let workspace_paths = WorkspacePaths::create(working.root(), &ticket)?;
    let private_mount = workspace_paths.view().to_owned();
    let spool = workspace_paths.spool().join("dirty-ranges");
    let paths = ValidatedPaths {
        store: store.clone(),
        mount: published_mount.clone(),
        spool: spool.clone(),
        receipt: receipt.clone(),
    };
    let control = control_paths(&arguments, &paths)?;
    drop(working);
    let workspace = MountedWorkspace::open(&store, admission, integrity, spool)?;
    let mut connections_high_water = workspace.active_store_connections()?;
    let driver = MountDriver::new(workspace, private_mount.clone());
    let (mut operation, _) = OperationWorkspace::start(ticket, driver, Some(workspace_paths))?;
    let fuse = LayerFuse::from_shared(operation.driver().shared_workspace(), uid, gid);
    let shared_workspace = fuse.shared_workspace();
    let shared_fuse_counters = fuse.shared_counters();
    let notifier_slot = fuse.notifier_slot();
    let byte_budget = fuse.byte_budget();
    let mut config = Config::default();
    config.mount_options = vec![
        MountOption::FSName("layerfs".to_owned()),
        MountOption::Subtype("layerfs".to_owned()),
        MountOption::RW,
        MountOption::Exec,
        MountOption::NoDev,
        MountOption::NoSuid,
        MountOption::NoAtime,
        MountOption::DefaultPermissions,
    ];
    config.n_threads = Some(4);
    config.clone_fd = true;
    let (stop_sender, stop_receiver) = std::sync::mpsc::channel();
    fuse.set_lifecycle_sender(stop_sender.clone())
        .map_err(|_| "FUSE lifecycle sender initialized twice")?;
    let mut signals = Signals::new([SIGTERM, SIGINT, SIGHUP])?;
    let signal_handle = signals.handle();
    let session = fuser::Session::new(fuse, &private_mount, &config)?;
    let notifier = session.notifier();
    notifier_slot
        .set(notifier.clone())
        .map_err(|_| "FUSE notifier initialized twice")?;
    let invalidations = [
        notifier.inval_inode(INodeNo::ROOT, -1, 0),
        notifier.inval_entry(INodeNo::ROOT, OsStr::new(".layerfs-invalidation-admission")),
    ];
    {
        let mut counters = shared_fuse_counters
            .lock()
            .map_err(|_| "counter lock poisoned")?;
        counters.invalidations_requested += invalidations.len() as u64;
        counters.invalidations_succeeded +=
            invalidations.iter().filter(|result| result.is_ok()).count() as u64;
        counters.invalidations_failed += invalidations
            .iter()
            .filter(|result| result.is_err())
            .count() as u64;
    }
    if invalidations.iter().any(Result::is_err) {
        shared_workspace
            .lock()
            .map_err(|_| "workspace lock poisoned")?
            .mark_incomplete();
        return Err("FUSE invalidation admission failed".into());
    }
    move_mount(&private_mount, &published_mount)?;
    let background_session = session.spawn()?;
    let signal_thread = std::thread::spawn(move || {
        let mut forwarded = false;
        for signal in signals.forever() {
            if !forwarded {
                let _ = stop_sender.send(LayerFuseEvent::Signal(signal));
                forwarded = true;
            }
        }
    });
    println!(
        "{{\"backend\":\"layerfs-mount\",\"mount\":\"{}\",\"integrity\":\"{}\",\"source_commit\":\"{}\",\"source_tree\":\"{}\"}}",
        json(published_mount.to_string_lossy().as_ref()),
        match integrity {
            IntegrityMode::Verified => "Verified",
            IntegrityMode::TrustedLocalDev => "TrustedLocalDev",
        },
        json(source_commit),
        json(source_tree),
    );
    std::io::stdout().flush()?;
    let live_started = Instant::now();
    let stop = stop_receiver.recv().map_err(|_| "stop channel closed")?;
    let live_ns = live_started.elapsed().as_nanos();
    let (signal, mut lifecycle_error, session_terminated, session_error) = match stop {
        LayerFuseEvent::Signal(signal) => {
            let mut lifecycle_error = match (signal, control.as_ref()) {
                (SIGHUP, Some(control)) => {
                    execute_splice_control(&shared_workspace, &byte_budget, control).err()
                }
                _ => None,
            };
            if let Err(error) = move_mount(&published_mount, &private_mount) {
                lifecycle_error.get_or_insert(error);
            }
            match background_session.umount_and_join() {
                Ok(()) => (signal, lifecycle_error, true, None),
                Err(error) => (signal, lifecycle_error, false, Some(error.to_string())),
            }
        }
        LayerFuseEvent::SessionEnded => {
            let session_error = background_session
                .join()
                .map_err(|error| error.to_string())
                .err();
            (0, None, true, session_error)
        }
    };
    let mut quiescence_ns = 0_u128;
    let mut driver_freeze_ns = 0_u128;
    if lifecycle_error.is_none() && session_terminated && session_error.is_none() {
        let queue_quiescence = Instant::now();
        lifecycle_error = byte_budget
            .close_and_wait()
            .map_err(|error| error.to_string())
            .err();
        quiescence_ns = queue_quiescence.elapsed().as_nanos();
        if lifecycle_error.is_none() {
            match operation.freeze_observed(std::time::Duration::from_secs(30)) {
                Ok(observation) => {
                    quiescence_ns += observation.quiescence_ns;
                    driver_freeze_ns = observation.driver_freeze_ns;
                }
                Err(error) => lifecycle_error = Some(error.to_string()),
            }
        }
    }
    signal_handle.close();
    let signal_thread_error = signal_thread
        .join()
        .err()
        .map(|_| "signal thread panicked".to_owned());
    drop(notifier_slot);
    let fuse = *shared_fuse_counters
        .lock()
        .map_err(|_| "counter lock poisoned")?;
    drop(shared_fuse_counters);
    let finalize_allowed =
        lifecycle_error.is_none() && session_terminated && session_error.is_none();
    let candidate_root = shared_workspace
        .lock()
        .map_err(|_| "workspace lock poisoned")?
        .candidate_root();
    let candidate_started = Instant::now();
    let finalize_error = if finalize_allowed {
        operation
            .finalize_candidate(admission.base.root(), candidate_root, Vec::new())
            .map(drop)
            .map_err(|error| error.to_string())
            .err()
    } else {
        None
    };
    let candidate_ns = driver_freeze_ns + candidate_started.elapsed().as_nanos();
    let working_commit_started = Instant::now();
    let (commit, commit_transport_error) = if finalize_allowed && finalize_error.is_none() {
        match shared_workspace
            .lock()
            .map_err(|_| "workspace lock poisoned")?
            .commit_operation()
        {
            Ok(outcome) => (Some(outcome), None),
            Err(error) => (None, Some(error.to_string())),
        }
    } else {
        (None, None)
    };
    let working_commit_ns = working_commit_started.elapsed().as_nanos();
    let cleanup_started = Instant::now();
    let cleanup_error = if finalize_allowed && finalize_error.is_none() {
        operation
            .cleanup()
            .map(drop)
            .map_err(|error| error.to_string())
            .err()
    } else {
        None
    };
    let cleanup_ns = cleanup_started.elapsed().as_nanos();
    let mut workspace = shared_workspace
        .lock()
        .map_err(|_| "workspace lock poisoned")?;
    let mounted = workspace.counters()?;
    let engine = workspace.engine_counters()?;
    let (accepted, record, reconciled, commit_error) = match commit {
        Some(CommitResult::WorkingRecorded {
            head,
            record,
            reconciled,
        }) => (head, Some(record), Some(reconciled), None),
        Some(CommitResult::Conflict { actual, candidate }) => (
            actual,
            None,
            None,
            Some(format!(
                "Working OperationCommit conflict; candidate {} preserved for operation {}",
                candidate.root,
                hex(candidate.operation_id.as_bytes())
            )),
        ),
        None => (admission.branch_head_before, None, None, None),
    };
    let acknowledgement_error = if cleanup_error.is_none() {
        record.and_then(|record| {
            workspace
                .acknowledge_operation(record)
                .map(drop)
                .map_err(|error| error.to_string())
                .err()
        })
    } else {
        None
    };
    let connections_before_drop = workspace.active_store_connections()?;
    connections_high_water = connections_high_water.max(connections_before_drop);
    workspace.close_store_connection()?;
    let connections_terminal = workspace.active_store_connections()?;
    drop(workspace);
    let kernel_cache_released = finalize_allowed && cleanup_error.is_none();
    let terminal_error = lifecycle_error
        .as_ref()
        .or(session_error.as_ref())
        .or(signal_thread_error.as_ref())
        .or(finalize_error.as_ref())
        .or(commit_transport_error.as_ref())
        .or(cleanup_error.as_ref())
        .or(acknowledgement_error.as_ref())
        .or(commit_error.as_ref())
        .cloned();
    include!("terminal_receipt.rs")
}
