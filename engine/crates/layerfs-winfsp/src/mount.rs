//! One mount over the shared services.
use crate::ffi;
use crate::services::Context;

/// A mounted WinFsp file system: dropping it stops the dispatcher,
/// removes the mount point and deletes the file system object.
pub struct Mounted {
    raw: *mut ffi::FSP_FILE_SYSTEM,
    context: Option<Box<Context>>,
}

// SAFETY: the dispatcher reads the handle under its own
// synchronization; the context box moves with the mount.
unsafe impl Send for Mounted {}


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
        _pad0: 0,
        VolumeCreationTime: 0,
        VolumeSerialNumber: 0,
        TransactTimeout: 0,
        IrpTimeout: 0,
        IrpCapacity: 0,
        FileInfoTimeout: 1000,
        // CasePreservedNames(1), UnicodeOnDisk(1),
        // PersistentAcls(1), PostCleanupWhenModifiedOnly(13):
        flags_a: 1 << 1 | 1 << 2 | 1 << 3 | 1 << 13,
        flags_b: 0,
        flags_c: 0,
        flags_d: 0,
        Prefix: [0; 32],
        FileSystemName: [0; 32],
        tail: [0; 96],
    };
    let device: Vec<u16> = "WinFsp.Disk\0".encode_utf16().collect();
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

/// The full operation table over the daemon services.
pub fn interface() -> ffi::FSP_FILE_SYSTEM_INTERFACE {
    ffi::FSP_FILE_SYSTEM_INTERFACE {
        GetVolumeInfo: Some(crate::callbacks::get_volume_info),
        SetVolumeLabel: None,
        GetSecurityByName: Some(crate::callbacks::get_security_by_name),
        Create: Some(crate::callbacks::create),
        Open: Some(crate::callbacks::open),
        Overwrite: None,
        Read: Some(crate::callbacks::read),
        Write: Some(crate::callbacks::write),
        Flush: Some(crate::callbacks::flush),
        GetFileInfo: Some(crate::callbacks::get_file_info),
        SetBasicInfo: Some(crate::callbacks::set_basic_info),
        SetFileSize: Some(crate::callbacks::set_file_size),
        CanDelete: Some(crate::callbacks::can_delete),
        Rename: Some(crate::callbacks::rename),
        GetSecurity: None,
        SetSecurity: None,
        ReadDirectory: Some(crate::callbacks::read_directory),
        ResolveReparsePoints: None,
        GetReparsePoint: None,
        SetReparsePoint: None,
        DeleteReparsePoint: None,
        GetStreamInfo: None,
        GetDirInfoByName: None,
        Control: None,
    }
}

/// Mounts one Workspace over any mount services: every wired operation
/// answers, and the mount point names a drive or directory. The daemon
/// crate wraps its BoundWorkspace in Mounted and calls this.
pub fn mount_workspace(
    mount_point: &str,
    services: std::sync::Arc<dyn layerfs_mount_ports::ports::MountServices>,
) -> Result<Mounted, String> {
    let context = Context {
        services,
        fence: layerfs_mount_ports::ports::Fence::default(),
        mount: crate::services::mint_mount(),
    };
    mount(mount_point, interface(), context)
}