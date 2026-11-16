//! One mount over the shared services.
use crate::ffi;
use crate::services::Context;

/// A mounted WinFsp file system: dropping it stops the dispatcher,
/// removes the mount point and deletes the file system object.
pub struct Mounted {
    raw: *mut ffi::FSP_FILE_SYSTEM,
    context: Option<Box<Context>>,
}

impl Drop for Mounted {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe {
                ffi::FspFileSystemStopDispatcher(self.raw);
                ffi::FspFileSystemRemoveMountPoint(self.raw);
                ffi::FspFileSystemDelete(self.raw);
            }
        }
        drop(self.context.take());
    }
}

/// Creates, mounts and starts one file system: the callbacks reach the
/// services through the context the file system UserContext holds.
pub fn mount(
    mount_point: &str,
    interface: ffi::FSP_FILE_SYSTEM_INTERFACE,
    context: Context,
) -> Result<Mounted, String> {
    let params = ffi::FSP_FSCTL_VOLUME_PARAMS {
        SectorSize: 512,
        SectorsPerAllocationUnit: 1,
        MaxComponentLength: 255,
        VolumeCreationTime: 0,
        VolumeSerialNumber: 0,
        FileInfoTimeout: 1000,
        CaseSensitiveSearch: 0,
        CasePreservedNames: 1,
        UnicodeOnDisk: 1,
        PersistentAcls: 1,
        ReparsePoints: 0,
        ReparsePointsAccessCheck: 0,
        NamedStreams: 0,
        ReadOnlyVolume: 0,
        PostCleanupWhenModifiedOnly: 1,
        PassQueryDirectoryFileName: 0,
        Uint16: [0; 2],
        FswDeviceId: 0,
        Flags: 0,
    };
    let device: Vec<u16> = "LayerFS\0".encode_utf16().collect();
    let point: Vec<u16> = mount_point.encode_utf16().chain(std::iter::once(0)).collect();
    let interface = Box::leak(Box::new(interface));
    let mut context = Box::new(context);
    let mut raw: *mut ffi::FSP_FILE_SYSTEM = std::ptr::null_mut();
    let status = unsafe {
        ffi::FspFileSystemCreate(device.as_ptr(), &params, interface, &mut raw)
    };
    if status != ffi::STATUS_SUCCESS {
        return Err(format!("FspFileSystemCreate: {status:#x}"));
    }
    unsafe {
        (*raw).UserContext = context.as_mut() as *mut Context as *mut _;
    }
    unsafe { ffi::FspFileSystemSetMountPoint(raw, point.as_ptr()) };
    let status = unsafe { ffi::FspFileSystemStartDispatcher(raw, 0) };
    if status != ffi::STATUS_SUCCESS {
        unsafe {
            ffi::FspFileSystemRemoveMountPoint(raw);
            ffi::FspFileSystemDelete(raw);
        }
        return Err(format!("FspFileSystemStartDispatcher: {status:#x}"));
    }
    Ok(Mounted { raw, context: Some(context) })
}
