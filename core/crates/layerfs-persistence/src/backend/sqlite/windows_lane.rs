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
}

/// The lane's kernel object name: SHA-256 of the resolved directory's
/// encoded bytes, truncated to 64 bits. A versioned-stable digest, not
/// `DefaultHasher`, whose algorithm may change between Rust releases and
/// silently split one directory onto two lanes across builds (K3 review F1).
pub(crate) fn lane_name(directory: &Path) -> String {
    let resolved = std::fs::canonicalize(directory).unwrap_or_else(|_| directory.to_path_buf());
    let digest = Sha256::digest(resolved.as_os_str().as_encoded_bytes());
    let mut tag = [0u8; 8];
    tag.copy_from_slice(&digest[..8]);
    format!("Local\\layerfs-writer-lane-{:016x}", u64::from_be_bytes(tag))
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
            })
        } else {
            unsafe { CloseHandle(handle) };
            Err(())
        }
    }

    /// Releases the lane and destroys this handle; a failed release returns
    /// the retained lane unchanged.
    pub(crate) fn release(self) -> Result<(), Self> {
        let handle = self.handle as windows_sys::Win32::Foundation::HANDLE;
        if unsafe { ReleaseMutex(handle) } == 0 {
            return Err(Self {
                handle: self.handle,
            });
        }
        unsafe { CloseHandle(handle) };
        Ok(())
    }
}

/// The filesystem name of the local volume holding `directory`, for the
/// kernel guard's allowlist. Only a local drive letter is examined: UNC,
/// device and verbatim prefixes are network or device paths whose locks do
/// not coordinate Store writers.
pub fn windows_volume_filesystem(directory: &Path) -> io::Result<String> {
    use std::path::{Component, Prefix};
    let root = match directory.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(letter) => format!("{}:\\", letter as char),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "a network or device path is not a local volume",
                ))
            }
        },
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "the directory has no volume prefix",
            ))
        }
    };
    let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
    let mut filesystem = [0u16; 16];
    let examined = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetVolumeInformationW(
            wide.as_ptr(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            filesystem.as_mut_ptr(),
            filesystem.len() as u32,
        )
    };
    if examined == 0 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("the volume {root} cannot be examined"),
        ));
    }
    let end = filesystem.iter().position(|&wide| wide == 0).unwrap_or(0);
    Ok(String::from_utf16_lossy(&filesystem[..end]))
}
