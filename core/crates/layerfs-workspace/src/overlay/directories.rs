//! One generation-local directory delta and its exact immutable origin.
use super::pieces::{get, CapturedBase};
use crate::{
    backing::{
        metadata::RootOwner,
        metadata_pages::{self, PageRef},
        segments::Window,
    },
    NodeAttributes, NodeKind, WorkspaceError,
};
use layerfs_bridge::contract::Root;
use std::time::Instant;

#[derive(Clone, Copy)]
pub enum Origin {
    Empty,
    Canonical(Root),
    Captured(CapturedBase),
}
#[derive(Clone, Copy)]
pub struct Directory {
    pub origin: Origin,
    pub generation: u64,
    pub revision: u64,
    pub entries: PageRef,
    pub count: u16,
    pub bytes: u32,
    pub mode: u32,
    pub seconds: i64,
    pub nanos: u32,
}
impl Directory {
    pub fn initial(attr: NodeAttributes, base: Root) -> Self {
        Self {
            origin: Origin::Canonical(base),
            generation: 0,
            revision: 0,
            entries: PageRef::NULL,
            count: 0,
            bytes: 0,
            mode: attr.mode,
            seconds: attr.mtime_seconds,
            nanos: attr.mtime_nanoseconds,
        }
    }
    pub fn value(self) -> [u8; 128] {
        let mut value = [0; 128];
        value[0] = match self.origin {
            Origin::Empty => 0,
            Origin::Canonical(_) => 1,
            Origin::Captured(_) => 2,
        };
        value[8..16].copy_from_slice(&self.generation.to_be_bytes());
        value[16..24].copy_from_slice(&self.revision.to_be_bytes());
        match self.origin {
            Origin::Empty => {}
            Origin::Canonical(base) => value[24..56].copy_from_slice(&base),
            Origin::Captured(base) => value[24..56].copy_from_slice(&base.bytes()),
        }
        value[56..64].copy_from_slice(&self.entries.bytes());
        value[64..66].copy_from_slice(&self.count.to_be_bytes());
        value[68..72].copy_from_slice(&self.bytes.to_be_bytes());
        value[72..76].copy_from_slice(&self.mode.to_be_bytes());
        value[76..84].copy_from_slice(&self.seconds.to_be_bytes());
        value[84..88].copy_from_slice(&self.nanos.to_be_bytes());
        value
    }
    pub fn parse(value: &[u8]) -> Result<Self, WorkspaceError> {
        if value.len() != 128
            || value[0] > 2
            || value[1..8]
                .iter()
                .chain(&value[66..68])
                .chain(&value[88..])
                .any(|b| *b != 0)
        {
            return Err(WorkspaceError::Io);
        }
        let base: Root = value[24..56].try_into().map_err(|_| WorkspaceError::Io)?;
        let directory = Self {
            origin: match value[0] {
                0 if base == [0; 32] => Origin::Empty,
                1 => Origin::Canonical(base),
                2 => Origin::Captured(CapturedBase::parse(base)?),
                _ => return Err(WorkspaceError::Io),
            },
            generation: get(value, 8)?,
            revision: get(value, 16)?,
            entries: PageRef::parse(&value[56..64])?,
            count: u16::from_be_bytes(value[64..66].try_into().map_err(|_| WorkspaceError::Io)?),
            bytes: u32::from_be_bytes(value[68..72].try_into().map_err(|_| WorkspaceError::Io)?),
            mode: u32::from_be_bytes(value[72..76].try_into().map_err(|_| WorkspaceError::Io)?),
            seconds: i64::from_be_bytes(value[76..84].try_into().map_err(|_| WorkspaceError::Io)?),
            nanos: u32::from_be_bytes(value[84..88].try_into().map_err(|_| WorkspaceError::Io)?),
        };
        if directory.generation == 0
            || directory.revision == 0
            || directory.count > 128
            || (directory.count == 0) != (directory.entries == PageRef::NULL)
            || directory.bytes > u32::from(directory.count) * 265
            || directory.bytes < u32::from(directory.count) * 11
            || directory.mode & !0o1777 != 0
            || directory.nanos >= 1_000_000_000
        {
            return Err(WorkspaceError::Io);
        }
        Ok(directory)
    }
    pub fn attributes(self, mut attr: NodeAttributes) -> NodeAttributes {
        attr.kind = NodeKind::Directory;
        attr.size = 0;
        attr.mode = self.mode;
        attr.mtime_seconds = self.seconds;
        attr.mtime_nanoseconds = self.nanos;
        attr
    }
}
pub fn load(
    owner: &RootOwner,
    serial: u64,
    window: &mut Window,
    deadline: Instant,
) -> Result<Option<Directory>, WorkspaceError> {
    owner
        .arena
        .find(
            owner.root()?,
            &metadata_pages::namespace_key(serial),
            window,
            deadline,
        )?
        .map(|cell| Directory::parse(cell.value()))
        .transpose()
}
pub fn captured(
    owner: &RootOwner,
    reference: CapturedBase,
    window: &mut Window,
    deadline: Instant,
) -> Result<Directory, WorkspaceError> {
    let parent = owner.parent.as_ref().ok_or(WorkspaceError::Io)?;
    if parent.root()? != reference.root {
        return Err(WorkspaceError::Io);
    }
    let directory = load(parent, reference.inode, window, deadline)?.ok_or(WorkspaceError::Io)?;
    if directory.generation != reference.generation
        || directory.revision != reference.revision
        || matches!(directory.origin, Origin::Captured(_))
    {
        return Err(WorkspaceError::Io);
    }
    Ok(directory)
}
pub fn entry(serial: u64, kind: crate::NodeKind) -> Result<[u8; 16], WorkspaceError> {
    let mut value = [0; 16];
    value[..8].copy_from_slice(&serial.to_be_bytes());
    value[8] = match kind {
        crate::NodeKind::File => 1,
        crate::NodeKind::Directory => 2,
        crate::NodeKind::Symlink => 3,
    };
    Ok(value)
}
pub fn entry_serial(value: &[u8]) -> Result<u64, WorkspaceError> {
    entry_info(value).map(|(serial, _)| serial)
}
pub fn entry_info(value: &[u8]) -> Result<(u64, crate::NodeKind), WorkspaceError> {
    if value.len() != 16 || !matches!(value[8], 1..=3) || value[9..].iter().any(|b| *b != 0) {
        return Err(WorkspaceError::Io);
    }
    let serial = get(value, 0)?;
    if serial == 0 || serial > i64::MAX as u64 {
        return Err(WorkspaceError::Io);
    }
    Ok((
        serial,
        match value[8] {
            1 => crate::NodeKind::File,
            2 => crate::NodeKind::Directory,
            _ => crate::NodeKind::Symlink,
        },
    ))
}
