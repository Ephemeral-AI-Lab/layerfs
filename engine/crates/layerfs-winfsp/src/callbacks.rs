//! The WinFsp callbacks: the volume face, the root lookup and the
//! first services-backed operation (ReadDirectory over the ports).
use crate::ffi::*;
use crate::services::{block_on, Context};
use std::os::raw::c_void;
use std::sync::Arc;

/// Reads the context a mounted file system carries.
unsafe fn context(fs: *mut FSP_FILE_SYSTEM) -> Option<&'static Context> {
    unsafe { (fs as *const c_void as *const Context).as_ref() }
}

/// The volume: 1 GB free of 1 GB.
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
/// found until the ports-backed lookup lands.
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
        String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len))
    }
}

/// ReadDirectory: opens the request face and asks for the directory
/// window. The reply encoding into the WinFsp buffer lands with the
/// next step; this arm proves the context reach and the handshake.
pub unsafe extern "system" fn read_directory(
    fs: *mut FSP_FILE_SYSTEM,
    _file: *mut c_void,
    _pattern: *const u16,
    _marker: *const c_void,
    _buffer: *mut c_void,
    _length: u32,
    _bytes_transferred: *mut u32,
) -> NTSTATUS {
    let Some(context) = (unsafe { context(fs) }) else {
        return STATUS_NOT_IMPLEMENTED;
    };
    let _services: Arc<_> = match context.request() {
        Ok(services) => services,
        Err(_) => return STATUS_NOT_IMPLEMENTED,
    };
    // The directory_visit future drives here (the next step); the
    // handshake reach is this arm proof.
    let _ = block_on(async { () });
    STATUS_SUCCESS
}
