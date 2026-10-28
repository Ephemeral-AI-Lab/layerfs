//! The Windows host-identity probe of namespace Init.
//!
//! This is layerfs-project's one owner-authorized module of raw Win32
//! calls, allowed by the boundary guard and compiled on Windows only; no
//! other module in this crate may use unsafe Rust. The acquisition keys
//! rows by device and file index; the volume serial number and file index
//! are the entry's real identity.
#![allow(unsafe_code)]
use std::io;
use std::path::Path;

/// The Windows host identity of one entry: its volume serial number, its
/// file index and its creation time as Unix seconds and nanoseconds.
pub(crate) struct WindowsFileIdentity {
    pub(crate) device: u64,
    pub(crate) index: u64,
    pub(crate) created: i64,
    pub(crate) created_nsec: i64,
}

/// Reads `path`'s volume, file index and creation time without opening the
/// entry's data: attributes only, with traversal of neither links nor
/// content. Directories need the backup-semantics flag.
pub(crate) fn windows_file_identity(path: &Path) -> io::Result<WindowsFileIdentity> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        OPEN_EXISTING,
    };
    let wide: Vec<u16> = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
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
        return Err(io::Error::last_os_error());
    }
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    let read = unsafe { GetFileInformationByHandle(handle, &mut information) };
    unsafe { CloseHandle(handle) };
    if read == 0 {
        return Err(io::Error::last_os_error());
    }
    // FILETIME is 100ns ticks since 1601-01-01; Unix time starts
    // 11644473600 seconds later.
    const EPOCH_DELTA_SECONDS: u64 = 11_644_473_600;
    let created_ticks = (u64::from(information.ftCreationTime.dwHighDateTime) << 32)
        | u64::from(information.ftCreationTime.dwLowDateTime);
    let created_unix = (created_ticks / 10_000_000).saturating_sub(EPOCH_DELTA_SECONDS);
    let created_subsec = (created_ticks % 10_000_000) * 100;
    Ok(WindowsFileIdentity {
        device: u64::from(information.dwVolumeSerialNumber),
        index: (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow),
        created: created_unix as i64,
        created_nsec: created_subsec as i64,
    })
}
