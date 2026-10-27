//! The overlay's one audited Windows FFI surface: a file's real identity.
//!
//! This follows the audited-module precedent of the storage codec, the
//! persistence file-control and Windows lanes, and the project host probe:
//! one owner-authorized module of raw Win32 calls, allowed by the boundary
//! guard and compiled on Windows only; no other module in this crate may
//! use unsafe Rust. The allocation's identity checks need the volume serial
//! number, the file index and the link count, which std does not expose.
#![allow(unsafe_code)]
use std::fs::File;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use windows_sys::Win32::Storage::FileSystem::BY_HANDLE_FILE_INFORMATION;

/// One entry's Windows identity.
pub struct WindowsIdentity {
    pub device: u64,
    pub index: u64,
    pub links: u64,
}

/// The identity of the entry an open file already holds, without opening
/// anything new.
pub fn windows_handle_identity(file: &File) -> io::Result<WindowsIdentity> {
    use windows_sys::Win32::Storage::FileSystem::GetFileInformationByHandle;
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    let read = unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) };
    finish(read, information)
}

/// The identity of `path`'s entry, read as attributes only: no data access,
/// no link traversal; directories need the backup-semantics flag.
/// The entry's on-disk allocation in bytes (compression/sparse aware),
/// read through FileStandardInfo on the open handle.
pub fn windows_allocation_size(file: &File) -> io::Result<u64> {
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandleEx, FILE_STANDARD_INFO, FileStandardInfo,
    };
    let mut info = FILE_STANDARD_INFO::default();
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileStandardInfo,
            &mut info as *mut FILE_STANDARD_INFO as *mut core::ffi::c_void,
            std::mem::size_of::<FILE_STANDARD_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(info.AllocationSize.max(0) as u64)
}

pub fn windows_path_identity(path: &Path) -> io::Result<WindowsIdentity> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, GetFileInformationByHandle, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
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
    finish(read, information)
}

fn finish(read: i32, information: BY_HANDLE_FILE_INFORMATION) -> io::Result<WindowsIdentity> {
    if read == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(WindowsIdentity {
        device: u64::from(information.dwVolumeSerialNumber),
        index: (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow),
        links: u64::from(information.nNumberOfLinks),
    })
}

/// The bytes available to the caller on `directory`'s volume.
pub(crate) fn windows_free_bytes(directory: &Path) -> io::Result<u64> {
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = directory
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut free: u64 = 0;
    let read = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if read == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(free)
}
