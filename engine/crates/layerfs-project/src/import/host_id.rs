//! Host file identity: the (device, inode) pair the acquisition layer keys
//! rows by. Unix reads it from stat metadata; Windows opens the file once
//! (no reparse-point following, the symlink_metadata discipline) and asks
//! GetFileInformationByHandle for the volume serial and file index - the
//! audited Win32 identity surface of this crate (the same shape the
//! sandbox and control crates keep for their FFI).
#[cfg(unix)]
pub(super) fn of_metadata(metadata: &std::fs::Metadata) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    (metadata.dev(), metadata.ino())
}

#[cfg(windows)]
#[allow(unsafe_code)]
pub mod windows {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;

    #[repr(C)]
    #[allow(non_snake_case)]
    struct BY_HANDLE_FILE_INFORMATION {
        dwFileAttributes: u32,
        ftCreationTime: [u32; 2],
        ftLastAccessTime: [u32; 2],
        ftLastWriteTime: [u32; 2],
        dwVolumeSerialNumber: u32,
        nFileSizeHigh: u32,
        nFileSizeLow: u32,
        nNumberOfLinks: u32,
        nFileIndexHigh: u32,
        nFileIndexLow: u32,
    }

    #[allow(unsafe_code)]
    extern "system" {
        #[link_name = "GetFileInformationByHandle"]
        fn GetFileInformationByHandle(file: usize, information: *mut BY_HANDLE_FILE_INFORMATION) -> u32;
    }

    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    /// (volume serial, file index) of the file `path` names, without
    /// following a reparse point. The plain (0, 0) face only when it
    /// cannot be opened or asked.
    pub(crate) fn of_path(path: &Path) -> (u64, u64) {
        let opened = OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
            .open(path);
        let Ok(file) = opened else {
            return (0, 0);
        };
        let mut information = BY_HANDLE_FILE_INFORMATION {
            dwFileAttributes: 0,
            ftCreationTime: [0; 2],
            ftLastAccessTime: [0; 2],
            ftLastWriteTime: [0; 2],
            dwVolumeSerialNumber: 0,
            nFileSizeHigh: 0,
            nFileSizeLow: 0,
            nNumberOfLinks: 0,
            nFileIndexHigh: 0,
            nFileIndexLow: 0,
        };
        let read = unsafe {
            GetFileInformationByHandle(file.as_raw_handle() as usize, &mut information)
        };
        if read == 0 {
            return (0, 0);
        }
        let index = (u64::from(information.nFileIndexHigh) << 32)
            | u64::from(information.nFileIndexLow);
        (u64::from(information.dwVolumeSerialNumber), index)
    }
}

#[cfg(unix)]
pub(super) fn of_path(path: &std::path::Path) -> (u64, u64) {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => of_metadata(&metadata),
        Err(_) => (0, 0),
    }
}

#[cfg(unix)]
pub(super) use of_metadata;

#[cfg(windows)]
pub(super) use windows::of_path;
