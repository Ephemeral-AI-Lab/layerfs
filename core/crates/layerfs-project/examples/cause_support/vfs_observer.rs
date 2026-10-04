//! Diagnostic-only explicit initialization of the sealed delegated VFS library.
#[cfg(target_os = "macos")]
extern "C" {
    fn dlsym(handle: *mut std::ffi::c_void, name: *const std::ffi::c_char)
        -> *mut std::ffi::c_void;
}
pub fn initialize() -> Result<(), String> {
    match std::env::var("LAYERFS_CAUSE_VFS_ENABLED") {
        Err(std::env::VarError::NotPresent) => return Ok(()),
        Ok(value) if value == "1" => (),
        _ => return Err("invalid delegated VFS observer selection".into()),
    }
    #[cfg(target_os = "macos")]
    {
        // SAFETY: dyld RTLD_DEFAULT is documented as -2 on this host. The sealed
        // observer exports exactly this process-lifetime C signature. Initialization
        // runs once after SQLLOG configuration and before any product connection.
        let pointer = unsafe {
            dlsym(
                (-2isize) as *mut std::ffi::c_void,
                c"cause_vfs_initialize".as_ptr(),
            )
        };
        if pointer.is_null() {
            return Err("delegated VFS observer symbol unavailable".into());
        }
        let initialize: unsafe extern "C" fn() -> std::ffi::c_int =
            unsafe { std::mem::transmute(pointer) };
        let code = unsafe { initialize() };
        if code != 0 {
            return Err(format!("delegated VFS initialization refused:{code}"));
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("delegated VFS observer unavailable on this platform".into())
    }
}
