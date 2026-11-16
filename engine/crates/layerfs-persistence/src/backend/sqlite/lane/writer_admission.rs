//! Cooperative local-volume ownership before one SQLite write attempt.
use crate::backend::records::BackendError;
use std::path::Path;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use {
    nix::{
        errno::Errno,
        fcntl::{Flock, FlockArg},
    },
    std::fs::File,
};

#[cfg(windows)]
use std::ffi::c_void;

/// A kernel mutex handle:Send by construction (HANDLEs are owned,
/// thread-affine only in wait calls that take &self).
#[cfg(windows)]
#[derive(Clone, Copy)]
pub(crate) struct MutexHandle(*mut c_void);

#[cfg(windows)]
unsafe impl Send for MutexHandle {}
#[cfg(windows)]
unsafe impl Sync for MutexHandle {}

pub(crate) struct Writer {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    lock: Flock<File>,
    /// The named mutex this lane holds, by store directory hash.
    #[cfg(windows)]
    mutex: MutexHandle,
}

impl Writer {
    pub(crate) fn acquire(path: &Path) -> Result<Self, BackendError> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            // Never open another FD to SQLite's main/WAL/SHM files here:
            // closing any main-file FD cancels every process POSIX lock on
            // that inode, including SQLite's live SHARED locks, and Darwin
            // flock on that file conflicts with SQLite's own fcntl locks. An
            // independent description of the existing parent directory
            // coordinates short writer jobs without interfering with SQLite's
            // descriptors. Multiple Store files in one directory share this
            // coarse lane.
            let directory = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            let file = File::open(directory).map_err(|_| BackendError::Integrity)?;
            if !file
                .metadata()
                .map_err(|_| BackendError::Integrity)?
                .is_dir()
            {
                return Err(BackendError::Integrity);
            }
            // One blocking kernel wait, taken after the Session's FIFO turn,
            // so at most one thread per Session waits here. Its exits: the lock
            // (a holder process that exits releases it); `Cancelled` when a
            // signal interrupts the wait. No BEGIN/body has been attempted.
            match Flock::lock(file, FlockArg::LockExclusive) {
                Ok(lock) => Ok(Self { lock }),
                Err((_, Errno::EINTR)) => Err(BackendError::Cancelled),
                Err(_) => Err(BackendError::Integrity),
            }
        }
        #[cfg(target_os = "macos")]
        {
            // Darwin's flock conflicts with SQLite's open-file locks.
            // Host Init is sole-owner; cross-session contention
            // retains exact SQLite Busy.
            let _ = path;
            Ok(Self {})
        }
        #[cfg(windows)]
        {
            // The named-mutex lane (Plan B): one kernel mutex per Store
            // directory, held for one short writer job, so multi-opener
            // contention serializes instead of colliding on SQLite Busy.
            let mutex = unsafe { windows_acquire(path) }?;
            Ok(Self { mutex: MutexHandle(mutex) })
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            let _ = path;
            Err(BackendError::Integrity)
        }
    }

    /// One non-blocking attempt at the same lane, for work that is skipped
    /// rather than waited for (#86: the idle WAL checkpoint). `Ok(None)`:
    /// another holder has it now (EWOULDBLOCK) and the caller skips its work.
    /// This is a single attempt, never a retry: no loop, sleep or busy handler
    /// follows it here, and a caller must not wrap it in one.
    pub(crate) fn try_acquire(path: &Path) -> Result<Option<Self>, BackendError> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            // The same independent directory description as `acquire`.
            let directory = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            let file = File::open(directory).map_err(|_| BackendError::Integrity)?;
            if !file
                .metadata()
                .map_err(|_| BackendError::Integrity)?
                .is_dir()
            {
                return Err(BackendError::Integrity);
            }
            match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
                Ok(lock) => Ok(Some(Self { lock })),
                Err((_, Errno::EWOULDBLOCK)) => Ok(None),
                Err((_, Errno::EINTR)) => Err(BackendError::Cancelled),
                Err(_) => Err(BackendError::Integrity),
            }
        }
        #[cfg(target_os = "windows")]
        {
            // The same lane, one non-blocking attempt (0 ms).
            let name = windows_name(path);
            let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
            if mutex.is_null() {
                return Err(BackendError::Integrity);
            }
            let wait = unsafe { WaitForSingleObject(mutex, 0) };
            if wait == WAIT_OBJECT_0 {
                Ok(Some(Self { mutex: MutexHandle(mutex) }))
            } else {
                unsafe { CloseHandle(mutex) };
                Ok(None)
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            // No advisory lane on other platforms.
            Self::acquire(path).map(Some)
        }
    }

    pub(crate) fn release(self) -> Result<(), Self> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            match self.lock.unlock() {
                Ok(file) => {
                    drop(file);
                    Ok(())
                }
                Err((lock, _)) => Err(Self { lock }),
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            // The lane's hold is owned by the Writer; on Windows the
            // mutex releases on drop, so the explicit path is a no-op.
            let _ = self;
            Ok(())
        }
    }
}
impl crate::backend::sqlite::session::connection::State {
    pub(crate) fn finish_writer(&mut self) -> Result<(), BackendError> {
        if self.quarantined {
            // Connection is declared before this retained owner in State, so
            // SQLite teardown precedes advisory release at last-session drop.
            return Ok(());
        }
        if let Some(writer) = self.writer_hold.take() {
            if let Err(retained) = writer.release() {
                self.quarantined = true;
                self.writer_hold = Some(retained);
                return Err(BackendError::Unknown);
            }
        }
        Ok(())
    }
}


#[cfg(windows)]
impl Drop for Writer {
    fn drop(&mut self) {
        if !self.mutex.0.is_null() {
            unsafe { windows_release(self.mutex.0) };
        }
    }
}

/// The kernel mutex name for one Store directory: a stable hash of its
/// canonical text, namespaced so no other product collides.
#[cfg(windows)]
fn windows_name(path: &Path) -> Vec<u16> {
    use std::hash::{Hash, Hasher};
    let text = path.to_string_lossy();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    let hash = hasher.finish();
    format!("Local\\LayerFS-WriterLane-{hash:016x}\0")
        .encode_utf16()
        .collect()
}

#[cfg(windows)]
extern "system" {
    fn CreateMutexW(
        attributes: *const c_void,
        initial_owner: i32,
        name: *const u16,
    ) -> *mut c_void;
    fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
    fn ReleaseMutex(handle: *mut c_void) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
    fn GetLastError() -> u32;
}

const WAIT_OBJECT_0: u32 = 0;
const ERROR_ALREADY_EXISTS: u32 = 183;

/// Opens (or creates) the lane mutex and takes it with one bounded wait.
#[cfg(windows)]
unsafe fn windows_acquire(path: &Path) -> Result<*mut c_void, BackendError> {
    let name = windows_name(path);
    let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if mutex.is_null() {
        return Err(BackendError::Integrity);
    }
    // One bounded kernel wait, matching the unix lane's single blocking
    // acquisition; INFINITE is correct here because the lane is short.
    let wait = unsafe { WaitForSingleObject(mutex, 0xFFFFFFFF) };
    if wait != WAIT_OBJECT_0 {
        unsafe { CloseHandle(mutex) };
        return Err(BackendError::Integrity);
    }
    let _ = ERROR_ALREADY_EXISTS;
    let _ = GetLastError;
    Ok(mutex)
}

#[cfg(windows)]
unsafe fn windows_release(mutex: *mut c_void) {
    unsafe {
        ReleaseMutex(mutex);
        CloseHandle(mutex);
    }
}
