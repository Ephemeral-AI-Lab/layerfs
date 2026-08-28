//! Object-safe projection workspace contract.

use super::{
    DirectoryDurability, DirectoryHandle, NamePreflight, NativeEntry, NativeMetadata,
    OwnedTempHandle, ProjectionFacts, RegularFileHandle, Result,
};

pub trait ProjectionWorkspace: Send {
    fn projection_facts(&self) -> ProjectionFacts {
        ProjectionFacts::default()
    }
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
    fn atomic_replace_with_directory_durability(
        &self,
        temp: Box<dyn OwnedTempHandle>,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        _requested: DirectoryDurability,
    ) -> Result<DirectoryDurability> {
        self.atomic_replace(temp, parent, name)?;
        Ok(DirectoryDurability::ImmediateDirectoryDurability)
    }
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
    /// Discards the root created by `ManagedCreateOwned` when admission fails
    /// before the portable layer can obtain a root handle or stable identity.
    /// The workspace retains the creation-time identity needed to remove only
    /// that exact owned root.
    fn discard_owned_root(self: Box<Self>) -> Result<()>;
    fn remove_owned_root(&self, expected_identity: &[u8]) -> Result<()>;
}
