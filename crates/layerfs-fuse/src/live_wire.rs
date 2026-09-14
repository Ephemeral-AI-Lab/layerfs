//! Live-owner transport values. These carry resolved facts, never host-side POSIX edits.
use layerfs_content::file::content::FileContentRoot;
use layerfs_content::tree::directory::DirectoryStateRoot;
use layerfs_content::tree::inode::InodeId;
use layerfs_content::{CanonicalName, CanonicalPath, ObjectId};
use layerfs_workspace_core::backing::{BackingId, BackingRef};
use layerfs_workspace_core::file_edit::{CompactPending, Piece, PieceTree};
use layerfs_workspace_core::{Data, DirectoryData, FileData, Node, NodeId};
use std::io::{self, Read, Write};

pub const FACT_PAGE_BYTES: usize = 64 * 1024;
pub const FACT_PAGE_NODES: usize = 128;
pub const MAX_NODE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_FACT_MEMORY: usize = 96 * 1024 * 1024;

pub const MAX_FRAME: usize = 1024 * 1024 + 64 * 1024;
pub const IMMUTABLE_PREFETCH_FILE_BYTES: usize = 8 * 1024;
pub const IMMUTABLE_PREFETCH_PAGE_BYTES: usize = 512 * 1024;
pub const SEED: u8 = 1;
pub const LOOKUP: u8 = 2;
pub const RESERVE: u8 = 3;
pub const APPEND: u8 = 4;
pub const READ_BACKING: u8 = 5;
pub const READ_BASE: u8 = 6;
pub const CHECK: u8 = 7;
pub const RELEASE: u8 = 8;
pub const FACTS_BEGIN: u8 = 9;
pub const FACTS_NODE: u8 = 10;
pub const FACTS_END: u8 = 11;
pub const CANCEL_RESERVATION: u8 = 12;
pub const DIRECTORY_PAGE: u8 = 13;
pub const FACTS_NODE_BEGIN: u8 = 14;
pub const FACTS_NODE_CHUNK: u8 = 15;
/// Point metadata lookup without speculative sibling or payload export.
pub const LOOKUP_METADATA: u8 = 16;
/// Ordered no-result backing requests with one acknowledged completed prefix.
pub const BATCH: u8 = 17;
pub const MAX_BATCH_FRAMES: usize = 128;

pub fn validate_batch_frame(bytes: &[u8]) -> io::Result<()> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err(invalid());
    }
    let mut input = Input(bytes);
    match input.byte()? {
        APPEND => {
            input.u64()?;
            input.u64()?;
            let data = input.bytes()?;
            if data.is_empty() || data.len() > 1024 * 1024 {
                return Err(invalid());
            }
        }
        CANCEL_RESERVATION | FACTS_END => {
            input.u64()?;
            input.u64()?;
        }
        CHECK => {
            if input.byte()? > 1 || input.0.len() % 8 != 0 {
                return Err(invalid());
            }
            input.0 = &[];
        }
        RELEASE => {
            if input.0.len() % 8 != 0 {
                return Err(invalid());
            }
            input.0 = &[];
        }
        FACTS_BEGIN => {
            input.u64()?;
        }
        FACTS_NODE => {
            let mut count = 0;
            while !input.0.is_empty() {
                count += 1;
                if count > FACT_PAGE_NODES || input.byte()? > 1 || input.bytes()?.is_empty() {
                    return Err(invalid());
                }
            }
        }
        FACTS_NODE_BEGIN => {
            if input.byte()? > 1 || !(1..=MAX_NODE_BYTES as u64).contains(&input.u64()?) {
                return Err(invalid());
            }
        }
        FACTS_NODE_CHUNK => {
            if input.0.is_empty() {
                return Err(invalid());
            }
            input.0 = &[];
        }
        _ => return Err(invalid()),
    }
    input.done()
}

/// Validate every frame before any request in the envelope can mutate state.
pub fn batch_frames(bytes: &[u8]) -> io::Result<Vec<&[u8]>> {
    if bytes.len() > MAX_FRAME {
        return Err(invalid());
    }
    let mut input = Input(bytes);
    if input.byte()? != BATCH {
        return Err(invalid());
    }
    let count = input.u32()? as usize;
    if !(1..=MAX_BATCH_FRAMES).contains(&count) {
        return Err(invalid());
    }
    let mut frames = Vec::with_capacity(count);
    for _ in 0..count {
        let frame = input.bytes()?;
        validate_batch_frame(frame)?;
        frames.push(frame);
    }
    input.done()?;
    Ok(frames)
}

pub fn batch_reply(completed: usize, error: Option<crate::PortError>) -> Vec<u8> {
    let mut out = (completed as u32).to_be_bytes().to_vec();
    out.push(u8::from(error.is_some()));
    if let Some(error) = error {
        out.push(crate::protocol::error_code(error));
    }
    out
}

pub(crate) fn parse_batch_reply(
    bytes: &[u8],
    count: usize,
) -> io::Result<(usize, Option<crate::PortError>)> {
    let mut input = Input(bytes);
    let completed = input.u32()? as usize;
    let error = match input.byte()? {
        0 if completed == count => None,
        1 if completed < count => Some(crate::protocol::port_error(input.byte()?)?),
        _ => return Err(invalid()),
    };
    input.done()?;
    Ok((completed, error))
}

pub fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "live owner transport")
}
pub fn u64_out(out: &mut Vec<u8>, n: u64) {
    out.extend_from_slice(&n.to_be_bytes());
}
pub fn bytes_out(out: &mut Vec<u8>, bytes: &[u8]) -> io::Result<()> {
    let size = u32::try_from(bytes.len()).map_err(|_| invalid())?;
    out.extend_from_slice(&size.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}
pub fn frame_out(out: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err(invalid());
    }
    out.write_all(&(bytes.len() as u32).to_be_bytes())?;
    out.write_all(bytes)
}
pub fn frame_in(input: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut length = [0; 4];
    input.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_FRAME {
        return Err(invalid());
    }
    let mut bytes = vec![0; length];
    input.read_exact(&mut bytes)?;
    Ok(bytes)
}
pub struct Input<'a>(pub &'a [u8]);
impl<'a> Input<'a> {
    pub fn raw(&mut self, count: usize) -> io::Result<&'a [u8]> {
        if count > self.0.len() {
            return Err(invalid());
        }
        let (value, rest) = self.0.split_at(count);
        self.0 = rest;
        Ok(value)
    }
    pub fn byte(&mut self) -> io::Result<u8> {
        Ok(self.raw(1)?[0])
    }
    pub fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_be_bytes(self.raw(4)?.try_into().unwrap()))
    }
    pub fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_be_bytes(self.raw(8)?.try_into().unwrap()))
    }
    pub fn head(&mut self) -> io::Result<Option<[u8; 33]>> {
        let bytes = self.bytes()?;
        if bytes.is_empty() {
            return Ok(None);
        }
        if bytes.len() != 33 || bytes[0] != 0x12 {
            return Err(invalid());
        }
        Ok(Some(bytes.try_into().map_err(|_| invalid())?))
    }
    pub fn object(&mut self) -> io::Result<ObjectId> {
        ObjectId::from_bytes(self.raw(32)?).map_err(|_| invalid())
    }
    pub fn bytes(&mut self) -> io::Result<&'a [u8]> {
        let n = self.u32()? as usize;
        self.raw(n)
    }
    pub fn done(self) -> io::Result<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(invalid())
        }
    }
    fn count(&mut self, minimum: usize) -> io::Result<usize> {
        let count = self.u32()? as usize;
        if count > self.0.len() / minimum {
            return Err(invalid());
        }
        Ok(count)
    }
}

pub fn node_encoded_bound(node: &Node) -> io::Result<usize> {
    let paths = node
        .paths
        .iter()
        .try_fold(0usize, |n, path| n.checked_add(4 + path.len()))
        .ok_or_else(invalid)?;
    let data = match &node.data {
        Data::File(FileData::Edited { pieces, .. }) => {
            // Bounded pending form: one base root plus one equal-length splice,
            // bounded exactly as encoded instead of as three logical pieces.
            if let Some(compact) = pieces.compact_encoded_bound() {
                usize::try_from(compact).ok()
            } else {
                pieces.count().checked_mul(49).and_then(|n| {
                    usize::try_from(pieces.inline_len())
                        .ok()
                        .and_then(|inline| n.checked_add(inline))
                })
            }
        }
        Data::Directory(directory) => directory
            .changes
            .keys()
            .try_fold(0usize, |n, name| n.checked_add(12 + name.len())),
        Data::Symlink(target) => Some(target.len()),
        Data::File(_) => Some(0),
    }
    .ok_or_else(invalid)?;
    let capacity = 128usize
        .checked_add(paths)
        .and_then(|n| n.checked_add(data))
        .filter(|n| *n <= MAX_NODE_BYTES)
        .ok_or_else(invalid)?;
    Ok(capacity)
}

pub fn node_out(id: NodeId, node: &Node) -> io::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(node_encoded_bound(node)?);
    u64_out(&mut out, id.0);
    u64_out(&mut out, node.revision);
    out.push(u8::from(node.canonical.is_some()));
    if let Some(inode) = node.canonical {
        out.extend_from_slice(inode.as_bytes());
    }
    out.extend_from_slice(&node.mode.to_be_bytes());
    out.extend_from_slice(&node.links.to_be_bytes());
    out.extend_from_slice(&node.pins.to_be_bytes());
    out.extend_from_slice(&node.mtime_seconds.to_be_bytes());
    out.extend_from_slice(&node.mtime_nanoseconds.to_be_bytes());
    out.extend_from_slice(&(u32::try_from(node.paths.len()).map_err(|_| invalid())?).to_be_bytes());
    for path in &node.paths {
        bytes_out(&mut out, path.as_bytes())?;
    }
    match &node.data {
        Data::File(FileData::Base { root, len }) => {
            out.push(0);
            out.extend_from_slice(root.0.as_bytes());
            u64_out(&mut out, *len);
        }
        Data::File(FileData::Edited {
            base,
            spool_high_water,
            pieces,
            edits,
        }) => {
            // Kind 4: the bounded pending form. The envelope carries the same
            // base, spool high-water mark and edit counter; the splice replaces
            // the piece list with one bounded descriptor.
            if let Some(compact) = pieces.compact_pending() {
                out.push(4);
                out.push(1);
                let (root, len) = match &compact {
                    CompactPending::Inline { base, len, .. } => (*base, *len),
                    CompactPending::Spool { base, len, .. } => (*base, *len),
                };
                out.extend_from_slice(root.0.as_bytes());
                u64_out(&mut out, len);
                u64_out(&mut out, *spool_high_water);
                out.extend_from_slice(&edits.to_be_bytes());
                match &compact {
                    CompactPending::Inline { offset, bytes, .. } => {
                        out.push(1);
                        u64_out(&mut out, *offset);
                        bytes_out(&mut out, bytes)?;
                    }
                    CompactPending::Spool { offset, slice, .. } => {
                        out.push(2);
                        u64_out(&mut out, *offset);
                        u64_out(&mut out, slice.segment.id().0);
                        u64_out(&mut out, slice.offset);
                        u64_out(&mut out, slice.len);
                    }
                }
                if out.len() > MAX_NODE_BYTES {
                    return Err(invalid());
                }
                return Ok(out);
            }
            out.push(1);
            out.push(u8::from(base.is_some()));
            if let Some((root, len)) = base {
                out.extend_from_slice(root.0.as_bytes());
                u64_out(&mut out, *len);
            }
            u64_out(&mut out, *spool_high_water);
            out.extend_from_slice(&edits.to_be_bytes());
            out.extend_from_slice(
                &(u32::try_from(pieces.count()).map_err(|_| invalid())?).to_be_bytes(),
            );
            for piece in pieces.cursor() {
                match piece {
                    Piece::Base { root, offset, len } => {
                        out.push(0);
                        out.extend_from_slice(root.0.as_bytes());
                        u64_out(&mut out, offset);
                        u64_out(&mut out, len);
                    }
                    Piece::Spool {
                        segment,
                        offset,
                        len,
                    } => {
                        out.push(1);
                        u64_out(&mut out, segment.id().0);
                        u64_out(&mut out, offset);
                        u64_out(&mut out, len);
                    }
                    Piece::Zero { len } => {
                        out.push(2);
                        u64_out(&mut out, len);
                    }
                    Piece::Inline { bytes, offset, len } => {
                        out.push(3);
                        let start = usize::try_from(offset).map_err(|_| invalid())?;
                        let end = usize::try_from(offset.checked_add(len).ok_or_else(invalid)?)
                            .map_err(|_| invalid())?;
                        bytes_out(&mut out, bytes.get(start..end).ok_or_else(invalid)?)?;
                    }
                }
            }
        }
        Data::Directory(directory) => {
            out.push(2);
            out.push(u8::from(directory.base.is_some()));
            if let Some(base) = directory.base {
                out.extend_from_slice(base.0.as_bytes());
            }
            out.extend_from_slice(
                &(u32::try_from(directory.changes.len()).map_err(|_| invalid())?).to_be_bytes(),
            );
            for (name, child) in &directory.changes {
                bytes_out(&mut out, name)?;
                u64_out(&mut out, child.map_or(0, |id| id.0));
            }
        }
        Data::Symlink(target) => {
            out.push(3);
            bytes_out(&mut out, target)?;
        }
    }
    if out.len() > MAX_NODE_BYTES {
        return Err(invalid());
    }
    Ok(out)
}

pub fn node_in(
    bytes: &[u8],
    mut backing: impl FnMut(BackingId, u64, u64) -> io::Result<BackingRef>,
) -> io::Result<(NodeId, Node)> {
    if bytes.len() > MAX_NODE_BYTES {
        return Err(invalid());
    }
    let mut input = Input(bytes);
    let id = NodeId(input.u64()?);
    if id.0 == 0 {
        return Err(invalid());
    }
    let revision = input.u64()?;
    let canonical = match input.byte()? {
        0 => None,
        1 => Some(InodeId(input.raw(32)?.try_into().unwrap())),
        _ => return Err(invalid()),
    };
    let mode = input.u32()?;
    let links = input.u32()?;
    let pins = input.u32()?;
    let mtime_seconds = input.u64()? as i64;
    let mtime_nanoseconds = input.u32()?;
    if mtime_nanoseconds >= 1_000_000_000 {
        return Err(invalid());
    }
    let count = input.count(4)?;
    let mut paths = std::collections::BTreeSet::new();
    for _ in 0..count {
        let path = std::str::from_utf8(input.bytes()?).map_err(|_| invalid())?;
        CanonicalPath::new(path).map_err(|_| invalid())?;
        if !paths.insert(path.to_owned()) {
            return Err(invalid());
        }
    }
    let data = match input.byte()? {
        0 => Data::File(FileData::Base {
            root: FileContentRoot(input.object()?),
            len: input.u64()?,
        }),
        1 => {
            let base = match input.byte()? {
                0 => None,
                1 => Some((FileContentRoot(input.object()?), input.u64()?)),
                _ => return Err(invalid()),
            };
            let spool_high_water = input.u64()?;
            // The cumulative pre-Commit edit count is informational; the piece,
            // inline, spool and allocation charges below remain the real bounds.
            let edits = input.u64()?;
            let count = input.count(5)?;
            if count > layerfs_workspace_core::file_edit::MAX_PIECES_PER_FILE {
                return Err(invalid());
            }
            let mut pieces = Vec::with_capacity(count);
            for _ in 0..count {
                pieces.push(match input.byte()? {
                    0 => Piece::Base {
                        root: FileContentRoot(input.object()?),
                        offset: input.u64()?,
                        len: input.u64()?,
                    },
                    1 => {
                        let id = BackingId(input.u64()?);
                        let offset = input.u64()?;
                        let len = input.u64()?;
                        offset.checked_add(len).ok_or_else(invalid)?;
                        Piece::Spool {
                            segment: backing(id, offset, len)?,
                            offset,
                            len,
                        }
                    }
                    2 => Piece::Zero { len: input.u64()? },
                    3 => {
                        let bytes: std::sync::Arc<[u8]> = input.bytes()?.into();
                        let len = bytes.len() as u64;
                        Piece::Inline {
                            bytes,
                            offset: 0,
                            len,
                        }
                    }
                    _ => return Err(invalid()),
                });
            }
            Data::File(FileData::Edited {
                base,
                spool_high_water,
                edits,
                pieces: PieceTree::empty()
                    .replace(0, 0, pieces)
                    .map_err(|_| invalid())?,
            })
        }
        4 => {
            // Bounded pending form: one base root, one equal-length splice.
            if input.byte()? != 1 {
                return Err(invalid());
            }
            let base = FileContentRoot(input.object()?);
            let len = input.u64()?;
            let spool_high_water = input.u64()?;
            let edits = input.u64()?;
            let pieces = match input.byte()? {
                1 => {
                    let offset = input.u64()?;
                    PieceTree::compact_inline(base, len, offset, input.bytes()?.into())
                        .map_err(|_| invalid())?
                }
                2 => {
                    let offset = input.u64()?;
                    let id = BackingId(input.u64()?);
                    let slice_offset = input.u64()?;
                    let slice_len = input.u64()?;
                    slice_offset.checked_add(slice_len).ok_or_else(invalid)?;
                    PieceTree::compact_spool_splice(
                        base,
                        len,
                        offset,
                        layerfs_workspace_core::file_edit::SpoolSlice {
                            segment: backing(id, slice_offset, slice_len)?,
                            offset: slice_offset,
                            len: slice_len,
                        },
                    )
                    .map_err(|_| invalid())?
                }
                _ => return Err(invalid()),
            };
            Data::File(FileData::Edited {
                base: Some((base, len)),
                spool_high_water,
                edits,
                pieces,
            })
        }
        2 => {
            let base = match input.byte()? {
                0 => None,
                1 => Some(DirectoryStateRoot(input.object()?)),
                _ => return Err(invalid()),
            };
            let count = input.count(12)?;
            let mut changes = std::collections::BTreeMap::new();
            for _ in 0..count {
                let name = input.bytes()?;
                CanonicalName::from_bytes(name).map_err(|_| invalid())?;
                let node = input.u64()?;
                if changes
                    .insert(name.to_vec(), (node != 0).then_some(NodeId(node)))
                    .is_some()
                {
                    return Err(invalid());
                }
            }
            Data::Directory(DirectoryData { base, changes })
        }
        3 => {
            let target = input.bytes()?;
            if target.len() > 4096 || target.contains(&0) {
                return Err(invalid());
            }
            Data::Symlink(target.to_vec())
        }
        _ => return Err(invalid()),
    };
    input.done()?;
    Ok((
        id,
        Node {
            revision,
            canonical,
            paths,
            mode,
            links,
            pins,
            mtime_seconds,
            mtime_nanoseconds,
            data,
        },
    ))
}

pub const FREEZE: u8 = 32;
pub const RESUME: u8 = 33;
pub const OBSERVE: u8 = 34;
pub const INSTALL_BEGIN: u8 = 35;
pub const INSTALL_NODE: u8 = 36;
pub const INSTALL_END: u8 = 37;
pub const SHUTDOWN: u8 = 38;
pub const WRITE_METRICS: u8 = 39;
pub const READ_METRICS: u8 = 40;
pub const INVALIDATE: u8 = 41;

pub const EDIT_BEGIN: u8 = 42;
pub const EDIT_PART: u8 = 43;
pub const EDIT_END: u8 = 44;

// Optional diagnostics ride the existing edit transaction; ordinary payloads
// are unchanged. Durations are nested unless the named phase says otherwise.
pub const EDIT_DIAGNOSTIC_VERSION: u64 = 1;
pub const EDIT_DIAGNOSTIC_FIELDS: [&str; 18] = [
    "daemon_control_ns",
    "freeze_gate_ns",
    "kernel_flush_ns",
    "append_ns",
    "facts_ns",
    "retire_ns",
    "lookup_ns",
    "prepare_ns",
    "apply_ns",
    "reconcile_ns",
    "backing_wait_ns",
    "backing_calls",
    "facts_nodes",
    "facts_wire_bytes",
    "cached_nodes_scanned",
    "kernel_flushes",
    "reconcile_notifier",
    "reconcile_cached",
];

#[derive(Clone, Copy)]
#[allow(dead_code)] // Linux-only fields keep identical wire positions on every host.
pub(crate) enum EditMetric {
    Control,
    Gate,
    Kernel,
    Append,
    Facts,
    Retire,
    Lookup,
    Prepare,
    Apply,
    Reconcile,
    BackingWait,
    BackingCalls,
    FactNodes,
    FactBytes,
    CachedNodes,
    KernelFlushes,
    ReconcileNotifier,
    ReconcileCached,
}

pub fn valid_edit_diagnostic_nonce(nonce: &[u8]) -> bool {
    (16..=64).contains(&nonce.len())
        && nonce
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

pub fn read_edit_diagnostic(
    bytes: &[u8],
    nonce: &[u8],
) -> io::Result<[u64; EDIT_DIAGNOSTIC_FIELDS.len()]> {
    let mut input = Input(bytes);
    if !valid_edit_diagnostic_nonce(nonce)
        || input.u64()? != EDIT_DIAGNOSTIC_VERSION
        || input.bytes()? != nonce
    {
        return Err(invalid());
    }
    let mut values = [0; EDIT_DIAGNOSTIC_FIELDS.len()];
    for value in &mut values {
        *value = input.u64()?;
    }
    input.done()?;
    Ok(values)
}

#[cfg(test)]
mod edit_diagnostic_tests {
    use super::*;

    #[test]
    fn edit_diagnostic_rejects_wrong_nonce_version_and_length() {
        let nonce = b"0123456789abcdef";
        let mut bytes = Vec::new();
        u64_out(&mut bytes, EDIT_DIAGNOSTIC_VERSION);
        bytes_out(&mut bytes, nonce).unwrap();
        for index in 0..EDIT_DIAGNOSTIC_FIELDS.len() {
            u64_out(&mut bytes, index as u64);
        }
        let values = read_edit_diagnostic(&bytes, nonce).unwrap();
        assert_eq!(values[EditMetric::ReconcileCached as usize], 17);
        assert!(read_edit_diagnostic(&bytes, b"abcdef0123456789").is_err());
        assert!(read_edit_diagnostic(&bytes[..bytes.len() - 1], nonce).is_err());
        bytes.push(0);
        assert!(read_edit_diagnostic(&bytes, nonce).is_err());
        bytes.pop();
        bytes[7] = 2;
        assert!(read_edit_diagnostic(&bytes, nonce).is_err());
        for invalid in [
            &b"short"[..],
            &b"0123456789abcde\""[..],
            &b"0123456789ABCDEF"[..],
        ] {
            assert!(!valid_edit_diagnostic_nonce(invalid));
        }
    }
}

#[cfg(test)]
mod compact_pending_tests {
    use super::*;
    use layerfs_workspace_core::file_edit::{Piece, PieceTree};
    use std::sync::Arc;

    fn wire_test_backing_ref(id: BackingId) -> BackingRef {
        BackingRef::new(id, ())
    }

    fn wire_test_backing() -> BackingRef {
        wire_test_backing_ref(BackingId(1))
    }

    fn compact_node(inline: bool) -> Node {
        let root = FileContentRoot(ObjectId::for_bytes(b"compact-wire-base"));
        let pieces = if inline {
            PieceTree::base(root, 64)
                .unwrap()
                .replace(
                    4,
                    8,
                    [Piece::Inline {
                        bytes: Arc::from(&b"01234567"[..]),
                        offset: 0,
                        len: 8,
                    }],
                )
                .unwrap()
        } else {
            PieceTree::base(root, 64)
                .unwrap()
                .replace(
                    4,
                    8,
                    [Piece::Spool {
                        segment: wire_test_backing(),
                        offset: 128,
                        len: 8,
                    }],
                )
                .unwrap()
        };
        let mut paths = std::collections::BTreeSet::new();
        paths.insert("workspace/sequence/d0000/f000000".to_string());
        Node {
            revision: 3,
            canonical: None,
            paths,
            mode: 0o600,
            links: 1,
            pins: 0,
            mtime_seconds: 1,
            mtime_nanoseconds: 2,
            data: Data::File(FileData::Edited {
                base: Some((root, 64)),
                spool_high_water: 0,
                edits: 1,
                pieces,
            }),
        }
    }

    fn round_trip(node: &Node) -> Node {
        let bound = node_encoded_bound(node).unwrap();
        let out = node_out(NodeId(7), node).unwrap();
        assert!(
            out.len() <= bound,
            "encoded {} exceeds declared bound {}",
            out.len(),
            bound
        );
        let (id, decoded) = node_in(&out, |id, _, _| Ok(wire_test_backing_ref(id))).unwrap();
        assert_eq!(id, NodeId(7));
        decoded
    }

    /// Comparable rendering of one piece list: physical identity of a spool
    /// segment is its wire identity (id), not the receiver's local handle.
    fn render(pieces: &[Piece]) -> Vec<String> {
        pieces
            .iter()
            .map(|piece| match piece {
                Piece::Base { root, offset, len } => format!("base:{}:{offset}:{len}", root.0),
                Piece::Spool {
                    segment,
                    offset,
                    len,
                } => format!("spool:{}:{offset}:{len}", segment.id().0),
                Piece::Zero { len } => format!("zero:{len}"),
                Piece::Inline { offset, len, .. } => format!("inline:{offset}:{len}"),
            })
            .collect()
    }

    /// Logical identity across the wire: decoded contents, length, inline and
    /// spool payloads and the recorded base all match the sender. The treap
    /// priority serial is not a wire field (unchanged pre-existing behavior).
    fn assert_same_edited(sent: &Node, received: &Node) {
        let (pieces, received_pieces) = match (&sent.data, &received.data) {
            (
                Data::File(FileData::Edited {
                    base,
                    spool_high_water,
                    pieces,
                    edits,
                    ..
                }),
                Data::File(FileData::Edited {
                    base: received_base,
                    spool_high_water: received_high,
                    pieces: received_pieces,
                    edits: received_edits,
                    ..
                }),
            ) => {
                assert_eq!(base, received_base);
                assert_eq!(spool_high_water, received_high);
                assert_eq!(edits, received_edits);
                (pieces, received_pieces)
            }
            _ => panic!("edited file expected"),
        };
        assert_eq!(pieces.len(), received_pieces.len());
        assert_eq!(pieces.count(), received_pieces.count());
        assert_eq!(pieces.inline_len(), received_pieces.inline_len());
        assert_eq!(pieces.spool_len(), received_pieces.spool_len());
        assert_eq!(render(&pieces.pieces()), render(&received_pieces.pieces()));
        assert_eq!(
            pieces.logical_allocation_charge().unwrap(),
            received_pieces.logical_allocation_charge().unwrap()
        );
        assert_eq!(
            render(&pieces.range(0, pieces.len()).unwrap()),
            render(&received_pieces.range(0, pieces.len()).unwrap())
        );
    }

    #[test]
    fn bounded_pending_inline_round_trips_within_its_declared_bound() {
        let node = compact_node(true);
        assert_same_edited(&node, &round_trip(&node));
    }

    #[test]
    fn bounded_pending_spool_round_trips_within_its_declared_bound() {
        let node = compact_node(false);
        assert_same_edited(&node, &round_trip(&node));
    }

    /// The default 96 MiB publication budget must admit the required 32,000
    /// changed files of the exact sequence shape, paths included.
    #[test]
    fn bounded_pending_sequence_set_fits_the_publication_budget() {
        let sample = compact_node(true);
        let bound = node_encoded_bound(&sample).unwrap() as u64;
        let inline = 12u64;
        let charge = (bound - inline) * 8 + 1024;
        assert!(
            32_000 * charge <= MAX_FACT_MEMORY as u64,
            "32,000 compact pending files charge {} bytes",
            32_000 * charge
        );
    }
}
