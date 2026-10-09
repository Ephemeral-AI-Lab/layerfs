//! Child-directory counts of immutable base directories.
//!
//! The canonical format stores no such count: a directory row is a name and a
//! serial, and an inode's reference count is not a POSIX link count. The
//! number of a base directory's bindings that are directories is therefore
//! derived, by listing it in bounded windows and reading the kinds of the
//! listed serials. A serial's kind never changes, so the answer is a pure
//! function of the directory's content root and is remembered under it.
use crate::BaseView;
use layerfs_content::filesystem::directory::{list_after, DirectoryReadWork};
use layerfs_content::filesystem::{DirectoryRoot, PathName};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{ContentError, ContentResult, ObjectId};
use std::collections::BTreeMap;

/// Remembered counts. A fixed number of entries for the whole daemon, shared
/// by every Workspace through the canonical cache owner: it does not grow
/// with directories, files or mutations. One entry is 88 logical bytes: the
/// 32-byte content root, the count and its age in one map, the age and the
/// content root in the other; 1,441,792 bytes at capacity. This is
/// accounting of the stored values, not a measured allocation.
pub const DIRECTORY_COUNT_CAPACITY: usize = 16_384;
/// Bindings listed, and kinds resolved, per window of one derivation.
const WINDOW_ENTRIES: usize = 256;
/// Encoded row bytes per listing window. One row is at most 265 bytes.
const WINDOW_BYTES: usize = 64 * 1024;

const NOT_REMEMBERED: ContentError = ContentError::ProviderFailure {
    what: "base directory count not resident",
};

/// Least-recently-used counts by content root. Eviction loses a remembered
/// answer and nothing else: the next demand derives it again.
#[derive(Default)]
pub(crate) struct DirectoryCounts {
    counts: BTreeMap<ObjectId, (u64, u64)>,
    ages: BTreeMap<(u64, ObjectId), ()>,
    epoch: u64,
}
impl DirectoryCounts {
    pub(crate) fn get(&mut self, root: ObjectId) -> Option<u64> {
        let (count, age) = self.counts.get_mut(&root)?;
        // A saturated epoch only stops reordering; keys stay unique.
        self.epoch = self.epoch.saturating_add(1);
        self.ages.remove(&(*age, root));
        *age = self.epoch;
        self.ages.insert((*age, root), ());
        Some(*count)
    }
    pub(crate) fn insert(&mut self, root: ObjectId, count: u64) {
        if let Some((_, age)) = self.counts.remove(&root) {
            self.ages.remove(&(age, root));
        }
        while self.counts.len() >= DIRECTORY_COUNT_CAPACITY {
            let Some(((_, oldest), ())) = self.ages.pop_first() else {
                break;
            };
            self.counts.remove(&oldest);
        }
        self.epoch = self.epoch.saturating_add(1);
        self.counts.insert(root, (count, self.epoch));
        self.ages.insert((self.epoch, root), ());
    }
    pub(crate) fn len(&self) -> usize {
        self.counts.len()
    }
}
impl BaseView {
    /// The number of this base directory's bindings that are directories.
    /// Reads the directory root page for its entry count; see
    /// [`BaseView::subdirs_among`].
    pub fn subdirs(&self, value: InodeValue) -> ContentResult<u64> {
        self.subdirs_among(value, self.entries(value)?)
    }
    /// The child-directory count of a base directory with `entries` bindings.
    ///
    /// An empty directory answers without a read, a remembered root without a
    /// listing. Otherwise the directory is listed in windows of at most
    /// [`WINDOW_ENTRIES`] rows and [`WINDOW_BYTES`] bytes, each resolved with
    /// one batch inode demand, so nothing held grows with the directory. That
    /// derivation is provider work: a memory-only client never performs it
    /// and reports the count as not resident instead, which leaves an owner
    /// visit undecided exactly like any other base fact it does not hold.
    pub(crate) fn subdirs_among(&self, value: InodeValue, entries: u64) -> ContentResult<u64> {
        if value.kind != InodeKind::Directory {
            return Err(ContentError::WrongLogicalRole);
        }
        if entries == 0 {
            return Ok(0);
        }
        let client = self.client();
        if let Some(count) = client.directory_count(value.content_root)? {
            return Ok(count);
        }
        if client.is_resident() {
            return Err(NOT_REMEMBERED);
        }
        let mut read = self.reader()?;
        let mut work = DirectoryReadWork::default();
        let mut after: Option<PathName> = None;
        let (mut listed, mut count) = (0_u64, 0_u64);
        loop {
            let page = list_after(
                client.as_ref(),
                DirectoryRoot(value.content_root),
                after.as_ref(),
                WINDOW_ENTRIES,
                WINDOW_BYTES,
                &mut work,
            )?;
            let serials: Vec<u64> = page.entries.iter().map(|(_, serial)| *serial).collect();
            for bound in read.lookup_inodes(&serials)? {
                let bound = bound.ok_or(ContentError::InvalidRecord("bound inode is absent"))?;
                count += u64::from(bound.kind == InodeKind::Directory);
            }
            listed += serials.len() as u64;
            match page.continuation {
                Some(name) => after = Some(name),
                None => break,
            }
        }
        if listed != entries {
            return Err(ContentError::InvalidRecord("directory entry count"));
        }
        client.remember_directory_count(value.content_root, count)?;
        Ok(count)
    }
}
