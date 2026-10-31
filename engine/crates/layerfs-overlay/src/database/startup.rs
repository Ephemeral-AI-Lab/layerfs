//! Complete creation receipts, including failed finite schema/profile work.
use crate::{AllocationState, AllocationWork, DatabaseWork, Overlay, OverlayResult, ProfileConfig};
use rusqlite::{Connection, OpenFlags};
use std::{
    cell::{Cell, RefCell},
    fs::OpenOptions,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

static NEXT_ENGINE: AtomicU64 = AtomicU64::new(1);

/// One attempted startup and its exact original result. Failed artifacts stay
/// in caller custody; receipt observation never retries startup or removes them.
/// Linux verifies a read-only reopened descriptor of the fresh owned file before
/// its filesystem probe, reservation or SQLite open. Failure retains the artifact.
pub struct Creation {
    pub result: OverlayResult<Overlay>,
    pub work: CreationWork,
}
/// Finite startup costs before daemon readiness, independent of ordinary jobs.
#[derive(Debug, Default)]
pub struct CreationWork {
    pub file_create_calls: u64,
    /// Copies into fresh, independently writable files; never a hardlink.
    pub file_copy_calls: u64,
    pub copied_bytes: u64,
    /// Actual Linux read-only O_NOFOLLOW opens of the newly created file for
    /// filesystem identification. Zero on other platforms or creation failure.
    pub filesystem_open_calls: u64,
    /// Actual descriptor metadata attempts comparing the original and reopened
    /// regular, singly linked file identities. At most two on Linux; zero elsewhere.
    pub filesystem_identity_calls: u64,
    /// Actual Linux fstatfs attempts on the identity-verified reopened descriptor.
    /// Zero on other platforms or when preceding creation/identity work failed.
    pub filesystem_probe_calls: u64,
    /// Original successful Linux fstatfs type from the verified reopened
    /// descriptor, represented in signed 64 bits.
    /// None means no successful Linux observation; a failed probe's original
    /// I/O error remains in Creation::result. A type is not a qualification.
    pub linux_filesystem_type: Option<i64>,
    pub sqlite_open_calls: u64,
    pub connection_configuration_calls: u64,
    pub cache_configuration_calls: u64,
    pub sql: DatabaseWork,
    pub allocation: AllocationWork,
    /// A separate actual metadata observation, including its original error.
    /// It is absent if no allocation owner was established. It does not replace
    /// the original creation outcome or establish pager/process residency.
    pub allocation_state: Option<OverlayResult<AllocationState>>,
    pub elapsed_ns: u64,
}

pub(crate) fn create(path: &Path, config: ProfileConfig) -> Creation {
    initialize(path, config, None)
}

pub(crate) fn copy_template(path: &Path, config: ProfileConfig, template: &Path) -> Creation {
    initialize(path, config, Some(template))
}

fn initialize(path: &Path, config: ProfileConfig, template: Option<&Path>) -> Creation {
    let start = Instant::now();
    let sql = RefCell::new(DatabaseWork::default());
    let mut work = CreationWork::default();
    let mut allocation = None;
    let result = (|| {
        if !cfg!(any(
            target_os = "macos",
            target_os = "linux",
            target_os = "windows"
        )) {
            return Err(crate::OverlayError::UnsupportedPlatform);
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        work.file_create_calls = 1;
        let mut file = options.open(path)?;
        if let Some(template) = template {
            let mut source = std::fs::File::open(template)?;
            work.file_copy_calls = 1;
            work.copied_bytes = std::io::copy(&mut source, &mut file)?;
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
            // Receipt 13: fresh create descriptors on the evidenced host share
            // report generic FUSE. Identify the same inode through one planned open.
            work.filesystem_open_calls = 1;
            let probe = OpenOptions::new()
                .read(true)
                .custom_flags(nix::libc::O_NOFOLLOW)
                .open(path)?;
            work.filesystem_identity_calls = 1;
            let original = file.metadata()?;
            work.filesystem_identity_calls = 2;
            let reopened = probe.metadata()?;
            if !original.is_file()
                || !reopened.is_file()
                || original.nlink() != 1
                || reopened.nlink() != 1
                || original.dev() != reopened.dev()
                || original.ino() != reopened.ino()
            {
                return Err(crate::OverlayError::Invalid(
                    "filesystem probe file identity",
                ));
            }
            work.filesystem_probe_calls = 1;
            let filesystem = nix::sys::statfs::fstatfs(&probe)
                .map_err(|error| std::io::Error::from_raw_os_error(error as i32))?;
            // nix's Linux fsword type varies by ABI; preserve its signed bits.
            #[allow(clippy::unnecessary_cast)]
            let linux_magic = filesystem.filesystem_type().0 as i64;
            work.linux_filesystem_type = Some(linux_magic);
        }
        allocation = Some(super::physical::Allocation::new(file, path)?);
        work.sqlite_open_calls = 1;
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        let mut profile = super::profile::initialize(
            &connection,
            config,
            &sql,
            &mut work.connection_configuration_calls,
        )?;
        if template.is_none() {
            crate::diagnostics::startup::batch(
                &connection,
                &sql,
                include_str!("../../sql/schema.sql"),
            )?;
        }
        profile.schema_version =
            super::profile::readback(&connection, &sql, "PRAGMA user_version", |row| row.get(0))?;
        let application: i64 =
            super::profile::readback(&connection, &sql, "PRAGMA application_id", |row| row.get(0))?;
        if profile.schema_version != 39 || application != 1279676210 {
            return Err(crate::OverlayError::Invalid("overlay schema readback"));
        }
        if template.is_some() {
            let populated: bool = super::profile::readback(
                &connection,
                &sql,
                "SELECT EXISTS(SELECT 1 FROM fs_inode) OR EXISTS(SELECT 1 FROM fs_entry)
                 OR EXISTS(SELECT 1 FROM fs_data) OR EXISTS(SELECT 1 FROM fs_data_ref)
                 OR EXISTS(SELECT 1 FROM fs_truncate) OR EXISTS(SELECT 1 FROM commit_record)
                 OR EXISTS(SELECT 1 FROM fs_digest) OR EXISTS(SELECT 1 FROM fs_predecessor)
                 OR EXISTS(SELECT 1 FROM cleanup_task)",
                |row| row.get(0),
            )?;
            if populated {
                return Err(crate::OverlayError::Invalid("populated overlay template"));
            }
        }
        work.cache_configuration_calls = 1;
        // The capacity covers the engine's fixed statement set, about 224
        // literal texts, so no statement of a steady request cycle is prepared
        // twice. The set is fixed by source; it does not grow with files,
        // bytes or operations.
        connection.set_prepared_statement_cache_capacity(256);
        let identity = NEXT_ENGINE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .map_err(|_| crate::OverlayError::Invalid("engine identity exhausted"))?;
        Ok(Overlay {
            identity,
            connection,
            open: RefCell::new(crate::open::OpenState::default()),
            work: RefCell::new(*sql.borrow()),
            payload_work: Cell::new(crate::PayloadWork::default()),
            quarantined: Cell::new(false),
            workspace: Cell::new(None),
            maintenance_ready: Cell::new(false),
            orphan_seen: Cell::new(false),
            pieces_seen: Cell::new(false),
            release_step: Cell::new(false),
            transaction: Cell::new(super::connection::Transaction::None),
            tickets: std::sync::Arc::new(crate::ReplyTickets::new(identity)),
            issued: Default::default(),
            profile,
            allocation: allocation.take().expect("allocation created"),
        })
    })();
    let physical = result
        .as_ref()
        .ok()
        .map(|db| &db.allocation)
        .or(allocation.as_ref());
    if let Some(physical) = physical {
        work.allocation_state = Some(physical.state());
        work.allocation = physical.work();
    }
    work.sql = *sql.borrow();
    work.elapsed_ns = start.elapsed().as_nanos().min(u64::MAX as u128) as u64;
    Creation { result, work }
}
