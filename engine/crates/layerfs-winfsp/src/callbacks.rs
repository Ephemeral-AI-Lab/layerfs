//! The WinFsp callbacks: the volume face, the root lookup, the root
//! Open and the first ReadDirectory window (the two dot entries).
use crate::ffi::*;
use crate::services::{block_on, Context};
use std::os::raw::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Reads the context a mounted file system carries.
unsafe fn context(fs: *mut FSP_FILE_SYSTEM) -> Option<&'static Context> {
    unsafe { (fs as *const c_void as *const Context).as_ref() }
}

/// One open handle the callbacks hand out.
static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

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

/// Open: the root opens (a fresh handle); everything else is not found.
pub unsafe extern "system" fn open(
    _fs: *mut FSP_FILE_SYSTEM,
    file_name: *const u16,
    _create_flags: u32,
    file_context: *mut c_void,
    file_info: *mut c_void,
    _security_descriptor: *const c_void,
    _granted_access: u32,
) -> NTSTATUS {
    let name = unsafe { wide_to_string(file_name) };
    if !matches!(name.as_str(), "\\" | "/" | "") {
        return STATUS_OBJECT_NAME_NOT_FOUND;
    }
    if !file_context.is_null() {
        let handle = Box::into_raw(Box::new(NEXT_HANDLE.fetch_add(1, Ordering::Relaxed)));
        unsafe { *(file_context as *mut *mut u64) = handle.cast() };
    }
    if !file_info.is_null() {
        let info = file_info.cast::<u32>();
        unsafe { *info = 0x10 };
    }
    STATUS_SUCCESS
}

/// The DIR_INFO header size (winfsp.h static assert: 104).
const ENTRY_SIZE: usize = 104;

/// One dot entry: attributes=directory, no times, the name after the header.
fn dot_entry(name: &str) -> Vec<u8> {
    let mut bytes = vec![0u8; ENTRY_SIZE + name.len() * 2 + 2];
    bytes[0..2].copy_from_slice(&(ENTRY_SIZE as u16).to_le_bytes());
    bytes[2..6].copy_from_slice(&0x10u32.to_le_bytes());
    let name_units: Vec<u16> = name.encode_utf16().collect();
    for (i, unit) in name_units.iter().enumerate() {
        let at = ENTRY_SIZE + i * 2;
        bytes[at..at + 2].copy_from_slice(&unit.to_le_bytes());
    }
    bytes
}

/// ReadDirectory: the ports-backed window over the DirectoryStream,
/// falling to the static dot face when the services refuse.
pub unsafe extern "system" fn read_directory(
    fs: *mut FSP_FILE_SYSTEM,
    _file: *mut c_void,
    _pattern: *const u16,
    marker: *const u16,
    buffer: *mut c_void,
    length: u32,
    bytes_transferred: *mut u32,
) -> NTSTATUS {
    let Some(context) = (unsafe { context(fs) }) else {
        return STATUS_NOT_IMPLEMENTED;
    };
    let marker_present = !marker.is_null() && unsafe { *marker } != 0;
    let offset = if marker_present { 1 } else { 0 }; // past the dots
    let window = match crate::directory::root_window(context, 1, offset) {
        Ok(window) => window,
        Err(_) => return STATUS_NOT_IMPLEMENTED,
    };
    if (length as usize) < window.len() {
        return STATUS_NOT_IMPLEMENTED;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(window.as_ptr(), buffer.cast(), window.len());
        *bytes_transferred = window.len() as u32;
    }
    STATUS_SUCCESS
}

/// Read: the ports-backed data window (the serial is the handle the
/// open made; the file context names it).
pub unsafe extern "system" fn read(
    fs: *mut FSP_FILE_SYSTEM,
    file: *mut c_void,
    buffer: *mut c_void,
    offset: u64,
    length: *mut u32,
) -> NTSTATUS {
    let Some(context) = (unsafe { context(fs) }) else {
        return STATUS_NOT_IMPLEMENTED;
    };
    let serial = unsafe { *(file as *const u64) };
    let want = if length.is_null() { 0 } else { unsafe { *length } };
    let data = match crate::data::data(context, serial, serial, offset, want) {
        Ok(data) => data,
        Err(_) => return STATUS_NOT_IMPLEMENTED,
    };
    if !length.is_null() {
        unsafe { *length = data.len() as u32 };
    }
    if !data.is_empty() {
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), buffer.cast(), data.len()) };
    }
    STATUS_SUCCESS
}
/// Write: the ports-backed mutation (the serial is the handle).
pub unsafe extern "system" fn write(
    fs: *mut FSP_FILE_SYSTEM,
    file: *mut c_void,
    buffer: *const c_void,
    offset: u64,
    length: u32,
    _to_end: u32,
    _constrained: u32,
    transferred: *mut u32,
    _file_info: *mut c_void,
) -> NTSTATUS {
    let Some(context) = (unsafe { context(fs) }) else {
        return STATUS_NOT_IMPLEMENTED;
    };
    let serial = unsafe { *(file as *const u64) };
    let mut data = vec![0u8; length as usize];
    if length > 0 {
        unsafe { std::ptr::copy_nonoverlapping(buffer as *const u8, data.as_mut_ptr(), length as usize) };
    }
    match crate::write::write(context, serial, serial, offset, data) {
        Ok(end) => {
            if !transferred.is_null() {
                unsafe { *transferred = (end - offset) as u32 };
            }
            STATUS_SUCCESS
        }
        Err(_) => STATUS_NOT_IMPLEMENTED,
    }
}

/// GetFileInfo: the root answers its directory facts.
pub unsafe extern "system" fn get_file_info(
    _fs: *mut FSP_FILE_SYSTEM,
    file: *mut c_void,
    info: *mut c_void,
) -> NTSTATUS {
    let _serial = unsafe { *(file as *const u64) };
    let info = info.cast::<u32>();
    unsafe { *info = 0x10 };
    STATUS_SUCCESS
}

/// Flush: the Workspace owns its durability; nothing held here.
pub unsafe extern "system" fn flush(
    _fs: *mut FSP_FILE_SYSTEM,
    _file: *mut c_void,
    _file_info: *mut c_void,
) -> NTSTATUS {
    STATUS_SUCCESS
}

/// SetFileSize: truncation and growth by the ports attributes input.
pub unsafe extern "system" fn set_file_size(
    fs: *mut FSP_FILE_SYSTEM,
    file: *mut c_void,
    new_size: u64,
    _allocation: u32,
    _file_info: *mut c_void,
) -> NTSTATUS {
    let Some(context) = (unsafe { context(fs) }) else {
        return STATUS_NOT_IMPLEMENTED;
    };
    let serial = unsafe { *(file as *const u64) };
    match crate::write::truncate(context, serial, new_size) {
        Ok(()) => STATUS_SUCCESS,
        Err(_) => STATUS_NOT_IMPLEMENTED,
    }
}

/// CanDelete: allowed; the mutation face performs the removal.
pub unsafe extern "system" fn can_delete(
    _fs: *mut FSP_FILE_SYSTEM,
    _file: *mut c_void,
    _name: *const u16,
) -> NTSTATUS {
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
