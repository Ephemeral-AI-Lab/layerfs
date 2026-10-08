//! One directory reply window over a completed local page and admitted base I/O.
use crate::{SourceView, WorkspaceResult};
use layerfs_content::{object::inode_leaf::InodeKind, ContentError};
use layerfs_overlay::{NativeDirectoryPage, PAGE_ROWS};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeDirectoryEntry {
    pub name: Vec<u8>,
    pub serial: u64,
    pub kind: InodeKind,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeDirectoryListing {
    pub entries: Vec<NativeDirectoryEntry>,
    pub continuation: Option<Vec<u8>>,
    pub visited: usize,
}
impl SourceView {
    /// The native read source and original owner completion stay held. This
    /// issues no SQL and acquires no kernel lookup references. An empty result
    /// with continuation requires another bounded page for this same request.
    pub fn native_directory_listing(
        &self,
        page: &NativeDirectoryPage,
    ) -> WorkspaceResult<NativeDirectoryListing> {
        if page.read.source() != self.source()
            || page.local.parent != page.read.directory().serial()
            || page.local_kinds.len() > 2 * PAGE_ROWS
            || page
                .local_kinds
                .windows(2)
                .any(|pair| pair[0].0 >= pair[1].0)
        {
            return Err(ContentError::InvalidRecord("native directory page identity").into());
        }
        if let Some(inode) = &page.local.parent_inode {
            if inode.nlink == 0 && inode.serial != self.root_serial() {
                // Rmdir/replacement removes only an empty directory. An actual
                // descriptor/source retains that proved-empty view; ordinary
                // unowned namespace listing still refuses a removed serial.
                if inode.kind != layerfs_overlay::InodeKind::Directory || inode.entries != 0 {
                    return Err(ContentError::InvalidRecord("removed native directory").into());
                }
                return Ok(NativeDirectoryListing {
                    entries: Vec::new(),
                    continuation: None,
                    visited: 0,
                });
            }
        }
        let listing =
            self.list_from_window(page.local.parent, page.after.as_deref(), page.local.clone())?;
        let mut entries = Vec::with_capacity(listing.entries.len());
        for (name, serial) in listing.entries {
            let kind = match page
                .local_kinds
                .binary_search_by_key(&serial, |entry| entry.0)
            {
                Ok(index) => match page.local_kinds[index].1 {
                    layerfs_overlay::InodeKind::File => InodeKind::RegularFile,
                    layerfs_overlay::InodeKind::Directory => InodeKind::Directory,
                    layerfs_overlay::InodeKind::Symlink => InodeKind::Symlink,
                },
                Err(_) => self.base().inode(serial)?.value.kind,
            };
            entries.push(NativeDirectoryEntry { name, serial, kind });
        }
        Ok(NativeDirectoryListing {
            entries,
            continuation: listing.continuation,
            visited: listing.visited,
        })
    }
}
