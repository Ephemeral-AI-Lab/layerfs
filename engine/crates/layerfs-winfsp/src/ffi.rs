//! The hand-written WinFsp FFI: the callback interface table and the
//! file-system lifecycle entry points. Signatures use opaque pointers
//! where the mount face does not read the fields yet.
#![allow(non_snake_case, non_camel_case_types, dead_code)]

use std::os::raw::{c_void, c_ulong};

pub type NTSTATUS = i32;
pub const STATUS_SUCCESS: NTSTATUS = 0;
pub const STATUS_NOT_IMPLEMENTED: NTSTATUS = 0xC0000002u32 as i32;
pub const STATUS_OBJECT_NAME_NOT_FOUND: NTSTATUS = 0xC0000034u32 as i32;
/// FSP_FSD_INTERFACE_VERSION from winfsp.h.
pub const FSP_FSD_INTERFACE_VERSION: u32 = 5;

/// The file system handle.
#[repr(C)]
pub struct FSP_FILE_SYSTEM {
    pub UserContext: *mut c_void,
}

/// The volume parameters (a subset the create call reads; opaque rest).
#[repr(C)]
pub struct FSP_FSCTL_VOLUME_PARAMS {
    pub SectorSize: u16,
    pub SectorsPerAllocationUnit: u16,
    pub MaxComponentLength: u16,
    pub VolumeCreationTime: u64,
    pub VolumeSerialNumber: u32,
    pub FileInfoTimeout: u32,
    pub CaseSensitiveSearch: u32,
    pub CasePreservedNames: u32,
    pub UnicodeOnDisk: u32,
    pub PersistentAcls: u32,
    pub ReparsePoints: u32,
    pub ReparsePointsAccessCheck: u32,
    pub NamedStreams: u32,
    pub ReadOnlyVolume: u32,
    pub PostCleanupWhenModifiedOnly: u32,
    pub PassQueryDirectoryFileName: u32,
    pub Uint16: [u8; 2],
    pub FswDeviceId: u32,
    pub Flags: u32,
}

/// The 23-slot callback table in WinFsp order (winfsp.h). Opaque
/// pointer parameters fill as each operation lands.
#[derive(Default)]
#[repr(C)]
pub struct FSP_FILE_SYSTEM_INTERFACE {
    pub GetVolumeInfo: Option<unsafe extern "system" fn(*mut FSP_FILE_SYSTEM, *mut c_void) -> NTSTATUS>,
    pub SetVolumeLabel: Option<unsafe extern "system" fn(*mut FSP_FILE_SYSTEM, *const u16, *mut c_void) -> NTSTATUS>,
    pub GetSecurityByName: Option<unsafe extern "system" fn(*mut FSP_FILE_SYSTEM, *const u16, *mut u32, *mut c_void, *mut usize) -> NTSTATUS>,
    pub Create: Option<unsafe extern "system" fn() -> NTSTATUS>,
    pub Open: Option<unsafe extern "system" fn(*mut FSP_FILE_SYSTEM, *const u16, u32, *mut c_void, *mut c_void, *const c_void, u32) -> NTSTATUS>,
    pub Overwrite: Option<unsafe extern "system" fn() -> NTSTATUS>,
    pub Read: Option<unsafe extern "system" fn(*mut FSP_FILE_SYSTEM, *mut c_void, *mut c_void, u64, *mut u32) -> NTSTATUS>,
    pub Write: Option<unsafe extern "system" fn(*mut FSP_FILE_SYSTEM, *mut c_void, *const c_void, u64, *mut u32, u32, *mut c_void) -> NTSTATUS>,
    pub Flush: Option<unsafe extern "system" fn() -> NTSTATUS>,
    pub GetFileInfo: Option<unsafe extern "system" fn(*mut FSP_FILE_SYSTEM, *mut c_void, *mut c_void) -> NTSTATUS>,
    pub SetBasicInfo: Option<unsafe extern "system" fn() -> NTSTATUS>,
    pub SetFileSize: Option<unsafe extern "system" fn() -> NTSTATUS>,
    pub CanDelete: Option<unsafe extern "system" fn() -> NTSTATUS>,
    pub Rename: Option<unsafe extern "system" fn() -> NTSTATUS>,
    pub GetSecurity: Option<unsafe extern "system" fn() -> NTSTATUS>,
    pub SetSecurity: Option<unsafe extern "system" fn() -> NTSTATUS>,
    pub ReadDirectory: Option<unsafe extern "system" fn(*mut FSP_FILE_SYSTEM, *mut c_void, *const u16, *const u16, *mut c_void, u32, *mut u32) -> NTSTATUS>,
    pub ResolveReparsePoints: Option<unsafe extern "system" fn() -> NTSTATUS>,
    pub GetReparsePoint: Option<unsafe extern "system" fn() -> NTSTATUS>,
    pub SetReparsePoint: Option<unsafe extern "system" fn() -> NTSTATUS>,
    pub DeleteReparsePoint: Option<unsafe extern "system" fn() -> NTSTATUS>,
    pub GetStreamInfo: Option<unsafe extern "system" fn() -> NTSTATUS>,
    pub GetDirInfoByName: Option<unsafe extern "system" fn() -> NTSTATUS>,
    pub Control: Option<unsafe extern "system" fn() -> NTSTATUS>,
}

extern "system" {
    pub fn FspFileSystemCreate(
        DevicePath: *const u16,
        VolumeParams: *const FSP_FSCTL_VOLUME_PARAMS,
        Interface: *const FSP_FILE_SYSTEM_INTERFACE,
        PFileSystem: *mut *mut FSP_FILE_SYSTEM,
    ) -> NTSTATUS;
    pub fn FspFileSystemDelete(FileSystem: *mut FSP_FILE_SYSTEM);
    pub fn FspFileSystemSetMountPoint(FileSystem: *mut FSP_FILE_SYSTEM, MountPoint: *const u16);
    pub fn FspFileSystemRemoveMountPoint(FileSystem: *mut FSP_FILE_SYSTEM);
    pub fn FspFileSystemStartDispatcher(FileSystem: *mut FSP_FILE_SYSTEM, ThreadCount: c_ulong) -> NTSTATUS;
    pub fn FspFileSystemStopDispatcher(FileSystem: *mut FSP_FILE_SYSTEM);
}
