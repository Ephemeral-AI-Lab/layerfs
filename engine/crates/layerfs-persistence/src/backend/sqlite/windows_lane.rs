//! The Windows Store's one audited FFI surface.
//!
//! This is the Windows companion of the macOS seal file-control module: one
//! owner-authorized module of raw Win32 calls, allowed by the boundary guard
//! and compiled on Windows only; no other module in this crate may use
//! unsafe Rust. The writer lane is one named kernel mutex per Store
//! directory: the kernel releases it when a holder process exits (the next
//! waiter wakes on the abandoned mutex), which matches the Linux directory
//! flock's exit release, and it never opens a second descriptor to SQLite's
//! main/WAL/SHM files. The handle is held as an integer so `Writer` stays
//! plain data. The volume probe reports the filesystem of the directory's
//! local volume for the kernel guard's allowlist.
#![allow(unsafe_code)]
use sha2::{Digest, Sha256};
use std::io;
use std::path::Path;
use windows_sys::Win32::Foundation::{CloseHandle, WAIT_ABANDONED, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{
    CreateMutexW, ReleaseMutex, WaitForSingleObject, INFINITE,
};

/// The Store directory's cooperative writer lane.
pub(crate) struct LaneMutex {
    handle: usize,
    closed: bool,
}

/// The lane's kernel object name: SHA-256 of the resolved directory's
/// encoded bytes, truncated to 64 bits. A versioned-stable digest, not
/// `DefaultHasher`, whose algorithm may change between Rust releases and
/// silently split one directory onto two lanes across builds (K3 review F1).
/// The `Global\` namespace keeps one lane per directory across login
/// sessions (a session-0 scheduled task and the user session share one
/// Store file; `Local\` would give each its own lane and two writers —
/// round-3 A2 F4). A mutant needs no create-global privilege.
pub(crate) fn lane_name(directory: &Path) -> String {
    let resolved = std::fs::canonicalize(directory).unwrap_or_else(|_| directory.to_path_buf());
    let digest = Sha256::digest(resolved.as_os_str().as_encoded_bytes());
    let mut tag = [0u8; 8];
    tag.copy_from_slice(&digest[..8]);
    format!(
        "Global\\layerfs-writer-lane-{:016x}",
        u64::from_be_bytes(tag)
    )
}

impl LaneMutex {
    /// One blocking kernel wait on the directory's named mutex, taken after
    /// the Session's FIFO turn, so at most one thread per Session waits here.
    /// Creation only opens the shared kernel object. Exits: the lane (a
    /// holder process that exits releases it); a failed wait.
    pub(crate) fn acquire(directory: &Path) -> Result<Self, ()> {
        let name: Vec<u16> = lane_name(directory)
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if handle.is_null() {
            return Err(());
        }
        let waited = unsafe { WaitForSingleObject(handle, INFINITE) };
        if waited == WAIT_OBJECT_0 || waited == WAIT_ABANDONED {
            Ok(Self {
                handle: handle as usize,
                closed: false,
            })
        } else {
            unsafe { CloseHandle(handle) };
            Err(())
        }
    }

    /// Releases the lane and destroys this handle; a failed release returns
    /// the retained lane unchanged, so the quarantining caller keeps holding
    /// it, and its eventual drop retries the release after SQLite teardown.
    pub(crate) fn release(mut self) -> Result<(), Self> {
        let handle = self.handle as windows_sys::Win32::Foundation::HANDLE;
        if unsafe { ReleaseMutex(handle) } == 0 {
            return Err(Self {
                handle: self.handle,
                closed: false,
            });
        }
        self.closed = true;
        unsafe { CloseHandle(handle) };
        Ok(())
    }
}

impl Drop for LaneMutex {
    /// Best-effort release of a lane an error or panic path left unreleased
    /// (K3-R3 F6): the release can fail on a thread that does not own the
    /// mutex — the owner exits into the kernel's abandoned-mutex handoff —
    /// while the descriptor closes exactly once either way. Linux gets this
    /// for free from `Flock`'s own drop.
    fn drop(&mut self) {
        if self.closed {
            return;
        }
        let handle = self.handle as windows_sys::Win32::Foundation::HANDLE;
        self.closed = true;
        unsafe { ReleaseMutex(handle) };
        unsafe { CloseHandle(handle) };
    }
}

/// A verbatim (`\\?\`-prefixed) wide spelling of an absolute path, NUL
/// terminated, so the Win32 calls in the audited modules accept trees
/// deeper than MAX_PATH — a plain spelling can fail at 260 characters on
/// hosts without the long-path relaxation (the hosting node process
/// typically carries no `longPathAware` manifest). Already-verbatim
/// paths pass through unchanged; UNC shares become `\\?\UNC\...`;
/// relative paths pass through unchanged (every call site is absolute).
/// Forward slashes become backslashes in the spelling this function
/// builds; an already-verbatim input is passed through byte-for-byte —
/// Win32 does not normalize under the prefix, so a verbatim input
/// carrying forward slashes fails cleanly at its first use.
pub fn verbatim_wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    let mut units: Vec<u16> = path.as_os_str().encode_wide().collect();
    const PREFIX: [u16; 4] = [92, 92, 63, 92]; // \ ? \
    if units.len() >= 4 && units[..4] == PREFIX {
        // Already verbatim.
    } else if matches!(
        (units.first(), units.get(1)),
        (Some(&92), Some(&92)) | (Some(&47), Some(&47))
    ) {
        // \\server\share (or //server/share) → \\?\UNC\server\share — the
        // separator after UNC is part of the spelling; without it the
        // server name mis-joins.
        let mut prefixed = Vec::with_capacity(units.len() + 7);
        prefixed.extend_from_slice(&PREFIX);
        prefixed.extend([85, 78, 67, 92]); // U N C \
        prefixed.extend(
            units[2..]
                .iter()
                .map(|&unit| if unit == 47 { 92 } else { unit }),
        );
        units = prefixed;
    } else if path.is_absolute() {
        let mut prefixed = Vec::with_capacity(units.len() + 4);
        prefixed.extend_from_slice(&PREFIX);
        prefixed.extend(units.iter().map(|&unit| if unit == 47 { 92 } else { unit }));
        units = prefixed;
    }
    units.push(0);
    units
}

/// The filesystem name of the volume actually holding `directory`, for the
/// kernel guard's allowlist. The directory's own handle answers, and its
/// final path decides locality: the spelling is irrelevant (a verbatim
/// local path, a junction or a mapped drive all resolve), and a final path
/// that lands on a share (`\\?\UNC\...`) is refused — its locks travel the
/// SMB redirector, not this kernel's lock manager (K3-R3 F4).
pub fn windows_volume_filesystem(directory: &Path) -> io::Result<String> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, GetFinalPathNameByHandleW, GetVolumeInformationByHandleW,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        OPEN_EXISTING, VOLUME_NAME_DOS,
    };
    let wide = verbatim_wide(directory);
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!(
                "the directory {} cannot be opened to examine its volume: {}",
                directory.display(),
                io::Error::last_os_error()
            ),
        ));
    }
    let mut final_path = [0u16; 1024];
    let mut written = unsafe {
        GetFinalPathNameByHandleW(
            handle,
            final_path.as_mut_ptr(),
            final_path.len() as u32,
            VOLUME_NAME_DOS,
        )
    };
    if written as usize > final_path.len() {
        final_path = vec![0u16; written as usize].try_into().unwrap_or([0; 1024]);
        written = unsafe {
            GetFinalPathNameByHandleW(
                handle,
                final_path.as_mut_ptr(),
                final_path.len() as u32,
                VOLUME_NAME_DOS,
            )
        };
    }
    let share = written != 0
        && final_path
            .iter()
            .take(8)
            .zip(r"\\?\UNC\".encode_utf16())
            .all(|(unit, expected)| *unit == expected);
    let mut filesystem = [0u16; 16];
    let examined = unsafe {
        GetVolumeInformationByHandleW(
            handle,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            filesystem.as_mut_ptr(),
            filesystem.len() as u32,
        )
    };
    // Capture the failure before another syscall can overwrite it.
    let failure = (examined == 0).then(io::Error::last_os_error);
    unsafe { CloseHandle(handle) };
    if written == 0 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!(
                "the final path of {} cannot be read: {}",
                directory.display(),
                io::Error::last_os_error()
            ),
        ));
    }
    if share {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!(
                "the directory {} resolves onto a network share; the Store needs a local volume",
                directory.display()
            ),
        ));
    }
    if let Some(error) = failure {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!(
                "the volume of {} cannot be examined: {error}",
                directory.display()
            ),
        ));
    }
    let end = filesystem.iter().position(|&wide| wide == 0).unwrap_or(0);
    Ok(String::from_utf16_lossy(&filesystem[..end]))
}
