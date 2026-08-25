//! Object-safe native workspace boundary.

use std::any::Any;
use std::fmt;
use std::io::{self, Read, Seek, Write};
use std::path::Path;

pub const MAX_NATIVE_XATTR_BYTES: usize = 1024 * 1024;
const NATIVE_XATTR_CHUNK_BYTES: usize = 1024 * 1024;

pub trait DirectoryHandle: Send {
    fn as_any(&self) -> &dyn Any;
}
pub trait RegularFileHandle: Read + Write + Seek + Send {
    fn as_any(&self) -> &dyn Any;
}
pub trait OwnedTempHandle: Read + Write + Seek + Send {
    fn as_any(&self) -> &dyn Any;
    fn set_len(&mut self, len: u64) -> Result<()>;
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
    pub xattrs: NativeXattrs,
    pub acl: Option<Vec<u8>>,
    pub bsd_flags: u32,
}

/// Compact native xattrs with no per-entry heap allocation. The accepted
/// name+value population remains one MiB; framing is split into <=1 MiB
/// chunks and is not canonical LayerFS storage.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NativeXattrs {
    chunks: Vec<Vec<u8>>,
    count: usize,
    payload_bytes: usize,
    last_name: Option<Vec<u8>>,
}

impl NativeXattrs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn payload_bytes(&self) -> usize {
        self.payload_bytes
    }

    pub fn from_entries(entries: impl IntoIterator<Item = (Vec<u8>, Vec<u8>)>) -> Result<Self> {
        let mut xattrs = Self::new();
        for (name, value) in entries {
            xattrs.push(&name, &value)?;
        }
        Ok(xattrs)
    }

    pub fn push(&mut self, name: &[u8], value: &[u8]) -> Result<()> {
        if name.is_empty()
            || name.len() > 127
            || name.contains(&0)
            || value.len() > MAX_NATIVE_XATTR_BYTES
            || self
                .last_name
                .as_deref()
                .is_some_and(|previous| previous >= name)
        {
            return Err(DriverError::Unsupported);
        }
        self.payload_bytes = self
            .payload_bytes
            .checked_add(name.len())
            .and_then(|total| total.checked_add(value.len()))
            .filter(|total| *total <= MAX_NATIVE_XATTR_BYTES)
            .ok_or(DriverError::Unsupported)?;
        append_varint(&mut self.chunks, name.len() as u32);
        append_varint(
            &mut self.chunks,
            u32::try_from(value.len()).map_err(|_| DriverError::Unsupported)?,
        );
        append_chunked(&mut self.chunks, name);
        append_chunked(&mut self.chunks, value);
        self.last_name = Some(name.to_vec());
        self.count = self.count.checked_add(1).ok_or(DriverError::Unsupported)?;
        Ok(())
    }

    pub fn iter(&self) -> NativeXattrIter<'_> {
        NativeXattrIter {
            xattrs: self,
            chunk: 0,
            offset: 0,
            remaining: self.count,
        }
    }

    pub fn names(&self) -> NativeXattrNameIter<'_> {
        NativeXattrNameIter { inner: self.iter() }
    }
}

impl<'a> IntoIterator for &'a NativeXattrs {
    type Item = (Vec<u8>, Vec<u8>);
    type IntoIter = NativeXattrIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub struct NativeXattrIter<'a> {
    xattrs: &'a NativeXattrs,
    chunk: usize,
    offset: usize,
    remaining: usize,
}

pub struct NativeXattrNameIter<'a> {
    inner: NativeXattrIter<'a>,
}

impl Iterator for NativeXattrNameIter<'_> {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.inner.remaining == 0 {
            return None;
        }
        let name_len = self.inner.read_varint()? as usize;
        let value_len = self.inner.read_varint()? as usize;
        let name = self.inner.read_vec(name_len)?;
        self.inner.skip_bytes(value_len)?;
        self.inner.remaining -= 1;
        Some(name)
    }
}

impl Iterator for NativeXattrIter<'_> {
    type Item = (Vec<u8>, Vec<u8>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let name_len = self.read_varint()? as usize;
        let value_len = self.read_varint()? as usize;
        let name = self.read_vec(name_len)?;
        let value = self.read_vec(value_len)?;
        self.remaining -= 1;
        Some((name, value))
    }
}

impl NativeXattrIter<'_> {
    fn read_byte(&mut self) -> Option<u8> {
        while self
            .xattrs
            .chunks
            .get(self.chunk)
            .is_some_and(|chunk| self.offset == chunk.len())
        {
            self.chunk += 1;
            self.offset = 0;
        }
        let byte = *self.xattrs.chunks.get(self.chunk)?.get(self.offset)?;
        self.offset += 1;
        Some(byte)
    }

    fn read_varint(&mut self) -> Option<u32> {
        let mut value = 0_u32;
        for shift in (0..=28).step_by(7) {
            let byte = self.read_byte()?;
            value |= u32::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }

    fn read_vec(&mut self, len: usize) -> Option<Vec<u8>> {
        let mut value = Vec::with_capacity(len);
        for _ in 0..len {
            value.push(self.read_byte()?);
        }
        Some(value)
    }

    fn skip_bytes(&mut self, len: usize) -> Option<()> {
        for _ in 0..len {
            self.read_byte()?;
        }
        Some(())
    }
}

fn append_varint(chunks: &mut Vec<Vec<u8>>, mut value: u32) {
    let mut bytes = [0_u8; 5];
    let mut len = 0;
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        bytes[len] = byte;
        len += 1;
        if value == 0 {
            append_chunked(chunks, &bytes[..len]);
            return;
        }
    }
}

fn append_chunked(chunks: &mut Vec<Vec<u8>>, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        if chunks
            .last()
            .is_none_or(|chunk| chunk.len() == NATIVE_XATTR_CHUNK_BYTES)
        {
            chunks.push(Vec::with_capacity(
                NATIVE_XATTR_CHUNK_BYTES.min(bytes.len()),
            ));
        }
        let chunk = chunks.last_mut().unwrap();
        let take = bytes
            .len()
            .min(NATIVE_XATTR_CHUNK_BYTES.saturating_sub(chunk.len()));
        chunk.extend_from_slice(&bytes[..take]);
        bytes = &bytes[take..];
    }
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

    #[test]
    fn native_xattrs_are_compact_ordered_and_round_trip_the_full_envelope() {
        let entries = (0..1024)
            .map(|index| (format!("x{index:015}").into_bytes(), vec![9; 1008]))
            .collect::<Vec<_>>();
        let xattrs = NativeXattrs::from_entries(entries.clone()).unwrap();
        assert_eq!(xattrs.payload_bytes(), MAX_NATIVE_XATTR_BYTES);
        assert_eq!(xattrs.iter().collect::<Vec<_>>(), entries);
        assert!(xattrs
            .chunks
            .iter()
            .all(|chunk| chunk.len() <= NATIVE_XATTR_CHUNK_BYTES));

        let mut unordered = NativeXattrs::new();
        unordered.push(b"b", b"1").unwrap();
        assert!(matches!(
            unordered.push(b"a", b"2"),
            Err(DriverError::Unsupported)
        ));
    }
}
