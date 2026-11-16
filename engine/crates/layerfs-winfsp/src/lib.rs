//! The Workspace filesystem face on Windows: WinFsp file system callbacks
//! over the platform-neutral [`layerfs_mount_ports`] services, as
//! `layerfs-fuse` does for FUSE (#454 A2). The FFI to winfsp.dll is
//! hand-written - no new dependency - beside its import library.

pub mod ffi;
pub mod callbacks;
pub mod data;
pub mod write;
pub mod named;
pub mod directory;
pub mod services;

pub mod mount;
pub use mount::{mount, mount_workspace, Mounted};

// The callbacks and the mount loop land with their steps; the skeleton
// wires the crate into the workspace first.


/// Whether the WinFsp control device answers: lsdrv names a loaded
/// file, a portable layout without the registered device still fails,
/// so this opens the control device itself.
#[cfg(windows)]
pub fn device_live() -> bool {
    #[repr(C)]
    struct SecurityAttributes {
        length: u32,
        descriptor: *mut std::ffi::c_void,
        inherit: i32,
    }
    extern "system" {
        fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            security: *const SecurityAttributes,
            disposition: u32,
            flags: u32,
            template: *mut std::ffi::c_void,
        ) -> *mut std::ffi::c_void;
        fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
    }
    let name: Vec<u16> = "\\\\.\\WinFsp.Disk\0".encode_utf16().collect();
    let handle = unsafe {
        CreateFileW(name.as_ptr(), 0x80000000, 0x3, std::ptr::null(), 3, 0, core::ptr::null_mut())
    };
    if handle as isize == -1 {
        return false;
    }
    unsafe { CloseHandle(handle) };
    true
}
