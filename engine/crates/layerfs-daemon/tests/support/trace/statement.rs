//! Names of the statements an engine connection runs, for count anatomy.
//!
//! The engine's own diagnostics count statements by family. This observer
//! adds the template of each one: `sqlite3_trace_v2(SQLITE_TRACE_PROFILE)` is
//! installed on the connection whose file a test registered, and the callback
//! records the statement's SQL and the VM steps of that run. It changes no
//! statement, plan or counter of the product; the product's `attempts` stay
//! the count, and a test checks that the two agree. Nothing here is a product
//! interface.
//!
//! The connection is reached where it is opened. With a statically linked
//! SQLite that is a process-wide automatic extension. The macOS system
//! library refuses automatic extensions, so there this test binary defines
//! `sqlite3_open_v2` itself: the static linker binds the engine's call to it,
//! and it forwards to the library's own function.
#[cfg(not(target_os = "macos"))]
use std::sync::Once;
use std::{
    collections::HashMap,
    ffi::CStr,
    os::raw::{c_char, c_int, c_uint, c_void},
    sync::{Arc, Mutex},
};

extern "C" {
    #[cfg(not(target_os = "macos"))]
    fn sqlite3_auto_extension(entry: Option<unsafe extern "C" fn()>) -> c_int;
    fn sqlite3_trace_v2(
        db: *mut c_void,
        mask: c_uint,
        callback: Option<
            unsafe extern "C" fn(c_uint, *mut c_void, *mut c_void, *mut c_void) -> c_int,
        >,
        context: *mut c_void,
    ) -> c_int;
    fn sqlite3_sql(statement: *mut c_void) -> *const c_char;
    fn sqlite3_stmt_status(statement: *mut c_void, counter: c_int, reset: c_int) -> c_int;
    fn sqlite3_db_filename(db: *mut c_void, name: *const c_char) -> *const c_char;
    fn sqlite3_libversion() -> *const c_char;
}
const TRACE_PROFILE: c_uint = 0x02;
const STATUS_VM_STEP: c_int = 4;

/// One finished statement: its template and the VM steps of that run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Run {
    pub sql: u32,
    pub vm_steps: u64,
    /// Whether it ran on the thread that watches the connection.
    pub own: bool,
}
/// The statements of one connection since the last `take`, in order.
pub struct Trace {
    watcher: std::thread::ThreadId,
    state: Mutex<State>,
}
#[derive(Default)]
struct State {
    templates: Vec<String>,
    index: HashMap<String, u32>,
    runs: Vec<Run>,
}
impl Trace {
    pub fn take(&self) -> Vec<Run> {
        std::mem::take(&mut self.state.lock().unwrap().runs)
    }
    pub fn template(&self, sql: u32) -> String {
        self.state.lock().unwrap().templates[sql as usize].clone()
    }
}
/// Files whose connection is observed, by the end of their path.
static WATCHED: Mutex<Vec<(String, Arc<Trace>)>> = Mutex::new(Vec::new());
#[cfg(not(target_os = "macos"))]
static INSTALL: Once = Once::new();

/// Observes the connection that will be opened on a file whose path ends
/// with `suffix`, for the calling thread. Call before that connection is
/// opened.
pub fn watch(suffix: &str) -> Arc<Trace> {
    #[cfg(not(target_os = "macos"))]
    INSTALL.call_once(|| {
        type Entry = unsafe extern "C" fn(*mut c_void, *mut *mut c_char, *const c_void) -> c_int;
        // SAFETY: SQLite calls an automatic extension with exactly the
        // arguments of `Entry`; the declared argument-less type is the
        // registration interface's own spelling of it.
        let status = unsafe {
            sqlite3_auto_extension(Some(std::mem::transmute::<Entry, unsafe extern "C" fn()>(
                opened,
            )))
        };
        assert_eq!(status, 0, "automatic extension registration");
    });
    let trace = Arc::new(Trace {
        watcher: std::thread::current().id(),
        state: Mutex::default(),
    });
    WATCHED
        .lock()
        .unwrap()
        .push((suffix.to_owned(), trace.clone()));
    trace
}
pub fn sqlite_version() -> String {
    // SAFETY: a static NUL-terminated string of the linked library.
    unsafe { CStr::from_ptr(sqlite3_libversion()) }
        .to_string_lossy()
        .into_owned()
}
/// The engine's `sqlite3_open_v2`, bound here by the static linker: the
/// library's own function opens the connection, which is then observed.
#[cfg(target_os = "macos")]
#[no_mangle]
pub unsafe extern "C" fn sqlite3_open_v2(
    filename: *const c_char,
    db: *mut *mut c_void,
    flags: c_int,
    vfs: *const c_char,
) -> c_int {
    type Open =
        unsafe extern "C" fn(*const c_char, *mut *mut c_void, c_int, *const c_char) -> c_int;
    // SAFETY: the next definition of this symbol is the system library's,
    // which has this signature; it is called with the caller's arguments.
    let status = unsafe {
        let library = nix::libc::dlsym(nix::libc::RTLD_NEXT, c"sqlite3_open_v2".as_ptr());
        assert!(!library.is_null(), "the library's sqlite3_open_v2");
        std::mem::transmute::<*mut c_void, Open>(library)(filename, db, flags, vfs)
    };
    if status == 0 {
        // SAFETY: a successful open stored the connection.
        unsafe { opened(*db, std::ptr::null_mut(), std::ptr::null()) };
    }
    status
}
unsafe extern "C" fn opened(db: *mut c_void, _: *mut *mut c_char, _: *const c_void) -> c_int {
    // SAFETY: `db` is the connection being opened; its main file name is a
    // NUL-terminated string or null for a temporary database.
    let name = unsafe { sqlite3_db_filename(db, c"main".as_ptr()) };
    if name.is_null() {
        return 0;
    }
    let name = unsafe { CStr::from_ptr(name) }.to_string_lossy();
    let watched = WATCHED.lock().unwrap();
    if let Some((_, trace)) = watched.iter().find(|(suffix, _)| name.ends_with(suffix)) {
        // The connection keeps one reference for the rest of the process.
        let context = Arc::into_raw(trace.clone()) as *mut c_void;
        // SAFETY: the callback and its context outlive the connection.
        unsafe { sqlite3_trace_v2(db, TRACE_PROFILE, Some(finished), context) };
    }
    0
}
unsafe extern "C" fn finished(
    _: c_uint,
    context: *mut c_void,
    statement: *mut c_void,
    _: *mut c_void,
) -> c_int {
    // SAFETY: `context` is the leaked `Arc<Trace>` of `opened`; `statement`
    // is the prepared statement that just finished, on the calling thread.
    let trace = unsafe { &*(context as *const Trace) };
    let sql = unsafe { sqlite3_sql(statement) };
    let steps = unsafe { sqlite3_stmt_status(statement, STATUS_VM_STEP, 0) };
    if sql.is_null() {
        return 0;
    }
    let sql = unsafe { CStr::from_ptr(sql) }.to_string_lossy();
    let mut state = trace.state.lock().unwrap();
    let id = match state.index.get(sql.as_ref()) {
        Some(id) => *id,
        None => {
            let id = state.templates.len() as u32;
            state.templates.push(sql.to_string());
            state.index.insert(sql.into_owned(), id);
            id
        }
    };
    state.runs.push(Run {
        sql: id,
        vm_steps: steps.max(0) as u64,
        own: std::thread::current().id() == trace.watcher,
    });
    0
}
