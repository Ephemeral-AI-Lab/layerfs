//! Indexed directory owners, one visit's local name page and the cookies of
//! one published reply.
use crate::{BaseSource, DirectoryEntryWindow, Inode, KernelMount};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KernelDirectory {
    pub(crate) mount: KernelMount,
    pub(crate) owner: u64,
    pub(crate) serial: u64,
}
impl KernelDirectory {
    pub const fn mount(self) -> KernelMount {
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
pub enum KernelDirectoryCursor {
    Start,
    AfterDot,
    Names(Option<Vec<u8>>),
    /// The offset was handed out before the handle was last listed from
    /// offset 0 over published replies. It denotes no position: the request
    /// is refused and the page lists nothing.
    Rewound,
}
impl KernelDirectoryCursor {
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
pub struct KernelDirectoryPage {
    pub(crate) directory: KernelDirectory,
    pub(crate) cursor: KernelDirectoryCursor,
    /// The handle's floor as the visit's fence read it.
    pub(crate) floor: u64,
    /// The parent the directory was reached through, read only for a reply
    /// that starts before its `..` entry.
    pub(crate) parent: Option<u64>,
    /// The name this window continues after: the cursor's, or a later one.
    pub after: Option<Vec<u8>>,
    pub local: DirectoryEntryWindow,
    /// The latest local inode row of every listed serial that has one,
    /// ascending by serial: the listing reads an entry's kind and attributes
    /// from here, with no further SQL.
    pub local_inodes: Vec<(u64, Inode)>,
}
impl KernelDirectoryPage {
    pub const fn directory(&self) -> KernelDirectory {
        self.directory
    }
    pub fn cursor(&self) -> &KernelDirectoryCursor {
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
pub struct KernelCookieOffer {
    pub(crate) directory: KernelDirectory,
    pub(crate) after: Vec<u8>,
    pub(crate) first: u64,
    pub(crate) published: Published,
    /// The entries this reply hands handles out for; empty for a READDIR.
    pub(crate) handled: Vec<KernelCookieEntry>,
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
impl KernelCookieOffer {
    pub const fn directory(&self) -> KernelDirectory {
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
    /// The entries whose handles this publication hands out, in reply order:
    /// the publishing visit takes one kernel lookup reference for each of
    /// them and records a directory's parent, so a handle the client later
    /// presents resolves without a LOOKUP of its own.
    pub fn adopt(&mut self, entries: impl IntoIterator<Item = (u64, bool)>) {
        self.handled = entries
            .into_iter()
            .map(|(serial, directory)| KernelCookieEntry { serial, directory })
            .collect();
    }
}
/// One entry whose handle a READDIRPLUS reply hands out.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KernelCookieEntry {
    pub serial: u64,
    /// A directory's parent is recorded with its reference.
    pub directory: bool,
}
