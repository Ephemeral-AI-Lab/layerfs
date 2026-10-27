//! Public Disposable startup/seal must never synchronize its backing files.
//! The observer delegates the public SQLite VFS ABI, preserving every callback.
#![cfg(any(target_os = "macos", target_os = "linux"))]

use layerfs_content::{FinalizedObject, ObjectRole};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{
    Handles, PersistenceConfig, SqliteAcquisitionSchema, SqlitePackLayout, SqlitePersistenceProfile,
};
use layerfs_storage::{Storage, StoragePolicy};
use rusqlite::ffi;
use std::{
    ffi::{c_char, c_int, c_void, CStr, CString},
    fs,
    mem::{align_of, size_of},
    path::PathBuf,
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};

struct Counters {
    opens: [AtomicU64; 4],
    writes: [AtomicU64; 4],
    syncs: [AtomicU64; 4],
    closes: [AtomicU64; 4],
    sync_flags: AtomicU64,
    errors: AtomicU64,
}
impl Counters {
    fn new() -> Self {
        Self {
            opens: std::array::from_fn(|_| AtomicU64::new(0)),
            writes: std::array::from_fn(|_| AtomicU64::new(0)),
            syncs: std::array::from_fn(|_| AtomicU64::new(0)),
            closes: std::array::from_fn(|_| AtomicU64::new(0)),
            sync_flags: AtomicU64::new(0),
            errors: AtomicU64::new(0),
        }
    }
    fn reset(&self) {
        for counters in [&self.opens, &self.writes, &self.syncs, &self.closes] {
            for value in counters {
                value.store(0, Ordering::Relaxed);
            }
        }
        self.sync_flags.store(0, Ordering::Relaxed);
        self.errors.store(0, Ordering::Relaxed);
    }
    fn report(&self, phase: &str) -> u64 {
        let values = |c: &[AtomicU64; 4]| c.each_ref().map(|v| v.load(Ordering::Relaxed));
        let syncs = values(&self.syncs);
        eprintln!(
            "DISPOSABLE_VFS phase={phase} classes=main,wal,journal,other opens={:?} writes={:?} syncs={syncs:?} closes={:?} flags={} errors={}",
            values(&self.opens), values(&self.writes), values(&self.closes),
            self.sync_flags.load(Ordering::Relaxed), self.errors.load(Ordering::Relaxed)
        );
        syncs.iter().sum()
    }
}
struct Context {
    original: *mut ffi::sqlite3_vfs,
    counts: Counters,
}
#[repr(C)]
struct ObservedFile {
    base: ffi::sqlite3_file,
    inner: *mut ffi::sqlite3_file,
    context: *const Context,
    kind: usize,
    methods: ffi::sqlite3_io_methods,
}
fn offset() -> usize {
    size_of::<ObservedFile>().next_multiple_of(align_of::<u128>())
}
unsafe fn file(p: *mut ffi::sqlite3_file) -> &'static mut ObservedFile {
    &mut *p.cast::<ObservedFile>()
}
unsafe fn original(v: *mut ffi::sqlite3_vfs) -> *mut ffi::sqlite3_vfs {
    (*(*v).pAppData.cast::<Context>()).original
}

macro_rules! file_delegate {
    ($name:ident, $field:ident, ($($arg:ident: $ty:ty),*) -> $ret:ty) => {
        unsafe extern "C" fn $name(p: *mut ffi::sqlite3_file, $($arg: $ty),*) -> $ret {
            let f = file(p);
            ((*(*f.inner).pMethods).$field.unwrap())(f.inner, $($arg),*)
        }
    };
}
file_delegate!(read_file, xRead, (b: *mut c_void, n: c_int, at: i64) -> c_int);
file_delegate!(truncate_file, xTruncate, (n: i64) -> c_int);
file_delegate!(size_file, xFileSize, (n: *mut i64) -> c_int);
file_delegate!(lock_file, xLock, (n: c_int) -> c_int);
file_delegate!(unlock_file, xUnlock, (n: c_int) -> c_int);
file_delegate!(reserved_file, xCheckReservedLock, (n: *mut c_int) -> c_int);
file_delegate!(control_file, xFileControl, (n: c_int, b: *mut c_void) -> c_int);
file_delegate!(sector_file, xSectorSize, () -> c_int);
file_delegate!(device_file, xDeviceCharacteristics, () -> c_int);
file_delegate!(map_file, xShmMap, (a: c_int, b: c_int, c: c_int, d: *mut *mut c_void) -> c_int);
file_delegate!(shm_lock, xShmLock, (a: c_int, b: c_int, c: c_int) -> c_int);
file_delegate!(barrier_file, xShmBarrier, () -> ());
file_delegate!(unmap_file, xShmUnmap, (a: c_int) -> c_int);
file_delegate!(fetch_file, xFetch, (a: i64, b: c_int, c: *mut *mut c_void) -> c_int);
file_delegate!(unfetch_file, xUnfetch, (a: i64, b: *mut c_void) -> c_int);

unsafe extern "C" fn close_file(p: *mut ffi::sqlite3_file) -> c_int {
    let f = file(p);
    let result = ((*(*f.inner).pMethods).xClose.unwrap())(f.inner);
    (*f.context).counts.closes[f.kind].fetch_add(1, Ordering::Relaxed);
    if result != ffi::SQLITE_OK {
        (*f.context).counts.errors.fetch_add(1, Ordering::Relaxed);
    }
    f.base.pMethods = ptr::null();
    result
}
unsafe extern "C" fn write_file(
    p: *mut ffi::sqlite3_file,
    b: *const c_void,
    n: c_int,
    at: i64,
) -> c_int {
    let f = file(p);
    (*f.context).counts.writes[f.kind].fetch_add(1, Ordering::Relaxed);
    ((*(*f.inner).pMethods).xWrite.unwrap())(f.inner, b, n, at)
}
unsafe extern "C" fn sync_file(p: *mut ffi::sqlite3_file, flags: c_int) -> c_int {
    let f = file(p);
    (*f.context).counts.syncs[f.kind].fetch_add(1, Ordering::Relaxed);
    (*f.context)
        .counts
        .sync_flags
        .fetch_or(flags as u64, Ordering::Relaxed);
    let result = ((*(*f.inner).pMethods).xSync.unwrap())(f.inner, flags);
    if result != ffi::SQLITE_OK {
        (*f.context).counts.errors.fetch_add(1, Ordering::Relaxed);
    }
    result
}
unsafe extern "C" fn open_file(
    v: *mut ffi::sqlite3_vfs,
    name: *const c_char,
    p: *mut ffi::sqlite3_file,
    flags: c_int,
    out: *mut c_int,
) -> c_int {
    let context = (*v).pAppData.cast::<Context>();
    let f = file(p);
    f.inner = p.cast::<u8>().add(offset()).cast();
    f.context = context;
    f.kind = if flags & ffi::SQLITE_OPEN_WAL != 0 {
        1
    } else if flags & ffi::SQLITE_OPEN_MAIN_DB != 0 {
        0
    } else if flags & ffi::SQLITE_OPEN_MAIN_JOURNAL != 0 {
        2
    } else {
        3
    };
    let prior = (*context).original;
    let result = ((*prior).xOpen.unwrap())(prior, name, f.inner, flags, out);
    if !(*f.inner).pMethods.is_null() {
        let m = &*(*f.inner).pMethods;
        if !(1..=3).contains(&m.iVersion) {
            (m.xClose.unwrap())(f.inner);
            return ffi::SQLITE_CANTOPEN;
        }
        f.methods = std::mem::zeroed();
        f.methods.iVersion = m.iVersion;
        macro_rules! wrap {
            ($field:ident, $callback:ident) => {
                f.methods.$field = m.$field.map(|_| $callback as _);
            };
        }
        wrap!(xClose, close_file);
        wrap!(xRead, read_file);
        wrap!(xWrite, write_file);
        wrap!(xTruncate, truncate_file);
        wrap!(xSync, sync_file);
        wrap!(xFileSize, size_file);
        wrap!(xLock, lock_file);
        wrap!(xUnlock, unlock_file);
        wrap!(xCheckReservedLock, reserved_file);
        wrap!(xFileControl, control_file);
        wrap!(xSectorSize, sector_file);
        wrap!(xDeviceCharacteristics, device_file);
        if m.iVersion >= 2 {
            wrap!(xShmMap, map_file);
            wrap!(xShmLock, shm_lock);
            wrap!(xShmBarrier, barrier_file);
            wrap!(xShmUnmap, unmap_file);
        }
        if m.iVersion >= 3 {
            wrap!(xFetch, fetch_file);
            wrap!(xUnfetch, unfetch_file);
        }
        f.base.pMethods = &f.methods;
        (*context).counts.opens[f.kind].fetch_add(1, Ordering::Relaxed);
    }
    result
}
macro_rules! vfs_delegate {
    ($name:ident, $field:ident, ($($arg:ident: $ty:ty),*) -> $ret:ty) => {
        unsafe extern "C" fn $name(v: *mut ffi::sqlite3_vfs, $($arg: $ty),*) -> $ret {
            let prior = original(v);
            ((*prior).$field.unwrap())(prior, $($arg),*)
        }
    };
}
type DlSymbol = Option<unsafe extern "C" fn(*mut ffi::sqlite3_vfs, *mut c_void, *const c_char)>;
vfs_delegate!(delete_vfs, xDelete, (n: *const c_char, s: c_int) -> c_int);
vfs_delegate!(access_vfs, xAccess, (n: *const c_char, f: c_int, r: *mut c_int) -> c_int);
vfs_delegate!(path_vfs, xFullPathname, (n: *const c_char, s: c_int, r: *mut c_char) -> c_int);
vfs_delegate!(dlopen_vfs, xDlOpen, (n: *const c_char) -> *mut c_void);
vfs_delegate!(dlerror_vfs, xDlError, (n: c_int, r: *mut c_char) -> ());
vfs_delegate!(dlsym_vfs, xDlSym, (h: *mut c_void, n: *const c_char) -> DlSymbol);
vfs_delegate!(dlclose_vfs, xDlClose, (h: *mut c_void) -> ());
vfs_delegate!(random_vfs, xRandomness, (n: c_int, r: *mut c_char) -> c_int);
vfs_delegate!(sleep_vfs, xSleep, (n: c_int) -> c_int);
vfs_delegate!(time_vfs, xCurrentTime, (r: *mut f64) -> c_int);
vfs_delegate!(error_vfs, xGetLastError, (n: c_int, r: *mut c_char) -> c_int);
vfs_delegate!(time64_vfs, xCurrentTimeInt64, (r: *mut i64) -> c_int);
vfs_delegate!(setcall_vfs, xSetSystemCall, (n: *const c_char, p: ffi::sqlite3_syscall_ptr) -> c_int);
vfs_delegate!(getcall_vfs, xGetSystemCall, (n: *const c_char) -> ffi::sqlite3_syscall_ptr);
vfs_delegate!(nextcall_vfs, xNextSystemCall, (n: *const c_char) -> *const c_char);

struct Observer {
    proxy: Box<ffi::sqlite3_vfs>,
    context: Box<Context>,
}
impl Observer {
    fn new() -> Self {
        unsafe {
            let prior = ffi::sqlite3_vfs_find(ptr::null());
            assert!(!prior.is_null());
            assert!((1..=3).contains(&(*prior).iVersion));
            let mut context = Box::new(Context {
                original: prior,
                counts: Counters::new(),
            });
            let mut proxy = Box::new(*prior);
            proxy.zName = c"layerfs-disposable-sync-observer".as_ptr();
            proxy.pNext = ptr::null_mut();
            proxy.pAppData = (&mut *context as *mut Context).cast();
            proxy.szOsFile = proxy.szOsFile.checked_add(offset() as c_int).unwrap();
            proxy.xOpen = Some(open_file);
            macro_rules! wrap {
                ($field:ident, $callback:ident) => {
                    proxy.$field = (*prior).$field.map(|_| $callback as _);
                };
            }
            wrap!(xDelete, delete_vfs);
            wrap!(xAccess, access_vfs);
            wrap!(xFullPathname, path_vfs);
            wrap!(xDlOpen, dlopen_vfs);
            wrap!(xDlError, dlerror_vfs);
            wrap!(xDlSym, dlsym_vfs);
            wrap!(xDlClose, dlclose_vfs);
            wrap!(xRandomness, random_vfs);
            wrap!(xSleep, sleep_vfs);
            wrap!(xCurrentTime, time_vfs);
            wrap!(xGetLastError, error_vfs);
            if proxy.iVersion >= 2 {
                wrap!(xCurrentTimeInt64, time64_vfs);
            }
            if proxy.iVersion >= 3 {
                wrap!(xSetSystemCall, setcall_vfs);
                wrap!(xGetSystemCall, getcall_vfs);
                wrap!(xNextSystemCall, nextcall_vfs);
            }
            assert_eq!(ffi::sqlite3_vfs_register(&mut *proxy, 1), ffi::SQLITE_OK);
            eprintln!(
                "DISPOSABLE_VFS sqlite={} underlying={}",
                CStr::from_ptr(ffi::sqlite3_libversion()).to_string_lossy(),
                CStr::from_ptr((*prior).zName).to_string_lossy()
            );
            Self { proxy, context }
        }
    }
    fn calibrate(&self, path: &std::path::Path) {
        // This control file is not a Store or Disposable backing. It proves
        // the observer counts and forwards xSync rather than suppressing it.
        let name = CString::new(path.to_str().unwrap()).unwrap();
        unsafe {
            let p = ffi::sqlite3_malloc(self.proxy.szOsFile).cast::<ffi::sqlite3_file>();
            assert!(!p.is_null());
            ptr::write_bytes(p.cast::<u8>(), 0, self.proxy.szOsFile as usize);
            let mut flags = 0;
            let result = (self.proxy.xOpen.unwrap())(
                (&*self.proxy as *const ffi::sqlite3_vfs).cast_mut(),
                name.as_ptr(),
                p,
                ffi::SQLITE_OPEN_MAIN_DB | ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE,
                &mut flags,
            );
            assert_eq!(result, ffi::SQLITE_OK);
            let m = &*(*p).pMethods;
            assert_eq!(
                (m.xWrite.unwrap())(p, b"control".as_ptr().cast(), 7, 0),
                ffi::SQLITE_OK
            );
            assert_eq!(
                (m.xSync.unwrap())(p, ffi::SQLITE_SYNC_NORMAL),
                ffi::SQLITE_OK
            );
            assert_eq!((m.xClose.unwrap())(p), ffi::SQLITE_OK);
            ffi::sqlite3_free(p.cast());
        }
        assert_eq!(self.context.counts.report("observer-control"), 1);
        self.context.counts.reset();
        fs::remove_file(path).unwrap();
    }
}
impl Drop for Observer {
    fn drop(&mut self) {
        unsafe {
            assert_eq!(
                ffi::sqlite3_vfs_register(self.context.original, 1),
                ffi::SQLITE_OK
            );
            assert_eq!(
                ffi::sqlite3_vfs_unregister(&mut *self.proxy),
                ffi::SQLITE_OK
            );
        }
    }
}
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "layerfs-disposable-startup-sync-{}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        eprintln!("DISPOSABLE_SYNC_FIXTURE {}", path.display());
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!(
                "RETAINED_FAILED_DISPOSABLE_SYNC_FIXTURE {}",
                self.0.display()
            );
        } else {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

#[test]
fn disposable_create_save_reopen_seal_never_calls_vfs_sync() {
    let fixture = Fixture::new();
    let observer = Observer::new();
    observer.calibrate(&fixture.0.join("non-store-observer-control"));
    let authority = HistoryCatalogConfig {
        binding_key: b"disposable-startup-sync".to_vec(),
        cursor_key: [73; 32],
        incarnation: 9,
    };
    let counts = &observer.context.counts;
    let mut opens = [0; 4];
    let mut writes = [0; 4];
    let mut closes = [0; 4];
    let mut errors = 0;
    let mut syncs = 0;
    let mut observe = |phase: &str| {
        for kind in 0..4 {
            opens[kind] += counts.opens[kind].load(Ordering::Relaxed);
            writes[kind] += counts.writes[kind].load(Ordering::Relaxed);
            closes[kind] += counts.closes[kind].load(Ordering::Relaxed);
        }
        errors += counts.errors.load(Ordering::Relaxed);
        syncs += counts.report(phase);
        counts.reset();
    };
    for layout in [
        SqlitePackLayout::Monolithic,
        SqlitePackLayout::GroupRows,
        SqlitePackLayout::GroupRowsIndexed,
    ] {
        for acquisition in [
            SqliteAcquisitionSchema::Absent,
            SqliteAcquisitionSchema::Tables,
        ] {
            let path = fixture.0.join(format!("{layout:?}-{acquisition:?}.sqlite"));
            let config = PersistenceConfig::sqlite(&path)
                .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
                .with_sqlite_pack_layout(layout)
                .with_sqlite_acquisition(acquisition);
            let handles =
                Handles::create(config.clone(), StoragePolicy::frozen_default(), &authority)
                    .unwrap();
            assert_eq!(
                handles.profile().persistence,
                SqlitePersistenceProfile::Disposable
            );
            assert_eq!(handles.profile().synchronous, 0);
            assert_eq!(handles.profile().fullfsync, 0);
            assert_eq!(handles.profile().journal_mode, "wal");
            observe(&format!("create-{layout:?}-{acquisition:?}"));
            let storage = Storage::new(handles.storage.clone()).unwrap();
            let object = FinalizedObject::new(
                ObjectRole::FileState,
                layerfs_content::object::codec::encode_bytes_object(b"Disposable canonical object")
                    .unwrap(),
            )
            .unwrap();
            let save = storage.begin_save().unwrap();
            save.accept(object.clone()).unwrap();
            save.finish().unwrap();
            drop(storage);
            observe(&format!("save-{layout:?}-{acquisition:?}"));
            handles.seal().unwrap();
            observe(&format!("seal-{layout:?}-{acquisition:?}"));
            for suffix in ["-wal", "-shm", "-journal"] {
                assert!(!PathBuf::from(format!("{}{suffix}", path.display())).exists());
            }
            let reopened =
                Handles::open_writable(config, &authority.binding_key, authority.cursor_key)
                    .unwrap();
            observe(&format!("reopen-{layout:?}-{acquisition:?}"));
            let storage = Storage::new(reopened.storage.clone()).unwrap();
            assert_eq!(
                storage
                    .reader()
                    .unwrap()
                    .read_objects(&[object.id()])
                    .unwrap(),
                vec![object.canonical().to_vec()]
            );
            drop(storage);
            reopened.seal().unwrap();
            observe(&format!("reopen-seal-{layout:?}-{acquisition:?}"));
        }
    }
    assert!(opens[0] >= 12);
    assert!(writes[0] > 0);
    assert!(writes[1] > 0);
    assert_eq!(opens, closes, "every observed file must be closed");
    assert_eq!(errors, 0);
    assert_eq!(
        syncs, 0,
        "public Disposable startup/save/seal issued actual delegated VFS xSync"
    );
}
