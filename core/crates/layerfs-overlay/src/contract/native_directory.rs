//! Indexed directory owners, independent reads and accepted-entry cookie plans.
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
}
impl NativeDirectoryCursor {
    pub fn after_name(&self) -> Option<&[u8]> {
        match self {
            Self::Names(Some(name)) => Some(name),
            _ => None,
        }
    }
}
/// This source protects metadata and the directory's cookie state independently
/// of the descriptor. Copying it does not acquire another read owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeDirectoryRead {
    pub(crate) source: BaseSource,
    pub(crate) directory: NativeDirectory,
    pub(crate) parent: u64,
    pub(crate) cursor: NativeDirectoryCursor,
}
impl NativeDirectoryRead {
    pub const fn source(&self) -> BaseSource {
        self.source
    }
    pub const fn directory(&self) -> NativeDirectory {
        self.directory
    }
    pub const fn parent(&self) -> u64 {
        self.parent
    }
    pub fn cursor(&self) -> &NativeDirectoryCursor {
        &self.cursor
    }
    pub fn heap_bytes(&self) -> usize {
        match &self.cursor {
            NativeDirectoryCursor::Names(Some(name)) => name.capacity(),
            _ => 0,
        }
    }
}
/// One owner's local name page and the current kinds of locally decided targets.
/// Immutable kind demands and name merging happen outside the SQL owner.
#[derive(Clone, Debug)]
pub struct NativeDirectoryPage {
    pub read: NativeDirectoryRead,
    pub after: Option<Vec<u8>>,
    pub local: DirectoryEntryWindow,
    pub local_kinds: Vec<(u64, InodeKind)>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeCookie {
    pub(crate) name: Vec<u8>,
    pub(crate) cookie: u64,
    pub(crate) existing: bool,
}
impl NativeCookie {
    pub fn name(&self) -> &[u8] {
        &self.name
    }
    pub const fn cookie(&self) -> u64 {
        self.cookie
    }
}
/// Allocator reservation alone changes no enumeration position. Only the exact
/// accepted prefix publishes mappings. Retain this original plan through its
/// publication outcome and reply attempt; neither Drop nor cloning releases it.
#[derive(Clone, Debug)]
pub struct NativeCookiePlan {
    pub(crate) read: NativeDirectoryRead,
    pub(crate) first: u64,
    pub(crate) end: u64,
    pub(crate) entries: Vec<NativeCookie>,
}
impl NativeCookiePlan {
    pub fn read(&self) -> &NativeDirectoryRead {
        &self.read
    }
    pub fn entries(&self) -> &[NativeCookie] {
        &self.entries
    }
    pub fn heap_bytes(&self) -> usize {
        self.entries.capacity() * std::mem::size_of::<NativeCookie>()
            + self
                .entries
                .iter()
                .map(|e| e.name.capacity())
                .sum::<usize>()
            + self.read.heap_bytes()
    }
}
