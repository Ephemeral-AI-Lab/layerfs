//! Indexed directory owners, one visit's local name page and the cookies of
//! one published reply.
use crate::{BaseSource, DirectoryEntryWindow, InodeKind, NativeMount};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeDirectory {
    pub(crate) mount: NativeMount,
    pub(crate) owner: u64,
    pub(crate) serial: u64,
}
impl NativeDirectory {
    pub const fn mount(self) -> NativeMount {
        self.mount
    }
    pub const fn owner_id(self) -> u64 {
        self.owner
    }
    pub const fn serial(self) -> u64 {
        self.serial
    }
}

/// Position immediately following the supplied kernel offset. Dot entries use
/// fixed cookies1/2; every ordinary cookie retains its own exact name boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeDirectoryCursor {
    Start,
    AfterDot,
    Names(Option<Vec<u8>>),
    /// The offset was handed out before the handle was last listed from
    /// offset 0 over published replies. It denotes no position: the request
    /// is refused and the page lists nothing.
    Rewound,
}
impl NativeDirectoryCursor {
    pub fn after_name(&self) -> Option<&[u8]> {
        match self {
            Self::Names(Some(name)) => Some(name),
            _ => None,
        }
    }
}
/// What one read-only READDIR visit found: the open directory, the position
/// its kernel offset denotes, and one window of current local names with the
/// kinds of their local inodes. The request owns it as plain values; nothing
/// is recorded for it. Immutable names and kinds are merged outside SQL.
#[derive(Clone, Debug)]
pub struct NativeDirectoryPage {
    pub(crate) directory: NativeDirectory,
    pub(crate) cursor: NativeDirectoryCursor,
    /// The handle's floor as the visit's fence read it.
    pub(crate) floor: u64,
    /// The parent the directory was reached through, read only for a reply
    /// that starts before its `..` entry.
    pub(crate) parent: Option<u64>,
    /// The name this window continues after: the cursor's, or a later one.
    pub after: Option<Vec<u8>>,
    pub local: DirectoryEntryWindow,
    /// (serial, kind) of every listed inode that has a local row, ascending.
    pub local_kinds: Vec<(u64, InodeKind)>,
}
impl NativeDirectoryPage {
    pub const fn directory(&self) -> NativeDirectory {
        self.directory
    }
    pub fn cursor(&self) -> &NativeDirectoryCursor {
        &self.cursor
    }
    pub const fn parent(&self) -> Option<u64> {
        self.parent
    }
    /// The visit's own source: the base its inherited names belong to.
    pub const fn source(&self) -> BaseSource {
        self.local.source
    }
}
/// The offsets one reply may hand out: the latest published reply of this
/// open directory that was listed after the same name, and a fresh range of
/// `PAGE_ROWS` numbers no one else holds. Neither makes an offset valid;
/// only publication of accepted names does.
///
/// An offer made at offset 0 for a handle that has published replies is a
/// rewind: it reuses none, and its publication raises the handle's floor to
/// its fresh range, which retires every earlier reply.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeCookieOffer {
    pub(crate) directory: NativeDirectory,
    pub(crate) after: Vec<u8>,
    pub(crate) first: u64,
    pub(crate) published: Published,
}
/// What an offer found among the handle's published replies.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Published {
    /// No reply listed after the same name.
    None,
    /// The latest reply listed after the same name: its first cookie and
    /// its names, to reuse.
    Latest(u64, Vec<Vec<u8>>),
    /// A reply listed from the start, found at offset 0: a rewind.
    Rewind,
}
impl NativeCookieOffer {
    pub const fn directory(&self) -> NativeDirectory {
        self.directory
    }
    /// The first number of the fresh range.
    pub const fn first(&self) -> u64 {
        self.first
    }
    /// Whether publishing this offer retires the handle's earlier replies.
    /// Such an offer is published even when its reply accepted no name.
    pub const fn rewinds(&self) -> bool {
        matches!(self.published, Published::Rewind)
    }
    /// The first cookie and the names of the published reply to reuse.
    pub fn existing(&self) -> Option<(u64, &[Vec<u8>])> {
        match &self.published {
            Published::Latest(first, names) => Some((*first, names.as_slice())),
            _ => None,
        }
    }
    pub fn heap_bytes(&self) -> usize {
        self.after.capacity()
            + self.existing().map_or(0, |(_, names)| {
                std::mem::size_of_val(names) + names.iter().map(Vec::capacity).sum::<usize>()
            })
    }
}
