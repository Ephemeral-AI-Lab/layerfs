//! Public read-only Workspace view-lease identities and results.
use crate::{WorkspaceError, WorkspaceId};
use layerfs_bridge::contract::{
    Code, Failure, Root, WorkspaceViewEntryWire, WorkspaceViewLeaseWire, WorkspaceViewListWire,
    WorkspaceViewReadWire, WorkspaceViewReleaseOutcome, WorkspaceViewStatusWire,
};

/// The kind of one entry a view lease resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceViewKind {
    File,
    Directory,
    Symlink,
}

impl WorkspaceViewKind {
    /// Decodes one wire kind code.
    pub fn from_code(code: u8) -> Result<Self, WorkspaceError> {
        match code {
            1 => Ok(Self::File),
            2 => Ok(Self::Directory),
            3 => Ok(Self::Symlink),
            _ => Err(Code::Integrity.into()),
        }
    }
}

/// One opaque entry bound to the lease that resolved it. It carries selected
/// identity and attributes only, never a physical page identity, and it is
/// refused by every lease other than its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceViewEntry {
    /// Opaque binding to the lease that resolved this entry.
    pub lease: [u8; 33],
    pub serial: u64,
    pub kind: WorkspaceViewKind,
    pub size: u64,
    pub references: u64,
    pub mode: u32,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
}

impl WorkspaceViewEntry {
    /// Builds one entry from its wire form, bound to one lease token.
    pub fn from_wire(
        lease: &[u8; 33],
        wire: &WorkspaceViewEntryWire,
    ) -> Result<Self, WorkspaceError> {
        Ok(Self {
            lease: *lease,
            serial: wire.serial,
            kind: WorkspaceViewKind::from_code(wire.kind)?,
            size: wire.size,
            references: wire.references,
            mode: wire.mode,
            mtime_seconds: wire.mtime_seconds,
            mtime_nanoseconds: wire.mtime_nanoseconds,
        })
    }
}

/// One held read-only lease on the current selected view of an attached
/// Workspace, bound to that Workspace and incarnation. It is an opaque client
/// handle: cloning shares it, and dropping it does **not** release the pin.
/// The checked remote release is the explicit `release_view` call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceViewLease {
    /// The attached Workspace this lease is bound to.
    pub workspace: WorkspaceId,
    pub(crate) incarnation: Root,
    /// Opaque lease token; dead after a Completed release.
    pub token: [u8; 33],
    pub(crate) root: WorkspaceViewEntry,
    pub(crate) generation: u64,
    pub(crate) revision: u64,
}

impl WorkspaceViewLease {
    /// The pinned root entry. A local accessor; no remote call.
    pub fn root(&self) -> &WorkspaceViewEntry {
        &self.root
    }
    /// The pinned generation, as observed when the lease was pinned.
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// The pinned revision, as observed when the lease was pinned.
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// Builds one lease from its wire form, bound to one Workspace identity.
    pub fn from_wire(
        workspace: &WorkspaceId,
        wire: &WorkspaceViewLeaseWire,
    ) -> Result<Self, WorkspaceError> {
        let token: [u8; 33] = wire
            .view
            .as_slice()
            .try_into()
            .map_err(|_| WorkspaceError::Failure(Code::Integrity.into()))?;
        Ok(Self {
            workspace: workspace.clone(),
            incarnation: wire.incarnation,
            token,
            root: WorkspaceViewEntry::from_wire(&token, &wire.root)?,
            generation: wire.generation,
            revision: wire.revision,
        })
    }
}

/// One bounded page of a pinned directory listing. Entries are `(name,
/// serial)`; a full entry comes from one component lookup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceViewDirectoryPage {
    pub entries: Vec<(Vec<u8>, u64)>,
    pub continuation: Option<Vec<u8>>,
}

impl WorkspaceViewDirectoryPage {
    pub fn from_wire(wire: &WorkspaceViewListWire) -> Self {
        Self {
            entries: wire.entries.clone(),
            continuation: wire.continuation.clone(),
        }
    }
}

/// Bounded pinned bytes with end-of-file and the pinned logical size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceViewRead {
    pub bytes: Vec<u8>,
    pub eof: bool,
    pub size: u64,
}

impl WorkspaceViewRead {
    pub fn from_wire(wire: &WorkspaceViewReadWire) -> Self {
        Self {
            bytes: wire.bytes.clone(),
            eof: wire.eof,
            size: wire.size,
        }
    }
}

/// Read-only custody observation of one held lease.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkspaceViewStatus {
    pub generation: u64,
    pub revision: u64,
    pub entries: u64,
    pub held_leases: u64,
}

impl WorkspaceViewStatus {
    pub fn from_wire(wire: &WorkspaceViewStatusWire) -> Self {
        Self {
            generation: wire.generation,
            revision: wire.revision,
            entries: wire.entries,
            held_leases: wire.held_leases,
        }
    }
}

/// The checked remote release outcome. `Retained` means physical custody
/// stayed with the Workspace (which stopped); the lease was not retried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceViewRelease {
    Completed,
    Retained { cause: Code },
}

impl WorkspaceViewRelease {
    pub fn from_wire(outcome: WorkspaceViewReleaseOutcome) -> Self {
        match outcome {
            WorkspaceViewReleaseOutcome::Completed => Self::Completed,
            WorkspaceViewReleaseOutcome::Retained(cause) => Self::Retained { cause },
        }
    }
}

impl From<Failure> for WorkspaceViewRelease {
    fn from(_: Failure) -> Self {
        Self::Retained { cause: Code::Io }
    }
}
