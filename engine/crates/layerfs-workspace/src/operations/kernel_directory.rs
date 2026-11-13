//! READDIR as owner visits that record no request source. A reading visit is
//! one read-only job: the open descriptor's fence, the position of the
//! kernel offset, and one window of current local names with their kinds.
//! Inherited names and kinds are merged inside the job when the canonical
//! cache holds them; otherwise the request merges them outside the owner
//! over one Store reader, at the base root the visit named. A reply that
//! shows each entry's facts (READDIRPLUS) takes them from the same memory
//! first, and reads what is left in one batch inode demand and one batch
//! length demand for the whole window, whatever its size. What those batches
//! do not cover is per entry and unchanged here: a non-inline metadata root,
//! a listed subdirectory's child-directory count, and a symlink's target are
//! still read once for each entry that needs them. The request then fills its
//! reply and a publishing visit records exactly the accepted names. Nothing
//! is held between the two visits or after the reply.
use crate::{BaseView, SourceView, ViewStat, Workspace, WorkspaceResult};
use layerfs_content::filesystem::inode::{lookup_many, InodeReadWork};
use layerfs_content::filesystem::Resolved;
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{ContentError, ObjectId};
use layerfs_overlay::{
    KernelCookieOffer, KernelDirectoryCursor, KernelDirectoryPage, KernelMount, Overlay,
    OverlayError, OverlayResult, PAGE_ROWS,
};
use std::{collections::BTreeMap, collections::BTreeSet, fmt, sync::Arc};

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
/// Where one enumeration request reads and what its reply shows.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct KernelDirectoryRead {
    /// The kernel offset this request resumes at.
    pub offset: u64,
    /// The name it continues after, when it continues an earlier window.
    pub after: Option<Vec<u8>>,
    /// The reply shows each entry's facts (READDIRPLUS).
    pub attributes: bool,
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
    /// The reply shows each entry's facts (READDIRPLUS): this visit answers
    /// from memory what it holds, and the request reads only what it left.
    attributes: bool,
}
/// What the reading visit found, as plain values its request owns.
#[derive(Clone, Debug)]
pub struct KernelDirectoryWindow {
    pub page: KernelDirectoryPage,
    /// The merged names of the window, when memory answered every inherited
    /// name and kind inside the job; a reply that shows facts also takes
    /// from here every fact memory held.
    pub listing: Option<KernelDirectoryListing>,
    /// The offsets the reply may hand out. Absent when the job decided that
    /// the window lists no name.
    pub offer: Option<KernelCookieOffer>,
    /// The facts of the directory being listed, when the visit read its own
    /// local row: a reply's `dir_attributes` then costs no further read.
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
    /// `resident` answers from memory only. No I/O here. `read` names the
    /// position (continuing past names that listed nothing) and whether the
    /// reply shows each entry's facts.
    pub fn kernel_directory_visit(
        &self,
        resident: Arc<crate::CanonicalClient>,
        mount: KernelMount,
        serial: u64,
        handle: u64,
        read: KernelDirectoryRead,
    ) -> WorkspaceResult<KernelDirectoryVisit> {
        let KernelDirectoryRead {
            offset,
            after,
            attributes,
        } = read;
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
            attributes,
        })
    }
}
impl KernelDirectoryVisit {
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
            .then(|| listing(&self.resident, &page).ok())
            .flatten()
            .map(|mut listing| {
                // A reply that shows facts takes the local rows this window
                // already read and whatever else memory answers, so only the
                // names memory left undecided take the one Store read.
                if self.attributes {
                    for entry in &mut listing.entries {
                        entry.stat = memory_stat(&self.resident, &page, entry.serial);
                    }
                }
                listing
            });
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
    /// Whether this reply needs the Store reader at all: the visit left the
    /// merge undone, or the reply shows facts of a name memory did not hold.
    /// A local directory, and a window whose inherited entries are all
    /// resident, take no reader and issue no Store read.
    pub fn needs_base(&self, attributes: bool) -> bool {
        match &self.listing {
            None => true,
            Some(listing) => attributes && listing.entries.iter().any(|entry| entry.stat.is_none()),
        }
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
    /// portable metadata and no file length for one: only the names, their
    /// kinds and their offsets.
    pub fn finish(self, base: Option<&BaseView>) -> WorkspaceResult<KernelDirectoryBatch> {
        self.finish_with(base, false)
    }
    /// The same for a reply that also shows what each accepted entry is, as
    /// READDIRPLUS does. The reading visit already said what memory held; the
    /// names it left undecided are read here in one batch inode demand and
    /// one batch length demand for the whole window, so the number of names
    /// adds no demand. Those two batches are not the whole cost of an entry:
    /// [`base_facts`] still reads an inherited inode's non-inline metadata
    /// root, a listed subdirectory's child-directory count and a symlink's
    /// target once per entry that needs them, as the base route always did.
    pub fn finish_with(
        self,
        base: Option<&BaseView>,
        attributes: bool,
    ) -> WorkspaceResult<KernelDirectoryBatch> {
        let Self {
            page,
            listing: merged,
            offer,
            directory,
        } = self;
        // The authoritative base is read at the root the visit named, and
        // only when memory did not answer the window's names or its facts.
        let undecided = attributes
            && merged
                .as_ref()
                .is_none_or(|listing| listing.entries.iter().any(|entry| entry.stat.is_none()));
        let base = match base {
            Some(base) if merged.is_none() || undecided => Some(base.at(page.source().root())?),
            _ => None,
        };
        let listing = match (merged, base.as_deref()) {
            (Some(listing), _) => listing,
            (_, Some(base)) if attributes => listing_with_facts(base, &page)?,
            (_, Some(base)) => listing(base, &page)?,
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
        if attributes {
            if let Some(base) = base.as_deref() {
                facts(base, &page, &mut entries)?;
            }
        }
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
/// Fills the facts a reply shows for the entries that still lack them: the
/// window's inherited serials in one batch inode demand, and the lengths of
/// its regular files in one batch length demand, so listing more names costs
/// no more of either. A local serial's facts are its current row. What
/// [`base_facts`] still reads once per entry is not batched here.
fn facts(
    base: &BaseView,
    page: &KernelDirectoryPage,
    entries: &mut [KernelDirectoryEntry],
) -> WorkspaceResult<()> {
    let inherited: Vec<u64> = entries
        .iter()
        .filter(|entry| entry.stat.is_none() && local_row(page, entry.serial).is_none())
        .map(|entry| entry.serial)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let found = inherited_values(base, &inherited)?;
    for entry in entries.iter_mut().filter(|entry| entry.stat.is_none()) {
        entry.stat = Some(match local_row(page, entry.serial) {
            Some(row) => ViewStat::from(row.clone()),
            None => {
                let value = *found.get(&entry.serial).ok_or(ContentError::PathNotFound)?;
                base_facts(base, entry.serial, value)?
            }
        });
    }
    Ok(())
}
/// The base value of every inherited serial in one batch inode demand, after
/// one length demand has remembered the window's ordinary file lengths.
fn inherited_values(
    base: &BaseView,
    serials: &[u64],
) -> WorkspaceResult<BTreeMap<u64, InodeValue>> {
    if serials.is_empty() {
        return Ok(BTreeMap::new());
    }
    let values = lookup_many(
        base.client().as_ref(),
        base.table(),
        serials,
        &mut InodeReadWork::default(),
    )?;
    // Every ordinary file's length, from the one demand this window makes.
    let roots: Vec<ObjectId> = values
        .iter()
        .flatten()
        .filter(|value| value.kind == InodeKind::RegularFile)
        .map(|value| value.content_root)
        .collect();
    if !roots.is_empty() {
        base.client().provide_lengths(&roots)?;
    }
    Ok(serials
        .iter()
        .copied()
        .zip(values)
        .filter_map(|(serial, value)| Some((serial, value?)))
        .collect())
}
/// The facts of one inherited name: its base inode, the metadata every
/// attribute carries, its content length and, for a directory, the count its
/// POSIX link count is derived from. The metadata root, a directory's child
/// directory count and a symlink's target are read here one entry at a time:
/// the base has no batch port for them, and this route adds none.
fn base_facts(base: &BaseView, serial: u64, value: InodeValue) -> WorkspaceResult<ViewStat> {
    let stat =
        base.stat_resolved_with_lengths(Resolved { serial, value }, base.client().as_ref())?;
    let subdirs = match stat.value.kind {
        InodeKind::Directory => base.subdirs(stat.value)?,
        _ => 0,
    };
    Ok(ViewStat::of_base(stat, subdirs))
}
/// The local row of one listed serial at this window's view, when it has one.
fn local_row(page: &KernelDirectoryPage, serial: u64) -> Option<&layerfs_overlay::Inode> {
    page.local_inodes
        .binary_search_by_key(&serial, |row| row.0)
        .ok()
        .map(|index| &page.local_inodes[index].1)
}
/// The content kind of a local row's kind.
const fn kind_of(kind: layerfs_overlay::InodeKind) -> InodeKind {
    match kind {
        layerfs_overlay::InodeKind::File => InodeKind::RegularFile,
        layerfs_overlay::InodeKind::Directory => InodeKind::Directory,
        layerfs_overlay::InodeKind::Symlink => InodeKind::Symlink,
    }
}
/// The facts of one entry as far as memory alone answers them: a local row,
/// or an inherited inode whose value, metadata, length and child-directory
/// count this daemon's caches already hold. Anything else is absent here, and
/// the request's one Store read supplies it.
fn memory_stat(base: &BaseView, page: &KernelDirectoryPage, serial: u64) -> Option<ViewStat> {
    if let Some(row) = local_row(page, serial) {
        return Some(ViewStat::from(row.clone()));
    }
    let value = base.inode(serial).ok()?.value;
    base_facts(base, serial, value).ok()
}
/// One window's listed names over `base`, before their kinds and facts.
struct MergedNames {
    names: Vec<(Vec<u8>, u64)>,
    continuation: Option<Vec<u8>>,
    visited: usize,
}
/// One window's merged names over `base`, which must be the page's own base
/// root. This issues no SQL and acquires no kernel lookup reference. An
/// empty result with a continuation needs another window of the same request.
fn merge(base: &BaseView, page: &KernelDirectoryPage) -> WorkspaceResult<MergedNames> {
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
            return Ok(MergedNames {
                names: Vec::new(),
                continuation: None,
                visited: 0,
            });
        }
    }
    let listing =
        view.list_from_window(page.local.parent, page.after.as_deref(), page.local.clone())?;
    Ok(MergedNames {
        names: listing.entries,
        continuation: listing.continuation,
        visited: listing.visited,
    })
}
/// The same window with the facts a reply shows for every entry: a local
/// row, or an inherited inode read in one batch demand for the whole window.
fn listing_with_facts(
    base: &BaseView,
    page: &KernelDirectoryPage,
) -> WorkspaceResult<KernelDirectoryListing> {
    let merged = merge(base, page)?;
    let inherited: Vec<u64> = merged
        .names
        .iter()
        .map(|(_, serial)| *serial)
        .filter(|serial| local_row(page, *serial).is_none())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let found = inherited_values(base, &inherited)?;
    let mut entries = Vec::with_capacity(merged.names.len());
    for (name, serial) in merged.names {
        let (kind, stat) = match local_row(page, serial) {
            Some(row) => (kind_of(row.kind), ViewStat::from(row.clone())),
            None => {
                let value = *found.get(&serial).ok_or(ContentError::PathNotFound)?;
                (value.kind, base_facts(base, serial, value)?)
            }
        };
        entries.push(KernelDirectoryEntry {
            name,
            serial,
            kind,
            stat: Some(stat),
        });
    }
    Ok(KernelDirectoryListing {
        entries,
        continuation: merged.continuation,
        visited: merged.visited,
    })
}
/// One window's merged names over `base` with their kinds, read from memory
/// alone: a local row, or an inherited inode the canonical cache answers.
fn listing(base: &BaseView, page: &KernelDirectoryPage) -> WorkspaceResult<KernelDirectoryListing> {
    let merged = merge(base, page)?;
    let mut entries = Vec::with_capacity(merged.names.len());
    for (name, serial) in merged.names {
        let kind = match local_row(page, serial) {
            Some(row) => kind_of(row.kind),
            None => base.inode(serial)?.value.kind,
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
        continuation: merged.continuation,
        visited: merged.visited,
    })
}
