//! One generation-local directory delta and its exact immutable origin.
use super::pieces::{get, CapturedBase};
use crate::{
    backing::{
        metadata::RootOwner,
        metadata_pages::{self, Cell, PageRef},
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
    /// Names removed from the effective namespace of this maintained delta.
    /// Absent from both pages means inherit from the origin.
    pub tombstones: PageRef,
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
            tombstones: PageRef::NULL,
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
        value[88..96].copy_from_slice(&self.tombstones.bytes());
        value
    }
    pub fn parse(value: &[u8]) -> Result<Self, WorkspaceError> {
        if value.len() != 128
            || value[0] > 2
            || value[1..8]
                .iter()
                .chain(&value[66..68])
                .chain(&value[96..])
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
            tombstones: PageRef::parse(&value[88..96])?,
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
            || (directory.tombstones != PageRef::NULL && matches!(directory.origin, Origin::Empty))
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
/// The exact earlier version a maintained delta is a delta against.
///
/// A reference is the directory record its own revision names, read either from
/// the root the reference belongs to or from the owner's direct parent root. An
/// operation may publish more than one root inside one generation, so the record
/// is authoritative and the referring root is only a locator: the record must
/// still carry the referenced generation and revision and must not itself be a
/// maintained delta.
pub fn captured(
    owner: &RootOwner,
    reference: CapturedBase,
    window: &mut Window,
    deadline: Instant,
) -> Result<Directory, WorkspaceError> {
    let from_parent = owner
        .parent
        .as_ref()
        .filter(|parent| parent.root().is_ok_and(|root| root == reference.root));
    let source: &RootOwner = from_parent.map_or(owner, |parent| parent.as_ref());
    let found = load(source, reference.inode, window, deadline)?;
    let directory = found.ok_or(WorkspaceError::Io)?;
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

/// One checked removal of a name from a directory's effective namespace.
/// The same page the additions live in; a tombstone means absent, an absent
/// record means inherit from the delta's origin.
pub fn tombstone_key(name: &[u8]) -> Result<Vec<u8>, WorkspaceError> {
    if name.is_empty() || name.len() > 255 {
        return Err(WorkspaceError::InvalidInput);
    }
    let mut key = crate::backing::metadata_index::vector(name.len() + 1)?;
    key.push(b'T');
    key.extend_from_slice(name);
    Ok(key)
}
pub fn tombstone(cell: &[u8]) -> Result<&[u8], WorkspaceError> {
    if cell.len() < 2 || cell[0] != b'T' {
        return Err(WorkspaceError::Io);
    }
    Ok(&cell[1..])
}
pub fn removed(
    owner: &RootOwner,
    directory: Directory,
    name: &[u8],
    window: &mut Window,
    deadline: Instant,
) -> Result<bool, WorkspaceError> {
    if directory.tombstones == PageRef::NULL {
        return Ok(false);
    }
    Ok(owner
        .arena
        .find(
            directory.tombstones,
            &tombstone_key(name)?,
            window,
            deadline,
        )?
        .is_some())
}
/// True when this directory's own entry page binds `name` locally.
pub fn has_entry(
    owner: &RootOwner,
    entries: PageRef,
    name: &[u8],
    window: &mut Window,
    deadline: Instant,
) -> Result<bool, WorkspaceError> {
    if entries == PageRef::NULL {
        return Ok(false);
    }
    Ok(owner
        .arena
        .find(entries, &metadata_pages::entry_key(name)?, window, deadline)?
        .is_some())
}
/// Rebuilds one directory's removal page without `name`, so a name this delta
/// binds again carries no removal record. One name owns a binding or a removal,
/// never both.
pub fn keep_name(
    candidate: &RootOwner,
    directory: Directory,
    name: &[u8],
    window: &mut Window,
    deadline: Instant,
) -> Result<PageRef, WorkspaceError> {
    if directory.tombstones == PageRef::NULL {
        return Ok(PageRef::NULL);
    }
    let mut names: Vec<Vec<u8>> = crate::backing::metadata_index::vector(128)?;
    let mut lower = vec![b'T'];
    let mut removed = false;
    while let Some(cell) = candidate.arena.next(
        directory.tombstones,
        &lower,
        !names.is_empty(),
        window,
        deadline,
    )? {
        if names.len() == 128 || cell.key() <= lower.as_slice() {
            return Err(WorkspaceError::Capacity);
        }
        let existing = tombstone(cell.key())?;
        if existing == name {
            removed = true;
        } else {
            names.push(existing.to_vec());
        }
        lower = cell.key().to_vec();
    }
    if !removed {
        // The name carries no removal record, so the page is already the one
        // this binding needs and no page is written.
        return Ok(directory.tombstones);
    }
    if names.len() == 128 {
        return Err(WorkspaceError::Capacity);
    }
    candidate.build_ordered(name_page(&mut names, 0), window, deadline)
}
/// Rebuilds one directory's entry page without `name`, and reports whether the
/// name was bound locally. A removed name becomes a tombstone instead, because
/// one name must never own both a binding and its removal: lowering would emit
/// two records for the same name and refuse the delta.
pub fn drop_entry(
    candidate: &RootOwner,
    directory: Directory,
    name: &[u8],
    window: &mut Window,
    deadline: Instant,
) -> Result<(PageRef, bool), WorkspaceError> {
    if !has_entry(candidate, directory.entries, name, window, deadline)? {
        return Ok((directory.entries, false));
    }
    let mut kept: Vec<(Vec<u8>, Vec<u8>)> = crate::backing::metadata_index::vector(128)?;
    let mut lower = metadata_pages::entry_key(&[])?;
    // The cursor advances by the cell this walk last visited, which is not the
    // same as the last cell it kept: the dropped name is visited exactly once.
    let mut first = true;
    while let Some(cell) =
        candidate
            .arena
            .next(directory.entries, &lower, !first, window, deadline)?
    {
        if kept.len() == 128 || cell.key() <= lower.as_slice() {
            return Err(WorkspaceError::Capacity);
        }
        first = false;
        if &cell.key()[1..] != name {
            kept.push((cell.key().to_vec(), cell.value().to_vec()));
        }
        lower = cell.key().to_vec();
    }
    if kept.is_empty() {
        return Ok((PageRef::NULL, true));
    }
    let mut at = 0;
    let page = candidate.build_ordered(
        |_| {
            let Some((key, value)) = kept.get(at) else {
                return Ok(None);
            };
            at += 1;
            Ok(Some(Cell::new(key, value)?))
        },
        window,
        deadline,
    )?;
    Ok((page, true))
}
/// Rebuilds one directory's tombstone page with `name` added. Existing names
/// stay in key order; the exact added name is the caller's single new removal.
/// The candidate owner holds this operation's page allowance, so the rebuilt
/// page is written through it exactly like every other page of the publication.
pub fn remove_name(
    candidate: &RootOwner,
    directory: Directory,
    name: &[u8],
    window: &mut Window,
    deadline: Instant,
) -> Result<PageRef, WorkspaceError> {
    if directory.tombstones == PageRef::NULL {
        let mut only = crate::backing::metadata_index::vector(1)?;
        only.push(name.to_vec());
        return candidate.build_ordered(name_page(&mut only, 0), window, deadline);
    }
    let mut names: Vec<Vec<u8>> = crate::backing::metadata_index::vector(128)?;
    let mut lower = vec![b'T'];
    while let Some(cell) = candidate.arena.next(
        directory.tombstones,
        &lower,
        !names.is_empty(),
        window,
        deadline,
    )? {
        if names.len() == 128 || cell.key() <= lower.as_slice() {
            return Err(WorkspaceError::Capacity);
        }
        let existing = tombstone(cell.key())?;
        if existing == name {
            return Ok(directory.tombstones);
        }
        names.push(existing.to_vec());
        lower = cell.key().to_vec();
    }
    let at = names
        .binary_search_by(|existing| existing.as_slice().cmp(name))
        .unwrap_or_else(|at| at);
    names.insert(at, name.to_vec());
    candidate.build_ordered(name_page(&mut names, 0), window, deadline)
}
/// Yields the remaining tombstone entries in key order, one per call.
fn name_page(
    names: &mut [Vec<u8>],
    mut at: usize,
) -> impl FnMut(&mut Window) -> Result<Option<Cell>, WorkspaceError> + '_ {
    move |_| {
        if at == names.len() {
            return Ok(None);
        }
        let name = &names[at];
        at += 1;
        Ok(Some(Cell::new(&tombstone_key(name)?, &[1])?))
    }
}
