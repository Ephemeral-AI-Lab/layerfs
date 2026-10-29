//! The `layerfs-remote` SQLite VFS (#510), registered once per process
//! through `rusqlite::ffi`. This module is the crate's only unsafe code.
//!
//! It opens main database files only, by the name of a live
//! [`RemoteFile`] (see [`register`]); a journal, WAL, `-shm` or temporary
//! file is never opened (temporary storage is memory). The file reports
//! `SQLITE_IOCAP_IMMUTABLE` and connections open it with `immutable=1`, so
//! SQLite takes no lock and checks for no change. Writes, truncation, sync
//! and delete refuse `SQLITE_READONLY`. Randomness, sleep and time are the
//! default VFS's. No callback lets a panic cross into SQLite.
#![allow(unsafe_code)]

use super::RemoteFile;
use rusqlite::ffi;
use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_void, CStr};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock, Weak};

/// The VFS name connections select.
pub(crate) const NAME: &str = "layerfs-remote";
const NAME_C: &CStr = c"layerfs-remote";

/// Live remote files by name. Weak: a file leaves when its last owner drops it.
static FILES: LazyLock<Mutex<HashMap<String, Weak<RemoteFile>>>> = LazyLock::new(Default::default);
static NEXT: AtomicU64 = AtomicU64::new(1);

/// A name no other remote file of this process has.
pub(super) fn unique_name() -> String {
    let next = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("/layerfs-remote/{}-{next}", std::process::id())
}

fn files() -> std::sync::MutexGuard<'static, HashMap<String, Weak<RemoteFile>>> {
    FILES.lock().unwrap_or_else(|poison| poison.into_inner())
}

pub(super) fn register(file: &Arc<RemoteFile>) {
    files().insert(file.name.clone(), Arc::downgrade(file));
}

pub(super) fn unregister(name: &str) {
    files().remove(name);
}

/// Whether the VFS registered. SQLite keeps its leaked struct for the process's life.
static VFS: OnceLock<Result<(), c_int>> = OnceLock::new();

/// Registers the VFS (not as the default) once; the SQLite code of a failure.
pub(crate) fn ensure() -> Result<(), c_int> {
    let registered = VFS.get_or_init(|| {
        // SAFETY: sqlite3_initialize and sqlite3_vfs_find are safe to call at
        // any time; the default VFS outlives the process.
        let default = unsafe {
            let rc = ffi::sqlite3_initialize();
            if rc != ffi::SQLITE_OK {
                return Err(rc);
            }
            ffi::sqlite3_vfs_find(ptr::null())
        };
        if default.is_null() {
            return Err(ffi::SQLITE_ERROR);
        }
        let vfs = Box::into_raw(Box::new(ffi::sqlite3_vfs {
            iVersion: 2,
            szOsFile: std::mem::size_of::<File>() as c_int,
            mxPathname: 1024,
            pNext: ptr::null_mut(),
            zName: NAME_C.as_ptr(),
            pAppData: default.cast::<c_void>(),
            xOpen: Some(x_open),
            xDelete: Some(x_delete),
            xAccess: Some(x_access),
            xFullPathname: Some(x_full_pathname),
            xDlOpen: Some(x_dl_open),
            xDlError: Some(x_dl_error),
            xDlSym: Some(x_dl_sym),
            xDlClose: Some(x_dl_close),
            xRandomness: Some(x_randomness),
            xSleep: Some(x_sleep),
            xCurrentTime: Some(x_current_time),
            xGetLastError: Some(x_get_last_error),
            xCurrentTimeInt64: Some(x_current_time_int64),
            xSetSystemCall: None,
            xGetSystemCall: None,
            xNextSystemCall: None,
        }));
        // SAFETY: `vfs` is a valid, leaked (never freed once registered)
        // sqlite3_vfs whose name and callbacks are 'static.
        let rc = unsafe { ffi::sqlite3_vfs_register(vfs, 0) };
        if rc != ffi::SQLITE_OK {
            // SAFETY: SQLite refused it, so it holds no pointer to `vfs`.
            drop(unsafe { Box::from_raw(vfs) });
            return Err(rc);
        }
        Ok(())
    });
    *registered
}

/// The file object SQLite allocates (`szOsFile` bytes) for each open.
#[repr(C)]
struct File {
    base: ffi::sqlite3_file,
    /// From `Arc::into_raw`; released by `x_close`.
    remote: *const RemoteFile,
}

static METHODS: ffi::sqlite3_io_methods = ffi::sqlite3_io_methods {
    iVersion: 1,
    xClose: Some(x_close),
    xRead: Some(x_read),
    xWrite: Some(x_write),
    xTruncate: Some(x_truncate),
    xSync: Some(x_sync),
    xFileSize: Some(x_file_size),
    xLock: Some(x_lock),
    xUnlock: Some(x_lock),
    xCheckReservedLock: Some(x_check_reserved_lock),
    xFileControl: Some(x_file_control),
    xSectorSize: Some(x_sector_size),
    xDeviceCharacteristics: Some(x_device_characteristics),
    xShmMap: None,
    xShmLock: None,
    xShmBarrier: None,
    xShmUnmap: None,
    xFetch: None,
    xUnfetch: None,
};

/// Runs `body`, answering `SQLITE_IOERR` if it panics.
fn guarded(body: impl FnOnce() -> c_int) -> c_int {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or(ffi::SQLITE_IOERR)
}

/// The default VFS this one delegates randomness and time to.
///
/// # Safety
/// `vfs` must be this module's registered VFS.
unsafe fn default_of(vfs: *mut ffi::sqlite3_vfs) -> *mut ffi::sqlite3_vfs {
    // SAFETY: the caller passes our VFS, whose pAppData is the default VFS.
    unsafe { (*vfs).pAppData.cast::<ffi::sqlite3_vfs>() }
}

/// The live remote file named `name`.
///
/// # Safety
/// `name` must be null or a NUL-terminated string.
unsafe fn lookup(name: *const c_char) -> Option<Arc<RemoteFile>> {
    if name.is_null() {
        return None;
    }
    // SAFETY: the caller guarantees a NUL-terminated string.
    let name = unsafe { CStr::from_ptr(name) }.to_str().ok()?;
    files().get(name)?.upgrade()
}

unsafe extern "C" fn x_open(
    _vfs: *mut ffi::sqlite3_vfs,
    name: *const c_char,
    file: *mut ffi::sqlite3_file,
    flags: c_int,
    out_flags: *mut c_int,
) -> c_int {
    guarded(|| {
        // SAFETY: SQLite passes `szOsFile` writable bytes at `file`. With no
        // methods set, SQLite does not call xClose on a failed open.
        unsafe { (*file).pMethods = ptr::null() };
        if flags & ffi::SQLITE_OPEN_MAIN_DB == 0 {
            return ffi::SQLITE_CANTOPEN;
        }
        // SAFETY: SQLite passes null or a NUL-terminated name.
        let Some(remote) = (unsafe { lookup(name) }) else {
            return ffi::SQLITE_CANTOPEN;
        };
        // SAFETY: as above, `file` holds a `File`'s bytes; the Arc's count
        // is released once, by x_close.
        unsafe {
            ptr::write(
                file.cast::<File>(),
                File {
                    base: ffi::sqlite3_file { pMethods: &METHODS },
                    remote: Arc::into_raw(remote),
                },
            );
            if !out_flags.is_null() {
                *out_flags = ffi::SQLITE_OPEN_READONLY | ffi::SQLITE_OPEN_MAIN_DB;
            }
        }
        ffi::SQLITE_OK
    })
}

/// The remote file behind an open `file`.
///
/// # Safety
/// `file` must be a file `x_open` opened and `x_close` has not closed.
unsafe fn remote<'a>(file: *mut ffi::sqlite3_file) -> &'a RemoteFile {
    // SAFETY: x_open wrote a `File` whose `remote` stays valid until x_close.
    unsafe { &*(*file.cast::<File>()).remote }
}

unsafe extern "C" fn x_close(file: *mut ffi::sqlite3_file) -> c_int {
    guarded(|| {
        // SAFETY: an open file from x_open; its Arc count is released here once.
        unsafe {
            let file = file.cast::<File>();
            let remote = std::mem::replace(&mut (*file).remote, ptr::null());
            if !remote.is_null() {
                drop(Arc::from_raw(remote));
            }
        }
        ffi::SQLITE_OK
    })
}

unsafe extern "C" fn x_read(
    file: *mut ffi::sqlite3_file,
    buffer: *mut c_void,
    amount: c_int,
    offset: ffi::sqlite3_int64,
) -> c_int {
    guarded(|| {
        let (Ok(amount), Ok(offset)) = (usize::try_from(amount), u64::try_from(offset)) else {
            return ffi::SQLITE_IOERR_READ;
        };
        // SAFETY: SQLite passes an open file and `amount` writable bytes.
        let (remote, out) = unsafe {
            (
                remote(file),
                std::slice::from_raw_parts_mut(buffer.cast::<u8>(), amount),
            )
        };
        let size = remote.bytes();
        let within = size.saturating_sub(offset).min(amount as u64) as usize;
        if remote.read(offset, &mut out[..within]).is_err() {
            return ffi::SQLITE_IOERR_READ;
        }
        if within < amount {
            out[within..].fill(0);
            return ffi::SQLITE_IOERR_SHORT_READ;
        }
        ffi::SQLITE_OK
    })
}

unsafe extern "C" fn x_write(
    _file: *mut ffi::sqlite3_file,
    _buffer: *const c_void,
    _amount: c_int,
    _offset: ffi::sqlite3_int64,
) -> c_int {
    ffi::SQLITE_READONLY
}

unsafe extern "C" fn x_truncate(_file: *mut ffi::sqlite3_file, _size: ffi::sqlite3_int64) -> c_int {
    ffi::SQLITE_READONLY
}

unsafe extern "C" fn x_sync(_file: *mut ffi::sqlite3_file, _flags: c_int) -> c_int {
    ffi::SQLITE_READONLY
}

unsafe extern "C" fn x_file_size(
    file: *mut ffi::sqlite3_file,
    size: *mut ffi::sqlite3_int64,
) -> c_int {
    guarded(|| {
        // SAFETY: an open file and a writable size slot.
        let bytes = unsafe { remote(file) }.bytes();
        let Ok(bytes) = ffi::sqlite3_int64::try_from(bytes) else {
            return ffi::SQLITE_IOERR_FSTAT;
        };
        // SAFETY: as above.
        unsafe { *size = bytes };
        ffi::SQLITE_OK
    })
}

/// Locks and unlocks: an immutable file takes none.
unsafe extern "C" fn x_lock(_file: *mut ffi::sqlite3_file, _level: c_int) -> c_int {
    ffi::SQLITE_OK
}

unsafe extern "C" fn x_check_reserved_lock(
    _file: *mut ffi::sqlite3_file,
    out: *mut c_int,
) -> c_int {
    // SAFETY: SQLite passes a writable result slot.
    unsafe { *out = 0 };
    ffi::SQLITE_OK
}

/// Answers the memory-map size (always 0: the file is never mapped, as
/// the Store profile requires); every other control is not handled here.
unsafe extern "C" fn x_file_control(
    _file: *mut ffi::sqlite3_file,
    op: c_int,
    arg: *mut c_void,
) -> c_int {
    if op != ffi::SQLITE_FCNTL_MMAP_SIZE || arg.is_null() {
        return ffi::SQLITE_NOTFOUND;
    }
    // SAFETY: for SQLITE_FCNTL_MMAP_SIZE SQLite passes an in/out i64.
    unsafe { *arg.cast::<ffi::sqlite3_int64>() = 0 };
    ffi::SQLITE_OK
}

unsafe extern "C" fn x_sector_size(_file: *mut ffi::sqlite3_file) -> c_int {
    4096
}

unsafe extern "C" fn x_device_characteristics(_file: *mut ffi::sqlite3_file) -> c_int {
    ffi::SQLITE_IOCAP_IMMUTABLE
}

unsafe extern "C" fn x_delete(
    _vfs: *mut ffi::sqlite3_vfs,
    _name: *const c_char,
    _sync_dir: c_int,
) -> c_int {
    ffi::SQLITE_READONLY
}

unsafe extern "C" fn x_access(
    _vfs: *mut ffi::sqlite3_vfs,
    name: *const c_char,
    flags: c_int,
    out: *mut c_int,
) -> c_int {
    guarded(|| {
        // SAFETY: SQLite passes a NUL-terminated name and a result slot.
        let exists = unsafe { lookup(name) }.is_some();
        let answer = flags == ffi::SQLITE_ACCESS_EXISTS && exists;
        // SAFETY: as above.
        unsafe { *out = c_int::from(answer) };
        ffi::SQLITE_OK
    })
}

unsafe extern "C" fn x_full_pathname(
    _vfs: *mut ffi::sqlite3_vfs,
    name: *const c_char,
    size: c_int,
    out: *mut c_char,
) -> c_int {
    guarded(|| {
        // SAFETY: SQLite passes a NUL-terminated name and `size` bytes at `out`.
        let bytes = unsafe { CStr::from_ptr(name) }.to_bytes_with_nul();
        if usize::try_from(size).map_or(true, |size| bytes.len() > size) {
            return ffi::SQLITE_CANTOPEN;
        }
        // SAFETY: as above; the regions do not overlap (SQLite's own buffer).
        unsafe { ptr::copy_nonoverlapping(bytes.as_ptr().cast::<c_char>(), out, bytes.len()) };
        ffi::SQLITE_OK
    })
}

/// No extension is loaded through this VFS.
unsafe extern "C" fn x_dl_open(_vfs: *mut ffi::sqlite3_vfs, _name: *const c_char) -> *mut c_void {
    ptr::null_mut()
}

unsafe extern "C" fn x_dl_error(_vfs: *mut ffi::sqlite3_vfs, size: c_int, out: *mut c_char) {
    let message = c"extensions are not loaded through the layerfs-remote VFS".to_bytes_with_nul();
    if let Ok(size) = usize::try_from(size) {
        if size > 0 {
            let n = message.len().min(size);
            // SAFETY: SQLite passes `size` writable bytes; the last one is NUL.
            unsafe {
                ptr::copy_nonoverlapping(message.as_ptr().cast::<c_char>(), out, n);
                *out.add(n - 1) = 0;
            }
        }
    }
}

type Symbol = unsafe extern "C" fn(*mut ffi::sqlite3_vfs, *mut c_void, *const c_char);

unsafe extern "C" fn x_dl_sym(
    _vfs: *mut ffi::sqlite3_vfs,
    _handle: *mut c_void,
    _symbol: *const c_char,
) -> Option<Symbol> {
    None
}

unsafe extern "C" fn x_dl_close(_vfs: *mut ffi::sqlite3_vfs, _handle: *mut c_void) {}

unsafe extern "C" fn x_randomness(
    vfs: *mut ffi::sqlite3_vfs,
    size: c_int,
    out: *mut c_char,
) -> c_int {
    // SAFETY: our VFS; the default VFS's callback gets its own pointer.
    unsafe {
        let default = default_of(vfs);
        match (*default).xRandomness {
            Some(randomness) => randomness(default, size, out),
            None => 0,
        }
    }
}

unsafe extern "C" fn x_sleep(vfs: *mut ffi::sqlite3_vfs, microseconds: c_int) -> c_int {
    // SAFETY: as in x_randomness.
    unsafe {
        let default = default_of(vfs);
        match (*default).xSleep {
            Some(sleep) => sleep(default, microseconds),
            None => 0,
        }
    }
}

unsafe extern "C" fn x_current_time(vfs: *mut ffi::sqlite3_vfs, out: *mut f64) -> c_int {
    // SAFETY: as in x_randomness.
    unsafe {
        let default = default_of(vfs);
        match (*default).xCurrentTime {
            Some(time) => time(default, out),
            None => ffi::SQLITE_ERROR,
        }
    }
}

unsafe extern "C" fn x_current_time_int64(
    vfs: *mut ffi::sqlite3_vfs,
    out: *mut ffi::sqlite3_int64,
) -> c_int {
    // SAFETY: as in x_randomness; a version 1 default VFS has no such field.
    unsafe {
        let default = default_of(vfs);
        if (*default).iVersion < 2 {
            return ffi::SQLITE_ERROR;
        }
        match (*default).xCurrentTimeInt64 {
            Some(time) => time(default, out),
            None => ffi::SQLITE_ERROR,
        }
    }
}

unsafe extern "C" fn x_get_last_error(
    vfs: *mut ffi::sqlite3_vfs,
    size: c_int,
    out: *mut c_char,
) -> c_int {
    // SAFETY: as in x_randomness.
    unsafe {
        let default = default_of(vfs);
        match (*default).xGetLastError {
            Some(last) => last(default, size, out),
            None => 0,
        }
    }
}
