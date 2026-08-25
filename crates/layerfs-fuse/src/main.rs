#[cfg(any(target_os = "linux", test))]
#[derive(Debug)]
struct ValidatedPaths {
    store: std::path::PathBuf,
    mount: std::path::PathBuf,
    spool: std::path::PathBuf,
    receipt: std::path::PathBuf,
}

#[cfg(any(target_os = "linux", test))]
fn prepare_paths(
    store: &std::path::Path,
    mount: &std::path::Path,
    spool: &std::path::Path,
    receipt: &std::path::Path,
) -> Result<ValidatedPaths, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(mount)?;
    let mount = std::fs::canonicalize(mount)?;
    if std::fs::read_dir(&mount)?.next().is_some() {
        return Err(format!("mountpoint must be empty: {}", mount.display()).into());
    }
    let store = canonical_target(store)?;
    let spool = canonical_target(spool)?;
    let receipt = canonical_target(receipt)?;
    if receipt.exists() {
        return Err(format!("receipt already exists: {}", receipt.display()).into());
    }
    let external = [&store, &spool, &receipt];
    for (index, path) in external.iter().enumerate() {
        if path.starts_with(&mount) {
            return Err(format!("path must be outside mount: {}", path.display()).into());
        }
        for other in &external[..index] {
            if path == other || same_existing_file(path, other)? {
                return Err(format!(
                    "Store, spool, and receipt paths must be distinct: {}",
                    path.display()
                )
                .into());
            }
        }
    }
    Ok(ValidatedPaths {
        store,
        mount,
        spool,
        receipt,
    })
}

#[cfg(any(target_os = "linux", test))]
fn canonical_target(
    path: &std::path::Path,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let name = absolute
        .file_name()
        .ok_or_else(|| format!("path must name a file: {}", path.display()))?;
    let parent = absolute
        .parent()
        .ok_or_else(|| format!("path has no parent: {}", path.display()))?;
    std::fs::create_dir_all(parent)?;
    let target = std::fs::canonicalize(parent)?.join(name);
    if target.exists() {
        Ok(std::fs::canonicalize(target)?)
    } else {
        Ok(target)
    }
}

#[cfg(any(target_os = "linux", test))]
fn same_existing_file(
    left: &std::path::Path,
    right: &std::path::Path,
) -> Result<bool, std::io::Error> {
    use std::os::unix::fs::MetadataExt;
    let (Ok(left), Ok(right)) = (std::fs::metadata(left), std::fs::metadata(right)) else {
        return Ok(false);
    };
    Ok(left.dev() == right.dev() && left.ino() == right.ino())
}

#[cfg(target_os = "linux")]
mod linux {
    use fuser::{Config, INodeNo, MountOption};
    use layerfs_fuse::{LayerFuse, FS_BENCH_SHA256};
    use layerfs_vfs::mounted::MountedWorkspace;
    use layerfs_vfs::IntegrityMode;
    use signal_hook::consts::signal::{SIGHUP, SIGINT, SIGTERM};
    use signal_hook::iterator::Signals;
    use std::collections::HashMap;
    use std::ffi::OsStr;
    use std::fmt::Write as _;
    use std::fs::{File, OpenOptions};
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};

    pub fn main() {
        if let Err(error) = run() {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }

    fn run() -> Result<(), Box<dyn std::error::Error>> {
        let arguments = parse_arguments()?;
        let store = required_path(&arguments, "store")?;
        let mount = required_path(&arguments, "mount")?;
        let spool = required_path(&arguments, "spool")?;
        let receipt = required_path(&arguments, "receipt")?;
        let paths = super::prepare_paths(&store, &mount, &spool, &receipt)?;
        let super::ValidatedPaths {
            store,
            mount,
            spool,
            receipt,
        } = paths;
        let ref_name = arguments.get("ref").map_or("main", String::as_str);
        let integrity = match arguments.get("integrity").map(String::as_str) {
            None | Some("trusted") => IntegrityMode::TrustedLocalDev,
            Some("verified") => IntegrityMode::Verified,
            Some(value) => return Err(format!("unsupported integrity mode {value}").into()),
        };
        let uid = number(&arguments, "uid", 0)?;
        let gid = number(&arguments, "gid", 0)?;
        let executable = std::env::current_exe()?;
        let executable_hash = hash_file(&executable)?;
        let workspace =
            MountedWorkspace::open(&store, ref_name, integrity, spool, executable_hash)?;
        let mut connections_high_water = workspace.active_store_connections()?;
        let fuse = LayerFuse::new(workspace, uid, gid);
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
        let session = fuser::spawn_mount(fuse, &mount, &config)?;
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
        println!(
            "{{\"backend\":\"layerfs-fuse\",\"mount\":\"{}\",\"integrity\":\"{}\",\"fs_bench_sha256\":\"{}\"}}",
            json(mount.to_string_lossy().as_ref()),
            match integrity {
                IntegrityMode::Verified => "Verified",
                IntegrityMode::TrustedLocalDev => "TrustedLocalDev",
            },
            FS_BENCH_SHA256,
        );
        std::io::stdout().flush()?;
        let mut signals = Signals::new([SIGTERM, SIGINT, SIGHUP])?;
        let signal = signals.forever().next().ok_or("signal iterator ended")?;
        byte_budget.close_and_wait()?;
        let shutdown = shared_workspace
            .lock()
            .map_err(|_| "workspace lock poisoned")?
            .shutdown();
        drop(session);
        drop(notifier_slot);
        let fuse = *shared_fuse_counters
            .lock()
            .map_err(|_| "counter lock poisoned")?;
        drop(shared_fuse_counters);
        let mut workspace = shared_workspace
            .lock()
            .map_err(|_| "workspace lock poisoned")?;
        let mounted = workspace.counters()?;
        let engine = workspace.engine_counters()?;
        let accepted = workspace.accepted().clone();
        let connections_before_drop = workspace.active_store_connections()?;
        connections_high_water = connections_high_water.max(connections_before_drop);
        workspace.close_store_connection()?;
        let connections_terminal = workspace.active_store_connections()?;
        drop(workspace);
        let status = if shutdown.is_ok() { "PASS" } else { "FAIL" };
        let body = format!(
        concat!(
            "{{\n",
            "  \"schema\": \"layerfs-fuse-terminal-v1\",\n",
            "  \"status\": \"{}\",\n",
            "  \"signal\": {},\n",
            "  \"backend\": \"layerfs-fuse\",\n",
            "  \"integrity\": \"{}\",\n",
            "  \"ref\": \"{}\",\n",
            "  \"generation\": {},\n",
            "  \"root\": \"{}\",\n",
            "  \"executable_blake3\": \"{}\",\n",
            "  \"fs_bench_sha256\": \"{}\",\n",
            "  \"callbacks\": {{\"lookup\":{},\"getattr\":{},\"create\":{},\"read\":{},\"write\":{},\"flush\":{},\"release\":{},\"fsync\":{},\"fsyncdir\":{},\"readdir\":{},\"mount_lock_wait_ns\":{},\"invalidations_requested\":{},\"invalidations_succeeded\":{},\"invalidations_failed\":{},\"invalidations_unsupported\":{}}},\n",
            "  \"mounted\": {{\"checkpoints\":{},\"no_op_checkpoints\":{},\"created_then_deleted\":{},\"splices\":{},\"lookup_refs\":{},\"lookup_refs_high_water\":{},\"live_nodes\":{},\"live_nodes_high_water\":{},\"open_handles\":{},\"open_handles_high_water\":{},\"pending_nodes\":{},\"pending_nodes_high_water\":{},\"dirty_nodes\":{},\"dirty_nodes_high_water\":{},\"dirty_ranges\":{},\"dirty_ranges_high_water\":{},\"directory_cursors\":{},\"directory_changes\":{},\"directory_changes_high_water\":{},\"inode_mappings\":{},\"inode_mappings_high_water\":{},\"logical_workspace_bytes\":{},\"logical_workspace_high_water_bytes\":{},\"spool_appended_bytes\":{},\"spool_live_bytes\":{},\"spool_live_high_water_bytes\":{},\"spool_dead_bytes\":{},\"spool_physical_bytes\":{},\"spool_physical_high_water_bytes\":{},\"spool_resets\":{},\"spool_compactions\":{},\"contiguous_file_builds\":{},\"largest_request_bytes\":{},\"operation_q_terminal_bytes\":{},\"operation_q_high_water_bytes\":{},\"materializations\":{},\"capture_scans\":{}}},\n",
            "  \"engine\": {{\"transactions_started\":{},\"transactions_committed\":{},\"transactions_rolled_back\":{},\"publication_commits\":{},\"objects_created\":{},\"objects_reused\":{},\"object_bytes_read\":{},\"object_bytes_written\":{},\"statements\":{},\"fetched_rows\":{},\"busy_events\":{},\"locked_events\":{},\"connection_mutex_wait_ns\":{},\"connections_high_water\":{},\"connections_before_drop\":{},\"connections_terminal\":{}}}\n",
            "}}\n"
        ),
        status,
        signal,
        match integrity {
            IntegrityMode::Verified => "Verified",
            IntegrityMode::TrustedLocalDev => "TrustedLocalDev",
        },
        json(&accepted.name),
        accepted.generation,
        accepted.root,
        hex(&executable_hash),
        FS_BENCH_SHA256,
        fuse.lookup,
        fuse.getattr,
        fuse.create,
        fuse.read,
        fuse.write,
        fuse.flush,
        fuse.release,
        fuse.fsync,
        fuse.fsyncdir,
        fuse.readdir,
        fuse.mount_lock_wait_ns,
        fuse.invalidations_requested,
        fuse.invalidations_succeeded,
        fuse.invalidations_failed,
        fuse.invalidations_unsupported,
        mounted.checkpoints,
        mounted.no_op_checkpoints,
        mounted.created_then_deleted,
        mounted.splices,
        mounted.lookup_refs,
        mounted.lookup_refs_high_water,
        mounted.live_nodes,
        mounted.live_nodes_high_water,
        mounted.open_handles,
        mounted.open_handles_high_water,
        mounted.pending_nodes,
        mounted.pending_nodes_high_water,
        mounted.dirty_nodes,
        mounted.dirty_nodes_high_water,
        mounted.dirty_ranges,
        mounted.dirty_ranges_high_water,
        mounted.directory_cursors,
        mounted.directory_changes,
        mounted.directory_changes_high_water,
        mounted.inode_mappings,
        mounted.inode_mappings_high_water,
        mounted.logical_workspace_bytes,
        mounted.logical_workspace_high_water_bytes,
        mounted.spool_appended_bytes,
        mounted.spool_live_bytes,
        mounted.spool_live_high_water_bytes,
        mounted.spool_dead_bytes,
        mounted.spool_physical_bytes,
        mounted.spool_physical_high_water_bytes,
        mounted.spool_resets,
        mounted.spool_compactions,
        mounted.contiguous_file_builds,
        mounted.largest_request_bytes,
        mounted.operation_q_current_bytes,
        mounted.operation_q_high_water_bytes,
        mounted.materializations,
        mounted.capture_scans,
        engine.transactions_started,
        engine.transactions_committed,
        engine.transactions_rolled_back,
        engine.publication_commits,
        engine.objects_created,
        engine.objects_reused,
        engine.object_bytes_read,
        engine.object_bytes_written,
        engine.statements,
        engine.fetched_rows,
        engine.busy_events,
        engine.locked_events,
        engine.connection_mutex_wait_ns,
        connections_high_water,
        connections_before_drop,
        connections_terminal,
    );
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(receipt)?;
        output.write_all(body.as_bytes())?;
        output.sync_all()?;
        shutdown.map(|_| ()).map_err(|error| error.into())
    }

    fn parse_arguments() -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
        let values = std::env::args().skip(1).collect::<Vec<_>>();
        if values.len() % 2 != 0 {
            return Err(usage().into());
        }
        let mut arguments = HashMap::new();
        for pair in values.chunks_exact(2) {
            let key = pair[0].strip_prefix("--").ok_or_else(usage)?;
            if arguments.insert(key.to_owned(), pair[1].clone()).is_some() {
                return Err(format!("duplicate --{key}").into());
            }
        }
        Ok(arguments)
    }

    fn usage() -> String {
        "usage: layerfs-fuse --store PATH --mount PATH --spool PATH --receipt PATH [--ref main] [--integrity trusted|verified] [--uid N] [--gid N]".to_owned()
    }

    fn required_path(
        arguments: &HashMap<String, String>,
        name: &str,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        arguments
            .get(name)
            .map(PathBuf::from)
            .ok_or_else(|| format!("missing --{name}").into())
    }

    fn number(
        arguments: &HashMap<String, String>,
        name: &str,
        default: u32,
    ) -> Result<u32, Box<dyn std::error::Error>> {
        arguments
            .get(name)
            .map(|value| value.parse().map_err(Into::into))
            .unwrap_or(Ok(default))
    }

    fn hash_file(path: &Path) -> Result<[u8; 32], Box<dyn std::error::Error>> {
        let mut file = File::open(path)?;
        let mut buffer = vec![0_u8; 1024 * 1024];
        let mut hasher = blake3::Hasher::new();
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                return Ok(*hasher.finalize().as_bytes());
            }
            hasher.update(&buffer[..read]);
        }
    }

    fn hex(bytes: &[u8]) -> String {
        let mut output = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            let _ = write!(output, "{byte:02x}");
        }
        output
    }

    fn json(value: &str) -> String {
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    }
}

#[cfg(target_os = "linux")]
fn main() {
    linux::main();
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("layerfs-fuse requires Linux");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_spool_and_receipt_are_distinct_and_outside_mount() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-fuse-paths-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mount = root.join("mount");
        let safe = prepare_paths(
            &root.join("store.sqlite"),
            &mount,
            &root.join("spool"),
            &root.join("receipt.json"),
        )
        .unwrap();
        assert!(!safe.store.starts_with(&safe.mount));
        assert!(!safe.spool.starts_with(&safe.mount));
        assert!(!safe.receipt.starts_with(&safe.mount));
        assert!(prepare_paths(
            &mount.join("store.sqlite"),
            &mount,
            &root.join("spool-2"),
            &root.join("receipt-2.json"),
        )
        .is_err());
        std::fs::write(root.join("existing-receipt.json"), b"existing").unwrap();
        assert!(prepare_paths(
            &root.join("store-4.sqlite"),
            &mount,
            &root.join("spool-4"),
            &root.join("existing-receipt.json"),
        )
        .is_err());
        std::fs::write(mount.join("occupied"), b"occupied").unwrap();
        assert!(prepare_paths(
            &root.join("store-5.sqlite"),
            &mount,
            &root.join("spool-5"),
            &root.join("receipt-5.json"),
        )
        .is_err());
        assert!(prepare_paths(
            &root.join("same"),
            &mount,
            &root.join("same"),
            &root.join("receipt-3.json"),
        )
        .is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
