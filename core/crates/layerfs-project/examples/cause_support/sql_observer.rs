//! Diagnostic-only public SQLite trace API. VM status is reset by this observer;
//! native SqlWork.vm_steps is therefore NOT qualified in this diagnostic.
//! No SQL state, VFS, profile, statement or algorithm is replaced.
use std::{
    collections::BTreeMap,
    ffi::{c_char, c_int, c_uint, c_void, CStr},
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Mutex,
    },
    time::Instant,
};
#[link(name = "sqlite3")]
extern "C" {
    fn sqlite3_config(option: c_int, ...) -> c_int;
    fn sqlite3_trace_v2(
        db: *mut c_void,
        mask: c_uint,
        callback: Option<
            unsafe extern "C" fn(c_uint, *mut c_void, *mut c_void, *mut c_void) -> c_int,
        >,
        context: *mut c_void,
    ) -> c_int;
    fn sqlite3_sql(stmt: *mut c_void) -> *const c_char;
    fn sqlite3_stmt_status(stmt: *mut c_void, status: c_int, reset: c_int) -> c_int;
    fn sqlite3_db_filename(db: *mut c_void, name: *const c_char) -> *const c_char;
    fn sqlite3_libversion() -> *const c_char;
}
const SQLLOG: c_int = 21;
const TRACE_STMT: c_uint = 1;
const TRACE_PROFILE: c_uint = 2;
const VM_STEP: c_int = 4;
const N: usize = 12;
const DBS: usize = 3;
static CALLS: [AtomicU64; N * DBS] = [const { AtomicU64::new(0) }; N * DBS];
static WALL: [AtomicU64; N * DBS] = [const { AtomicU64::new(0) }; N * DBS];
static VM: [AtomicU64; N * DBS] = [const { AtomicU64::new(0) }; N * DBS];
static RAW: [AtomicU64; N * DBS] = [const { AtomicU64::new(0) }; N * DBS];
static PROFILE_ACTIVE: AtomicUsize = AtomicUsize::new(0);
#[cfg(target_os = "macos")]
extern "C" {
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
}
static ERRORS: AtomicU64 = AtomicU64::new(0);
static CONNECTIONS: AtomicU64 = AtomicU64::new(0);
static ACTIVE: Mutex<BTreeMap<usize, (Instant, usize)>> = Mutex::new(BTreeMap::new());
fn category(sql: &[u8]) -> usize {
    let sql = sql.trim_ascii_start();
    if sql.starts_with(b"BEGIN IMMEDIATE") {
        0
    } else if sql.starts_with(b"BEGIN") {
        1
    } else if sql.starts_with(b"COMMIT") {
        2
    } else if sql.starts_with(b"ROLLBACK") {
        3
    } else if sql.starts_with(b"SELECT") {
        4
    } else if sql.starts_with(b"INSERT") {
        5
    } else if sql.starts_with(b"UPDATE") {
        6
    } else if sql.starts_with(b"DELETE") {
        7
    } else if sql.starts_with(b"PRAGMA") {
        8
    } else if sql.starts_with(b"CREATE") {
        9
    } else if sql.starts_with(b"WITH") {
        10
    } else {
        11
    }
}
unsafe extern "C" fn trace(
    event: c_uint,
    context: *mut c_void,
    stmt: *mut c_void,
    duration: *mut c_void,
) -> c_int {
    let pointer = PROFILE_ACTIVE.load(Ordering::Relaxed);
    if pointer != 0 {
        // SAFETY: the explicitly selected diagnostic library exports this fixed
        // signature and remains loaded. Its thread-local guard covers both events.
        let active: unsafe extern "C" fn() -> c_int = unsafe { std::mem::transmute(pointer) };
        if unsafe { active() } != 0 {
            return 0;
        }
    }
    if event == TRACE_STMT {
        // SAFETY: SQLite owns the SQL/connection strings for this callback's lifetime.
        let sql = unsafe { sqlite3_sql(stmt) };
        let file = unsafe { sqlite3_db_filename(context, c"main".as_ptr()) };
        if sql.is_null() {
            ERRORS.fetch_add(1, Ordering::Relaxed);
            return 0;
        }
        let bytes = unsafe { CStr::from_ptr(sql) }.to_bytes();
        let name = if file.is_null() {
            &[][..]
        } else {
            unsafe { CStr::from_ptr(file) }.to_bytes()
        };
        let db = if name.ends_with(b".history.sqlite") {
            1
        } else if name.ends_with(b"store.sqlite") {
            0
        } else {
            2
        };
        if let Ok(mut active) = ACTIVE.lock() {
            if active.len() == 32 && !active.contains_key(&(stmt as usize)) {
                ERRORS.fetch_add(1, Ordering::Relaxed);
            } else {
                active
                    .entry(stmt as usize)
                    .or_insert((Instant::now(), db * N + category(bytes)));
            }
        } else {
            ERRORS.fetch_add(1, Ordering::Relaxed);
        }
    } else if event == TRACE_PROFILE {
        // SAFETY: PROFILE supplies a live statement and sqlite3_uint64 duration.
        let steps = unsafe { sqlite3_stmt_status(stmt, VM_STEP, 1) };
        let raw = if duration.is_null() {
            0
        } else {
            unsafe { *(duration as *const u64) }
        };
        if let Ok(mut active) = ACTIVE.lock() {
            if let Some((start, i)) = active.remove(&(stmt as usize)) {
                CALLS[i].fetch_add(1, Ordering::Relaxed);
                WALL[i].fetch_add(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
                VM[i].fetch_add(steps.max(0) as u64, Ordering::Relaxed);
                RAW[i].fetch_add(raw, Ordering::Relaxed);
            } else {
                ERRORS.fetch_add(1, Ordering::Relaxed);
            }
        } else {
            ERRORS.fetch_add(1, Ordering::Relaxed);
        }
    }
    0
}
unsafe extern "C" fn opened(
    _context: *mut c_void,
    db: *mut c_void,
    _sql: *const c_char,
    event: c_int,
) {
    if event == 0 {
        CONNECTIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: SQLLOG event0 supplies the newly opened live connection. The callback
        // and static observer state outlive every diagnostic connection.
        if unsafe { sqlite3_trace_v2(db, TRACE_STMT | TRACE_PROFILE, Some(trace), db) } != 0 {
            ERRORS.fetch_add(1, Ordering::Relaxed);
        }
    }
}
pub fn initialize() -> Result<(), String> {
    if std::env::var_os("LAYERFS_CAUSE_MEMORY_ARM").is_some() {
        #[cfg(target_os = "macos")]
        {
            // SAFETY: explicit injected profile library, resolved before SQLite opens.
            let pointer = unsafe {
                dlsym(
                    (-2isize) as *mut c_void,
                    c"cause_memory_profile_active".as_ptr(),
                )
            };
            if pointer.is_null() {
                return Err("memory diagnostic guard symbol unavailable".into());
            }
            PROFILE_ACTIVE.store(pointer as usize, Ordering::Relaxed);
        }
        #[cfg(not(target_os = "macos"))]
        return Err("memory profile observer unavailable on this platform".into());
    }
    // SAFETY: called once before the diagnostic opens any SQLite connection;
    // SQLLOG's documented callback signature and process-lifetime state are used.
    let code = unsafe {
        sqlite3_config(
            SQLLOG,
            opened as unsafe extern "C" fn(*mut c_void, *mut c_void, *const c_char, c_int),
            std::ptr::null_mut::<c_void>(),
        )
    };
    if code != 0 {
        return Err(format!("SQLite SQLLOG observer unavailable: {code}"));
    }
    Ok(())
}
pub fn json() -> String {
    let categories = [
        "begin_write",
        "begin_read",
        "commit",
        "rollback",
        "select",
        "insert",
        "update",
        "delete",
        "pragma",
        "create",
        "with",
        "other",
    ];
    let databases = ["store", "history", "other"];
    let rows=databases.iter().enumerate().flat_map(|(db,name)|categories.iter().enumerate().map(move |(kind,category)|{let i=db*N+kind;format!("{{\"database\":\"{name}\",\"category\":\"{category}\",\"calls\":{},\"statement_lifetime_ns\":{},\"vm_steps\":{},\"sqlite_profile_raw_ns\":{}}}",CALLS[i].load(Ordering::Relaxed),WALL[i].load(Ordering::Relaxed),VM[i].load(Ordering::Relaxed),RAW[i].load(Ordering::Relaxed))})).collect::<Vec<_>>().join(",");
    let active = ACTIVE.lock().map(|x| x.len()).unwrap_or(usize::MAX);
    // SAFETY: sqlite3_libversion returns a process-lifetime NUL-terminated string.
    let version = unsafe { CStr::from_ptr(sqlite3_libversion()) }.to_string_lossy();
    format!("{{\"version\":\"{version}\",\"connections\":{},\"errors\":{},\"active_statements\":{},\"native_vm_counter_qualified\":false,\"vm_status_reset\":true,\"rows\":[{}]}}",CONNECTIONS.load(Ordering::Relaxed),ERRORS.load(Ordering::Relaxed),active,rows)
}
