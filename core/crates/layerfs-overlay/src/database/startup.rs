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
pub struct Creation {
    pub result: OverlayResult<Overlay>,
    pub work: CreationWork,
}
/// Finite startup costs before daemon readiness, independent of ordinary jobs.
#[derive(Debug, Default)]
pub struct CreationWork {
    pub file_create_calls: u64,
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
    let start = Instant::now();
    let sql = RefCell::new(DatabaseWork::default());
    let mut work = CreationWork::default();
    let mut allocation = None;
    let result = (|| {
        if !cfg!(any(target_os = "macos", target_os = "linux")) {
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
        let file = options.open(path)?;
        allocation = Some(super::allocation::Allocation::new(file, path)?);
        allocation
            .as_ref()
            .expect("allocation created")
            .admit(false, 0)?;
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
        crate::diagnostics::startup::batch(
            &connection,
            &sql,
            include_str!("../../sql/schema.sql"),
        )?;
        crate::diagnostics::startup::batch(
            &connection,
            &sql,
            include_str!("../../sql/accounting.sql"),
        )?;
        profile.schema_version =
            super::profile::readback(&connection, &sql, "PRAGMA user_version", |row| row.get(0))?;
        let application: i64 =
            super::profile::readback(&connection, &sql, "PRAGMA application_id", |row| row.get(0))?;
        if profile.schema_version != 14 || application != 1279676210 {
            return Err(crate::OverlayError::Invalid("overlay schema readback"));
        }
        work.cache_configuration_calls = 1;
        connection.set_prepared_statement_cache_capacity(48);
        let identity = NEXT_ENGINE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .map_err(|_| crate::OverlayError::Invalid("engine identity exhausted"))?;
        Ok(Overlay {
            identity,
            connection,
            work: RefCell::new(*sql.borrow()),
            payload_work: Cell::new(crate::PayloadWork::default()),
            quarantined: Cell::new(false),
            maintenance_ready: Cell::new(false),
            closed_ready: Cell::new(false),
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
