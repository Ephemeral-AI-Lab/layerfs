//! READDIR as owner visits that record no request source. A reading visit is
//! one read-only job: the open descriptor's fence, the position of the
//! kernel offset, and one window of current local names with their kinds.
//! Inherited names and kinds are merged inside the job when the canonical
//! cache holds them; otherwise the request merges them outside the owner
//! over one Store reader, at the base root the visit named. The request then
//! fills its reply and a publishing visit records exactly the accepted
//! names. Nothing is held between the two visits or after the reply.
use crate::{BaseView, SourceView, Workspace, WorkspaceResult};
use layerfs_content::{object::inode_leaf::InodeKind, ContentError};
use layerfs_overlay::{
    NativeCookieOffer, NativeDirectoryPage, NativeMount, Overlay, OverlayError, OverlayResult,
    PAGE_ROWS,
};
use std::{fmt, sync::Arc};

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
/// One READDIR reading visit as one read-only owner job.
#[derive(Clone)]
pub struct NativeDirectoryVisit {
    mount: NativeMount,
    serial: u64,
    handle: u64,
    offset: u64,
    after: Option<Vec<u8>>,
    resident: BaseView,
}
/// What the reading visit found, as plain values its request owns.
#[derive(Clone, Debug)]
pub struct NativeDirectoryWindow {
    pub page: NativeDirectoryPage,
    /// The merged names of the window, when memory answered every inherited
    /// name and kind inside the job.
    pub listing: Option<NativeDirectoryListing>,
    /// The offsets the reply may hand out. Absent when the job decided that
    /// the window lists no name.
    pub offer: Option<NativeCookieOffer>,
}
/// The entries one reply may offer, in order, with their offsets.
#[derive(Clone, Debug)]
pub struct NativeDirectoryBatch {
    pub entries: Vec<NativeDirectoryEntry>,
    /// Where the request's next window continues when this one lists none.
    pub continuation: Option<Vec<u8>>,
    /// The offset of the first entry; each next entry's is one more.
    pub first: u64,
    /// The offer whose fresh range the offsets are, to publish the accepted
    /// names with. None: the offsets are those of an already published
    /// reply with these names, and nothing is published.
    pub publish: Option<NativeCookieOffer>,
}
impl fmt::Debug for NativeDirectoryVisit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeDirectoryVisit")
            .field("mount", &self.mount)
            .field("serial", &self.serial)
            .field("handle", &self.handle)
            .field("offset", &self.offset)
            .field("after", &self.after)
            .finish_non_exhaustive()
    }
}
impl Workspace {
    /// `resident` answers from memory only. No I/O here. `after` continues
    /// an earlier window of the same request past names that listed nothing.
    pub fn native_directory_visit(
        &self,
        resident: Arc<crate::CanonicalClient>,
        mount: NativeMount,
        serial: u64,
        handle: u64,
        offset: u64,
        after: Option<Vec<u8>>,
    ) -> WorkspaceResult<NativeDirectoryVisit> {
        let base = self.base()?;
        if mount.route() != self.route() || mount.root_serial() != base.root().root_inode().serial()
        {
            return Err(OverlayError::Stale.into());
        }
        Ok(NativeDirectoryVisit {
            mount,
            serial,
            handle,
            offset,
            after,
            resident: base.with_client(resident),
        })
    }
}
impl NativeDirectoryVisit {
    pub const fn mount(&self) -> NativeMount {
        self.mount
    }
    pub fn charge(&self) -> usize {
        self.after.as_ref().map_or(0, Vec::capacity)
    }
    /// The visit always answers: what memory does not hold of the base is
    /// left to the one read that follows outside the owner. The reuse lookup
    /// is skipped only when the job itself found that no name is listed.
    pub fn perform(&self, db: &Overlay) -> OverlayResult<NativeDirectoryWindow> {
        let page = db.read_native_directory_visit(
            self.mount,
            self.serial,
            self.handle,
            self.offset,
            self.after.as_deref(),
        )?;
        let listing = (self.resident.identity().0.to_bytes() == page.source().root())
            .then(|| listing(&self.resident, &page).ok())
            .flatten();
        let offer = match &listing {
            Some(listing) if listing.entries.is_empty() => None,
            _ => Some(db.offer_native_cookies(&page)?),
        };
        Ok(NativeDirectoryWindow {
            page,
            listing,
            offer,
        })
    }
}
impl NativeDirectoryWindow {
    /// The reply's bound of one visit: two local windows, the merged window,
    /// and one published reply's names to compare with.
    pub const CHARGE: usize = 3
        * PAGE_ROWS
        * (std::mem::size_of::<layerfs_overlay::DirectoryEntry>()
            + std::mem::size_of::<NativeDirectoryEntry>()
            + 255)
        + PAGE_ROWS * (std::mem::size_of::<Vec<u8>>() + 255)
        + 3 * 255;
    /// The entries this window may offer and their offsets. `base` is the
    /// Workspace's base over an admitted Store reader and is required when
    /// the visit left the merge undone; it is read at the root the visit
    /// named, never at a later one.
    ///
    /// A published reply of this open directory that was listed after the
    /// same name and whose names these entries begin with is reused: the
    /// batch is exactly its names at its offsets. Otherwise the offsets are
    /// the offer's fresh range.
    pub fn finish(self, base: Option<&BaseView>) -> WorkspaceResult<NativeDirectoryBatch> {
        let listing = match self.listing {
            Some(listing) => listing,
            None => {
                let base = base.ok_or(ContentError::InvalidRecord(
                    "native directory window without its base",
                ))?;
                listing(&*base.at(self.page.source().root())?, &self.page)?
            }
        };
        let mut entries = listing.entries;
        let (first, publish) = match self.offer {
            None if entries.is_empty() => (0, None),
            None => {
                return Err(
                    ContentError::InvalidRecord("native directory names without offsets").into(),
                )
            }
            Some(offer) => match offer.existing() {
                Some((first, names))
                    if names.len() <= entries.len()
                        && names
                            .iter()
                            .zip(&entries)
                            .all(|(name, entry)| *name == entry.name) =>
                {
                    entries.truncate(names.len());
                    (first, None)
                }
                _ => (offer.first(), Some(offer)),
            },
        };
        Ok(NativeDirectoryBatch {
            entries,
            continuation: listing.continuation,
            first,
            publish,
        })
    }
}
/// One window's merged names over `base`, which must be the page's own base
/// root. This issues no SQL and acquires no kernel lookup reference. An
/// empty result with a continuation needs another window of the same request.
fn listing(base: &BaseView, page: &NativeDirectoryPage) -> WorkspaceResult<NativeDirectoryListing> {
    let view = SourceView {
        base: base.clone(),
        source: page.source(),
    };
    if page.local.parent != page.directory().serial()
        || page.local_kinds.len() > 2 * PAGE_ROWS
        || page
            .local_kinds
            .windows(2)
            .any(|pair| pair[0].0 >= pair[1].0)
    {
        return Err(ContentError::InvalidRecord("native directory page identity").into());
    }
    if let Some(inode) = &page.local.parent_inode {
        if inode.nlink == 0 && inode.serial != view.root_serial() {
            // Rmdir/replacement removes only an empty directory. An open
            // descriptor retains that proved-empty view; ordinary unowned
            // namespace listing still refuses a removed serial.
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
        view.list_from_window(page.local.parent, page.after.as_deref(), page.local.clone())?;
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
            Err(_) => view.base().inode(serial)?.value.kind,
        };
        entries.push(NativeDirectoryEntry { name, serial, kind });
    }
    Ok(NativeDirectoryListing {
        entries,
        continuation: listing.continuation,
        visited: listing.visited,
    })
}
