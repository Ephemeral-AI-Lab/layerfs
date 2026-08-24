//! Object-safe native workspace boundary.

use std::any::Any;
use std::fmt;
use std::io::{self, Read, Seek, Write};
use std::path::Path;

pub const MAX_NATIVE_XATTR_BYTES: usize = 1024 * 1024;

pub trait DirectoryHandle: Send {
    fn as_any(&self) -> &dyn Any;
}
pub trait RegularFileHandle: Read + Write + Seek + Send {
    fn as_any(&self) -> &dyn Any;
}
pub trait OwnedTempHandle: Read + Write + Seek + Send {
    fn as_any(&self) -> &dyn Any;
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
}
pub trait NamePreflight: Send {
    fn add(&mut self, name: &[u8]) -> Result<()>;
    fn finish(self: Box<Self>) -> Result<()>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspacePolicy {
    ManagedCreateOwned,
    ManagedPrivate,
    ExternalCooperative,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeKind {
    Directory,
    RegularFile,
    Symlink,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeEntry {
    pub name: Vec<u8>,
    pub kind: NativeKind,
    pub token: Vec<u8>,
    pub hard_link_key: Option<Vec<u8>>,
    pub link_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeMetadata {
    pub mode: u32,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
    pub xattrs: Vec<(Vec<u8>, Vec<u8>)>,
    pub acl: Option<Vec<u8>>,
    pub bsd_flags: u32,
}

#[derive(Debug)]
pub enum DriverError {
    Unsupported,
    NativeProtected,
    Conflict,
    VisibilityAmbiguous,
    DurabilityAmbiguous,
    Io(io::Error),
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => f.write_str("native operation is unsupported"),
            Self::NativeProtected => f.write_str("native object is protected"),
            Self::Conflict => f.write_str("native object changed"),
            Self::VisibilityAmbiguous => f.write_str("native visibility is ambiguous"),
            Self::DurabilityAmbiguous => f.write_str("native durability is ambiguous"),
            Self::Io(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for DriverError {}

impl From<io::Error> for DriverError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub type Result<T> = std::result::Result<T, DriverError>;

pub trait ProjectionWorkspace: Send {
    fn root_directory(&self) -> Result<Box<dyn DirectoryHandle>>;
    fn enumerate_at<'a>(
        &'a self,
        parent: &'a dyn DirectoryHandle,
    ) -> Result<Box<dyn Iterator<Item = Result<NativeEntry>> + 'a>>;
    fn open_directory_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<Box<dyn DirectoryHandle>>;
    fn duplicate_directory(
        &self,
        directory: &dyn DirectoryHandle,
    ) -> Result<Box<dyn DirectoryHandle>>;
    fn directory_token(&self, directory: &dyn DirectoryHandle) -> Result<Vec<u8>>;
    fn directory_identity(&self, directory: &dyn DirectoryHandle) -> Result<Vec<u8>>;
    fn revalidate_root_binding(&self) -> Result<()>;
    fn begin_name_preflight(&self) -> Result<Box<dyn NamePreflight>>;
    fn open_regular_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<Box<dyn RegularFileHandle>>;
    fn open_regular_read_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<Box<dyn RegularFileHandle>>;
    fn set_regular_len(&self, file: &mut dyn RegularFileHandle, len: u64) -> Result<()>;
    fn sync_regular(&self, file: &mut dyn RegularFileHandle) -> Result<()>;
    fn read_link_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<Vec<u8>>;
    fn read_metadata_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<NativeMetadata>;
    fn token_at(&self, parent: &dyn DirectoryHandle, name: &[u8]) -> Result<Vec<u8>>;
    fn identity_at(&self, parent: &dyn DirectoryHandle, name: &[u8]) -> Result<Vec<u8>>;
    fn read_root_metadata(&self) -> Result<NativeMetadata>;
    fn read_directory_metadata(&self, directory: &dyn DirectoryHandle) -> Result<NativeMetadata>;
    fn create_directory_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
    ) -> Result<Box<dyn DirectoryHandle>>;
    fn create_temp_at(&self, parent: &dyn DirectoryHandle) -> Result<Box<dyn OwnedTempHandle>>;
    fn clone_temp_from_regular(
        &self,
        source: &dyn RegularFileHandle,
    ) -> Result<Box<dyn OwnedTempHandle>>;
    fn read_temp_metadata(&self, temp: &dyn OwnedTempHandle) -> Result<NativeMetadata>;
    fn set_temp_metadata(
        &self,
        temp: &mut dyn OwnedTempHandle,
        metadata: &NativeMetadata,
    ) -> Result<()>;
    fn set_entry_metadata(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: &[u8],
        metadata: &NativeMetadata,
    ) -> Result<()>;
    fn atomic_replace(
        &self,
        temp: Box<dyn OwnedTempHandle>,
        parent: &dyn DirectoryHandle,
        name: &[u8],
    ) -> Result<()>;
    fn atomic_replace_checked(
        &self,
        temp: Box<dyn OwnedTempHandle>,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<()>;
    fn create_symlink_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        target: &[u8],
        metadata: &NativeMetadata,
    ) -> Result<()>;
    fn atomic_replace_symlink(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: Option<&[u8]>,
        target: &[u8],
        metadata: &NativeMetadata,
    ) -> Result<()>;
    fn create_hard_link_at(
        &self,
        source_parent: &dyn DirectoryHandle,
        source: &[u8],
        source_expected: &[u8],
        target_parent: &dyn DirectoryHandle,
        target: &[u8],
    ) -> Result<()>;
    fn finish_hard_link_at(
        &self,
        source_parent: &dyn DirectoryHandle,
        source: &[u8],
        source_expected: &[u8],
        metadata: &NativeMetadata,
    ) -> Result<()>;
    fn rename_at(
        &self,
        source_parent: &dyn DirectoryHandle,
        source: &[u8],
        target_parent: &dyn DirectoryHandle,
        target: &[u8],
    ) -> Result<()>;
    fn unlink_regular_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: &[u8],
    ) -> Result<()>;
    fn unlink_symlink_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: &[u8],
    ) -> Result<()>;
    fn remove_directory_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: &[u8],
    ) -> Result<()>;
    fn sync_directory(&self, directory: &dyn DirectoryHandle) -> Result<()>;
    fn set_root_metadata(&self, metadata: &NativeMetadata) -> Result<()>;
    fn remove_owned_root(&self, expected_identity: &[u8]) -> Result<()>;
}

pub trait ProjectionDriver: Send + Sync {
    fn open_workspace(
        &self,
        path: &Path,
        policy: WorkspacePolicy,
        store_id: [u8; 32],
    ) -> Result<Box<dyn ProjectionWorkspace>>;
    fn recover_owned_workspaces(&self, _parent: &Path, _store_id: [u8; 32]) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MemoryWorkspace;
    struct MemoryPreflight;
    impl NamePreflight for MemoryPreflight {
        fn add(&mut self, _name: &[u8]) -> Result<()> {
            Ok(())
        }
        fn finish(self: Box<Self>) -> Result<()> {
            Ok(())
        }
    }

    struct Dir;
    impl DirectoryHandle for Dir {
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    impl ProjectionWorkspace for MemoryWorkspace {
        fn root_directory(&self) -> Result<Box<dyn DirectoryHandle>> {
            Ok(Box::new(Dir))
        }

        fn enumerate_at<'a>(
            &'a self,
            _parent: &'a dyn DirectoryHandle,
        ) -> Result<Box<dyn Iterator<Item = Result<NativeEntry>> + 'a>> {
            Ok(Box::new(
                [NativeEntry {
                    name: b"file".to_vec(),
                    kind: NativeKind::RegularFile,
                    token: vec![1],
                    hard_link_key: None,
                    link_count: 1,
                }]
                .into_iter()
                .map(Ok),
            ))
        }

        fn open_directory_at(
            &self,
            _parent: &dyn DirectoryHandle,
            _name: &[u8],
            _expected: Option<&[u8]>,
        ) -> Result<Box<dyn DirectoryHandle>> {
            Err(DriverError::Unsupported)
        }
        fn duplicate_directory(
            &self,
            _directory: &dyn DirectoryHandle,
        ) -> Result<Box<dyn DirectoryHandle>> {
            Err(DriverError::Unsupported)
        }
        fn directory_token(&self, _directory: &dyn DirectoryHandle) -> Result<Vec<u8>> {
            Ok(vec![1])
        }
        fn directory_identity(&self, _directory: &dyn DirectoryHandle) -> Result<Vec<u8>> {
            Ok(vec![1])
        }
        fn revalidate_root_binding(&self) -> Result<()> {
            Ok(())
        }
        fn begin_name_preflight(&self) -> Result<Box<dyn NamePreflight>> {
            Ok(Box::new(MemoryPreflight))
        }
        fn open_regular_at(
            &self,
            _parent: &dyn DirectoryHandle,
            _name: &[u8],
            _expected: Option<&[u8]>,
        ) -> Result<Box<dyn RegularFileHandle>> {
            Err(DriverError::Unsupported)
        }
        fn open_regular_read_at(
            &self,
            _parent: &dyn DirectoryHandle,
            _name: &[u8],
            _expected: Option<&[u8]>,
        ) -> Result<Box<dyn RegularFileHandle>> {
            Err(DriverError::Unsupported)
        }
        fn set_regular_len(&self, _file: &mut dyn RegularFileHandle, _len: u64) -> Result<()> {
            Err(DriverError::Unsupported)
        }
        fn sync_regular(&self, _file: &mut dyn RegularFileHandle) -> Result<()> {
            Err(DriverError::Unsupported)
        }
        fn read_link_at(
            &self,
            _parent: &dyn DirectoryHandle,
            _name: &[u8],
            _expected: Option<&[u8]>,
        ) -> Result<Vec<u8>> {
            Err(DriverError::Unsupported)
        }
        fn read_metadata_at(
            &self,
            _parent: &dyn DirectoryHandle,
            _name: &[u8],
            _expected: Option<&[u8]>,
        ) -> Result<NativeMetadata> {
            Err(DriverError::Unsupported)
        }
        fn token_at(&self, _parent: &dyn DirectoryHandle, _name: &[u8]) -> Result<Vec<u8>> {
            Err(DriverError::Unsupported)
        }
        fn identity_at(&self, _parent: &dyn DirectoryHandle, _name: &[u8]) -> Result<Vec<u8>> {
            Err(DriverError::Unsupported)
        }
        fn read_root_metadata(&self) -> Result<NativeMetadata> {
            Err(DriverError::Unsupported)
        }
        fn read_directory_metadata(
            &self,
            _directory: &dyn DirectoryHandle,
        ) -> Result<NativeMetadata> {
            Err(DriverError::Unsupported)
        }
        fn create_directory_at(
            &self,
            _parent: &dyn DirectoryHandle,
            _name: &[u8],
        ) -> Result<Box<dyn DirectoryHandle>> {
            Err(DriverError::Unsupported)
        }
        fn create_temp_at(
            &self,
            _parent: &dyn DirectoryHandle,
        ) -> Result<Box<dyn OwnedTempHandle>> {
            Err(DriverError::Unsupported)
        }
        fn clone_temp_from_regular(
            &self,
            _source: &dyn RegularFileHandle,
        ) -> Result<Box<dyn OwnedTempHandle>> {
            Err(DriverError::Unsupported)
        }
        fn read_temp_metadata(&self, _temp: &dyn OwnedTempHandle) -> Result<NativeMetadata> {
            Err(DriverError::Unsupported)
        }
        fn set_temp_metadata(
            &self,
            _temp: &mut dyn OwnedTempHandle,
            _metadata: &NativeMetadata,
        ) -> Result<()> {
            Err(DriverError::Unsupported)
        }
        fn set_entry_metadata(
            &self,
            _parent: &dyn DirectoryHandle,
            _name: &[u8],
            _expected: &[u8],
            _metadata: &NativeMetadata,
        ) -> Result<()> {
            Err(DriverError::Unsupported)
        }
        fn atomic_replace(
            &self,
            _temp: Box<dyn OwnedTempHandle>,
            _parent: &dyn DirectoryHandle,
            _name: &[u8],
        ) -> Result<()> {
            Err(DriverError::Unsupported)
        }
        fn atomic_replace_checked(
            &self,
            _temp: Box<dyn OwnedTempHandle>,
            _parent: &dyn DirectoryHandle,
            _name: &[u8],
            _expected: Option<&[u8]>,
        ) -> Result<()> {
            Err(DriverError::Unsupported)
        }
        fn create_symlink_at(
            &self,
            _parent: &dyn DirectoryHandle,
            _name: &[u8],
            _target: &[u8],
            _metadata: &NativeMetadata,
        ) -> Result<()> {
            Err(DriverError::Unsupported)
        }
        fn atomic_replace_symlink(
            &self,
            _parent: &dyn DirectoryHandle,
            _name: &[u8],
            _expected: Option<&[u8]>,
            _target: &[u8],
            _metadata: &NativeMetadata,
        ) -> Result<()> {
            Err(DriverError::Unsupported)
        }
        fn create_hard_link_at(
            &self,
            _source_parent: &dyn DirectoryHandle,
            _source: &[u8],
            _source_expected: &[u8],
            _target_parent: &dyn DirectoryHandle,
            _target: &[u8],
        ) -> Result<()> {
            Err(DriverError::Unsupported)
        }
        fn finish_hard_link_at(
            &self,
            _source_parent: &dyn DirectoryHandle,
            _source: &[u8],
            _source_expected: &[u8],
            _metadata: &NativeMetadata,
        ) -> Result<()> {
            Err(DriverError::Unsupported)
        }
        fn rename_at(
            &self,
            _source_parent: &dyn DirectoryHandle,
            _source: &[u8],
            _target_parent: &dyn DirectoryHandle,
            _target: &[u8],
        ) -> Result<()> {
            Err(DriverError::Unsupported)
        }
        fn unlink_regular_at(
            &self,
            _parent: &dyn DirectoryHandle,
            _name: &[u8],
            _expected: &[u8],
        ) -> Result<()> {
            Err(DriverError::Unsupported)
        }
        fn unlink_symlink_at(
            &self,
            _parent: &dyn DirectoryHandle,
            _name: &[u8],
            _expected: &[u8],
        ) -> Result<()> {
            Err(DriverError::Unsupported)
        }
        fn remove_directory_at(
            &self,
            _parent: &dyn DirectoryHandle,
            _name: &[u8],
            _expected: &[u8],
        ) -> Result<()> {
            Err(DriverError::Unsupported)
        }
        fn sync_directory(&self, _directory: &dyn DirectoryHandle) -> Result<()> {
            Ok(())
        }
        fn set_root_metadata(&self, _metadata: &NativeMetadata) -> Result<()> {
            Err(DriverError::Unsupported)
        }
        fn remove_owned_root(&self, _expected_identity: &[u8]) -> Result<()> {
            Err(DriverError::Unsupported)
        }
    }

    struct MemoryDriver;

    impl ProjectionDriver for MemoryDriver {
        fn open_workspace(
            &self,
            _path: &Path,
            _policy: WorkspacePolicy,
            _store_id: [u8; 32],
        ) -> Result<Box<dyn ProjectionWorkspace>> {
            Ok(Box::new(MemoryWorkspace))
        }
    }

    #[test]
    fn erased_driver_and_handles_are_object_safe() {
        let driver: Box<dyn ProjectionDriver> = Box::new(MemoryDriver);
        let workspace = driver
            .open_workspace(
                Path::new("unused"),
                WorkspacePolicy::ManagedPrivate,
                [0; 32],
            )
            .unwrap();
        let root = workspace.root_directory().unwrap();
        let entries = workspace
            .enumerate_at(root.as_ref())
            .unwrap()
            .collect::<Result<Vec<_>>>()
            .unwrap();
        assert_eq!(entries[0].name, b"file");
    }
}
