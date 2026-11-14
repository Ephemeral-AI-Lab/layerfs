//! READDIR as owner visits that record no request source. A reading visit is
//! one read-only job: the open descriptor's fence, the position of the
//! kernel offset, and one window of current local names with their kinds.
//! Inherited names and kinds are merged inside the job when the canonical
//! cache holds them; otherwise the request merges them outside the owner
//! over one Store reader, at the base root the visit named. The request then
//! fills its reply and a publishing visit records exactly the accepted
//! names. Nothing is held between the two visits or after the reply.
use crate::{BaseView, SourceView, ViewStat, Workspace, WorkspaceResult};
use layerfs_content::{object::inode_leaf::InodeKind, ContentError};
use layerfs_overlay::{
    KernelCookieOffer, KernelDirectoryCursor, KernelDirectoryPage, KernelMount, Overlay,
    OverlayError, OverlayResult, PAGE_ROWS,
};
use std::{fmt, sync::Arc};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KernelDirectoryEntry {
    pub name: Vec<u8>,
    pub serial: u64,
    pub kind: InodeKind,
    /// What a reply that shows this entry's facts says about it: a local
    /// entry's current row, an inherited one's immutable base inode. None
    /// until a reply that carries facts reads them outside the owner.
    pub stat: Option<ViewStat>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KernelDirectoryListing {
    pub entries: Vec<KernelDirectoryEntry>,
    pub continuation: Option<Vec<u8>>,
    pub visited: usize,
}
/// One READDIR reading visit as one read-only owner job.
#[derive(Clone)]
pub struct KernelDirectoryVisit {
    mount: KernelMount,
    serial: u64,
    handle: u64,
    offset: u64,
    after: Option<Vec<u8>>,
    resident: BaseView,
    /// Prototype (READDIRPLUS experiment): keys the window visits.
    rows: usize,
}
/// What the reading visit found, as plain values its request owns.
#[derive(Clone, Debug)]
pub struct KernelDirectoryWindow {
    pub page: KernelDirectoryPage,
    /// The merged names of the window, when memory answered every inherited
    /// name and kind inside the job.
    pub listing: Option<KernelDirectoryListing>,
    /// The offsets the reply may hand out. Absent when the job decided that
    /// the window lists no name.
    pub offer: Option<KernelCookieOffer>,
    /// The facts of the directory being listed, when the visit read its own
    /// local row and the reply shows them: a READDIRPLUS reply's
    /// `dir_attributes` then costs no further read.
    pub directory: Option<ViewStat>,
}
/// The entries one reply may offer, in order, with their offsets.
#[derive(Clone, Debug)]
pub struct KernelDirectoryBatch {
    pub entries: Vec<KernelDirectoryEntry>,
    /// Where the request's next window continues when this one lists none.
    pub continuation: Option<Vec<u8>>,
    /// The offset of the first entry; each next entry's is one more.
    pub first: u64,
    /// The facts of the directory being listed, for a reply's own
    /// `post_op_attr`; absent when this window read none.
    pub directory: Option<ViewStat>,
    /// True when this window listed the directory's last name: accepting
    /// every entry of the window reaches its end, and no later reply follows.
    pub exhausted: bool,
    /// The offer whose fresh range the offsets are, to publish the accepted
    /// names with. None: the offsets are those of an already published
    /// reply with these names, and nothing is published. An offer that
    /// rewinds is published even when the reply accepts none of the names.
    pub publish: Option<KernelCookieOffer>,
}
impl fmt::Debug for KernelDirectoryVisit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KernelDirectoryVisit")
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
    pub fn kernel_directory_visit(
        &self,
        resident: Arc<crate::CanonicalClient>,
        mount: KernelMount,
        serial: u64,
        handle: u64,
        offset: u64,
        after: Option<Vec<u8>>,
    ) -> WorkspaceResult<KernelDirectoryVisit> {
        let base = self.base()?;
        if mount.engine() != self.route().engine()
            || mount.root_serial() != base.root().root_inode().serial()
        {
            return Err(OverlayError::Stale.into());
        }
        Ok(KernelDirectoryVisit {
            mount,
            serial,
            handle,
            offset,
            after,
            resident: base.with_client(resident),
            rows: PAGE_ROWS,
        })
    }
}
impl KernelDirectoryVisit {
    /// Prototype (READDIRPLUS experiment): a window of at most `rows` keys,
    /// for a reply that can take no more than that.
    pub fn with_rows(mut self, rows: usize) -> Self {
        self.rows = rows.clamp(1, PAGE_ROWS);
        self
    }
    pub const fn mount(&self) -> KernelMount {
        self.mount
    }

    /// The visit always answers: what memory does not hold of the base is
    /// left to the one read that follows outside the owner. The reuse lookup
    /// is skipped only when the job itself found that no name is listed.
    /// An offset from before the handle's last rewind lists nothing.
    pub fn perform(&self, db: &Overlay) -> OverlayResult<KernelDirectoryWindow> {
        let page = db.read_kernel_directory_visit(
            self.mount,
            self.serial,
            self.handle,
            self.offset,
            self.after.as_deref(),
        )?;
        if *page.cursor() == KernelDirectoryCursor::Rewound {
            return Ok(KernelDirectoryWindow {
                page,
                listing: Some(KernelDirectoryListing {
                    entries: Vec::new(),
                    continuation: None,
                    visited: 0,
                }),
                offer: None,
                directory: None,
            });
        }
        let listing = (self.resident.identity().0.to_bytes() == page.source().root())
            .then(|| listing(&self.resident, &page, self.rows).ok())
            .flatten();
        let offer = match &listing {
            Some(listing) if listing.entries.is_empty() => None,
            _ => Some(db.offer_kernel_cookies(&page)?),
        };
        let directory = page
            .local
            .parent_inode
            .as_ref()
            .map(|inode| ViewStat::from(inode.clone()));
        Ok(KernelDirectoryWindow {
            page,
            listing,
            offer,
            directory,
        })
    }
}
impl KernelDirectoryWindow {
    /// The request's offset was handed out before the handle was last
    /// listed from offset 0: it is refused, and the window lists nothing.
    pub fn rewound(&self) -> bool {
        *self.page.cursor() == KernelDirectoryCursor::Rewound
    }
    /// The entries this window may offer and their offsets. `base` is the
    /// Workspace's base over an admitted Store reader and is required when
    /// the visit left the merge undone; it is read at the root the visit
    /// named, never at a later one.
    ///
    /// A published reply of this open directory that was listed after the
    /// same name and whose names these entries begin with is reused: the
    /// batch is exactly its names at its offsets. Otherwise the offsets are
    /// the offer's fresh range.
    ///
    /// A plain READDIR reply shows no entry's facts, so this reads no
    /// portable metadata and no file length.
    pub fn finish(self, base: Option<&BaseView>) -> WorkspaceResult<KernelDirectoryBatch> {
        self.finish_with(base, false)
    }
    /// The same for a reply that also shows what each accepted entry is, as
    /// READDIRPLUS does: every entry's facts and the directory's own are read
    /// here, outside the owner, from the authoritative base — a local entry's
    /// facts are its current row, an inherited one's its immutable inode.
    pub fn finish_with(
        self,
        base: Option<&BaseView>,
        attributes: bool,
    ) -> WorkspaceResult<KernelDirectoryBatch> {
        self.finish_limited(base, attributes, None)
    }
    /// Prototype (READDIRPLUS experiment): as [`Self::finish_with`], but
    /// with `facts` set only the first `facts` entries carry facts, and the
    /// names the reading visit merged from memory are kept instead of being
    /// listed again from the authoritative base (the same root).
    pub fn finish_limited(
        self,
        base: Option<&BaseView>,
        attributes: bool,
        facts: Option<usize>,
    ) -> WorkspaceResult<KernelDirectoryBatch> {
        let Self {
            page,
            listing: merged,
            offer,
            directory,
        } = self;
        // A reply that shows an entry's facts reads the authoritative base,
        // not the resident one the reading visit merged with.
        let base = match base {
            Some(base) if attributes || merged.is_none() => Some(base.at(page.source().root())?),
            _ => None,
        };
        let listing = match (merged, base.as_deref()) {
            (Some(listing), _) if !attributes || facts.is_some() => listing,
            (_, Some(base)) => listing(base, &page, facts.unwrap_or(PAGE_ROWS))?,
            (_, None) => {
                return Err(
                    ContentError::InvalidRecord("kernel directory window without its base").into(),
                )
            }
        };
        let mut entries = listing.entries;
        let mut truncated = false;
        let (first, publish) = match offer {
            None if entries.is_empty() => (0, None),
            None => {
                return Err(
                    ContentError::InvalidRecord("kernel directory names without offsets").into(),
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
                    truncated = names.len() < entries.len();
                    entries.truncate(names.len());
                    (first, None)
                }
                _ => (offer.first(), Some(offer)),
            },
        };
        // Only the names this reply actually offers are read, and only a
        // reply that shows facts reads anything at all.
        let (directory, entries) = match base.as_deref() {
            Some(base) if attributes => (
                directory.or_else(|| base_stat(base, page.directory().serial()).ok()),
                match facts {
                    Some(limit) if limit < entries.len() => {
                        let mut entries = entries;
                        let rest = entries.split_off(limit);
                        let mut entries = with_facts(base, &page, entries)?;
                        entries.extend(rest);
                        entries
                    }
                    _ => with_facts(base, &page, entries)?,
                },
            ),
            _ => (None, entries),
        };
        let exhausted = !truncated && listing.continuation.is_none();
        Ok(KernelDirectoryBatch {
            entries,
            continuation: listing.continuation,
            first,
            directory,
            exhausted,
            publish,
        })
    }
}
/// One listing's entries with the facts a reply shows for each: a local
/// serial's current row, or an inherited one's immutable base inode.
fn with_facts(
    base: &BaseView,
    page: &KernelDirectoryPage,
    entries: Vec<KernelDirectoryEntry>,
) -> WorkspaceResult<Vec<KernelDirectoryEntry>> {
    entries
        .into_iter()
        .map(|mut entry| {
            entry.stat = Some(
                match page
                    .local_inodes
                    .binary_search_by_key(&entry.serial, |row| row.0)
                {
                    Ok(index) => ViewStat::from(page.local_inodes[index].1.clone()),
                    Err(_) => base_stat(base, entry.serial)?,
                },
            );
            Ok(entry)
        })
        .collect()
}
/// One window's merged names over `base`, which must be the page's own base
/// root. This issues no SQL and acquires no kernel lookup reference. An
/// empty result with a continuation needs another window of the same request.
fn listing(
    base: &BaseView,
    page: &KernelDirectoryPage,
    rows: usize,
) -> WorkspaceResult<KernelDirectoryListing> {
    let view = SourceView {
        base: base.clone(),
        source: page.source(),
    };
    if page.local.parent != page.directory().serial()
        || page.local_inodes.len() > 2 * PAGE_ROWS
        || page
            .local_inodes
            .windows(2)
            .any(|pair| pair[0].0 >= pair[1].0)
    {
        return Err(ContentError::InvalidRecord("kernel directory page identity").into());
    }
    if let Some(inode) = &page.local.parent_inode {
        if inode.nlink == 0 && inode.serial != view.root_serial() {
            // Rmdir/replacement removes only an empty directory. An open
            // descriptor retains that proved-empty view; ordinary unowned
            // namespace listing still refuses a removed serial.
            if inode.kind != layerfs_overlay::InodeKind::Directory || inode.entries != 0 {
                return Err(ContentError::InvalidRecord("removed kernel directory").into());
            }
            return Ok(KernelDirectoryListing {
                entries: Vec::new(),
                continuation: None,
                visited: 0,
            });
        }
    }
    let listing = view.list_from_window_rows(
        page.local.parent,
        page.after.as_deref(),
        page.local.clone(),
        rows,
    )?;
    let mut entries = Vec::with_capacity(listing.entries.len());
    for (name, serial) in listing.entries {
        let kind = match page
            .local_inodes
            .binary_search_by_key(&serial, |entry| entry.0)
        {
            Ok(index) => match page.local_inodes[index].1.kind {
                layerfs_overlay::InodeKind::File => InodeKind::RegularFile,
                layerfs_overlay::InodeKind::Directory => InodeKind::Directory,
                layerfs_overlay::InodeKind::Symlink => InodeKind::Symlink,
            },
            Err(_) => view.base().inode(serial)?.value.kind,
        };
        entries.push(KernelDirectoryEntry {
            name,
            serial,
            kind,
            stat: None,
        });
    }
    Ok(KernelDirectoryListing {
        entries,
        continuation: listing.continuation,
        visited: listing.visited,
    })
}
/// The effective facts of one inherited name: its base inode, the metadata
/// every attribute carries, its content length and, for a directory, the
/// child-directory count its POSIX link count is derived from.
fn base_stat(base: &BaseView, serial: u64) -> WorkspaceResult<ViewStat> {
    let stat = base.stat(serial)?;
    let subdirs = match stat.value.kind {
        InodeKind::Directory => base.subdirs(stat.value)?,
        _ => 0,
    };
    Ok(ViewStat::of_base(stat, subdirs))
}
