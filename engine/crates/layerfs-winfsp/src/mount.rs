//! One mount over the shared services.
use crate::ffi;

/// A mounted WinFsp file system: the handle unmount drops.
pub struct Mounted {
    raw: *mut ffi::FSP_FILE_SYSTEM,
}

impl Drop for Mounted {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe { ffi::FspUnmount(self.raw) };
        }
    }
}

// The callback wiring lands with the operation mapping step; the
// handle type and the unmount drop are the mount face's skeleton.
