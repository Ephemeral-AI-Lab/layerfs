//! The hand-written WinFsp FFI: only what the mount face calls.
//! The import library ships beside the WinFsp installation.
#![allow(non_snake_case, non_camel_case_types, dead_code)]

use std::os::raw::{c_char, c_int, c_uint, c_void};

/// The file system callback table (a subset; fields the mount face reads).
#[repr(C)]
pub struct FSP_FILE_SYSTEM_INTERFACE {
    pub GetVolumeInfo: *const c_void,
    pub GetSecurityByName: *const c_void,
    pub Create: *const c_void,
    pub Open: *const c_void,
    pub Close: *const c_void,
    pub Read: *const c_void,
    pub Write: *const c_void,
    pub Flush: *const c_void,
    pub GetFileInfo: *const c_void,
    pub SetBasicInfo: *const c_void,
    pub SetFileSize: *const c_void,
    pub CanDelete: *const c_void,
    pub Rename: *const c_void,
    pub ReadDirectory: *const c_void,
    pub Overwrite: *const c_void,
    pub GetSecurity: *const c_void,
    pub SetSecurity: *const c_void,
}

/// One file system's static description, as FspMount reads it.
#[repr(C)]
pub struct FSP_FILE_SYSTEM {
    pub UserContext: *mut c_void,
    /// The interface pointer the mount owns.
    pub Interface: *const FSP_FILE_SYSTEM_INTERFACE,
}

extern "system" {
    /// Mounts one file system: the volume prefix names it (a local path is
    /// the Windows face).
    pub fn FspMount(
        P: *const c_void,
        FileSystem: *mut *mut FSP_FILE_SYSTEM,
        Params: *const c_void,
        Interface: *const FSP_FILE_SYSTEM_INTERFACE,
        InterfaceVersion: c_uint,
        Context: *mut c_void,
    ) -> c_int;

    /// Unmounts and frees one mounted file system.
    pub fn FspUnmount(FileSystem: *mut FSP_FILE_SYSTEM) -> c_int;
}
