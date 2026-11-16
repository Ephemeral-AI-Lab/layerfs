//! The WinFsp callbacks: the first real ones answer the volume face and
//! the root's name lookup, over static answers the mount face owns. The
//! services-backed operations (Read, ReadDirectory over the ports traits)
//! land with their own step.
use crate::ffi::*;
use std::os::raw::c_void;

/// The volume: 1 GB free of 1 GB, label "LayerFS".
pub unsafe extern "system" fn get_volume_info(
    _fs: *mut FSP_FILE_SYSTEM,
    info: *mut c_void,
) -> NTSTATUS {
    let info = info.cast::<i64>();
    unsafe {
        *info.add(0) = 1 << 30;
        *info.add(1) = 1 << 30;
    }
    STATUS_SUCCESS
}

/// The root answers FILE_ATTRIBUTE_DIRECTORY; everything else is not
/// found yet (the ports-backed lookup is the next step).
pub unsafe extern "system" fn get_security_by_name(
    _fs: *mut FSP_FILE_SYSTEM,
    file_name: *const u16,
    attributes: *mut u32,
    _security: *mut c_void,
    _security_size: *mut usize,
) -> NTSTATUS {
    let name = unsafe { wide_to_string(file_name) };
    let directory = matches!(name.as_str(), "\\" | "/" | "");
    if !directory {
        return STATUS_OBJECT_NAME_NOT_FOUND;
    }
    if !attributes.is_null() {
        unsafe { *attributes = 0x10 };
    }
    if !_security_size.is_null() {
        unsafe { *_security_size = 0 };
    }
    STATUS_SUCCESS
}

/// Reads a NUL-terminated UTF-16 string.
unsafe fn wide_to_string(ptr: *const u16) -> String {
    let mut len = 0usize;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(ptr, len);
        String::from_utf16_lossy(slice)
    }
}
