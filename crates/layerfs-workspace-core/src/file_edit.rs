use crate::{Data, Error, FileData, LiveWorkspace, NodeId, Result};
use layerfs_content::file::content::FileContentRoot;
use std::sync::Arc;

pub const MAX_PIECES_PER_FILE: usize = 8_193;
pub const MAX_INLINE_PER_EDIT: usize = 1024 * 1024;
pub const MAX_INLINE_PER_WORKSPACE: u64 = 8 * 1024 * 1024;
pub const MAX_PIECE_ALLOCATION: u64 = 2 * 1024 * 1024;
pub const MAX_RESULT_BYTES: u64 = 1024 * 1024 * 1024 * 1024;
pub const MAX_LOGICAL_ZERO_BYTES: u64 = 1024 * 1024 * 1024;
pub const MAX_PREDICTED_ZERO_EXTENTS: u64 = 131_072;

/// One prepared inode revision and exact backing range. Consumed once at apply;
/// the adapter retains and charges any unused append if revalidation fails.
#[derive(Debug)]
pub struct PreparedFileEdit {
    node: NodeId,
    expected_revision: u64,
    before: FileData,
    next: FileData,
    appended: u64,
    bytes: usize,
    generations: u64,
}

impl PreparedFileEdit {
    pub fn backing_ranges(&self) -> impl Iterator<Item = SpoolSlice> {
        let pieces = match &self.before {
            FileData::Edited { pieces, .. } => pieces.cursor(),
            FileData::Base { .. } => PieceTree::empty().cursor(),
        };
        pieces.filter_map(|piece| match piece {
            Piece::Spool {
                segment,
                offset,
                len,
            } => Some(SpoolSlice {
                segment,
                offset,
                len,
            }),
            _ => None,
        })
    }
}

impl LiveWorkspace {
    /// Does no I/O and changes no live state. The adapter must hold affected-inode
    /// ordering and resource admission across preparation, acquisition and apply.
    pub fn prepare_write(
        &self,
        node: NodeId,
        offset: u64,
        bytes: usize,
        backing: Option<SpoolSlice>,
    ) -> Result<PreparedFileEdit> {
        self.next_generation()?;
        let expected = self.nodes.get(&node).ok_or(Error::NotFound("node"))?;
        expected
            .revision
            .checked_add(1)
            .ok_or(Error::Integrity("inode revision"))?;
        if bytes == 0 {
            return Err(Error::InvalidInput("empty prepared write"));
        }
        offset
            .checked_add(bytes as u64)
            .ok_or(Error::InvalidInput("write length"))?;
        let Data::File(before) = &expected.data else {
            return Err(Error::InvalidInput("file"));
        };
        let (base, high_water, old, edits) = match before {
            FileData::Base { root, len } => {
                (Some((*root, *len)), 0, PieceTree::base(*root, *len)?, 0)
            }
            FileData::Edited {
                base,
                spool_high_water,
                pieces,
                edits,
            } => (*base, *spool_high_water, pieces.clone(), *edits),
        };
        let appended = if backing.is_some() { bytes as u64 } else { 0 };
        let piece = match backing {
            Some(slice) => {
                if slice.len != bytes as u64 || slice.offset.checked_add(slice.len).is_none() {
                    return Err(Error::InvalidInput("write backing range"));
                }
                Piece::Spool {
                    segment: slice.segment,
                    offset: slice.offset,
                    len: slice.len,
                }
            }
            None => Piece::Zero { len: bytes as u64 },
        };
        let gap = (offset > old.len()).then(|| Piece::Zero {
            len: offset - old.len(),
        });
        let start = offset.min(old.len());
        let delete_len = old.len().saturating_sub(offset).min(bytes as u64);
        let next = old.replace(start, delete_len, gap.into_iter().chain([piece]))?;
        let prepared = PreparedFileEdit {
            node,
            expected_revision: expected.revision,
            before: before.clone(),
            next: FileData::Edited {
                base,
                spool_high_water: high_water
                    .checked_add(appended)
                    .ok_or(Error::InvalidInput("workspace spool limit"))?,
                pieces: next,
                edits: next_edit(edits)?,
            },
            appended,
            bytes,
            generations: 1,
        };
        self.write_resources(&prepared)?;
        Ok(prepared)
    }

    pub fn prepare_truncate(&self, node: NodeId, size: u64) -> Result<Option<PreparedFileEdit>> {
        let expected = self.nodes.get(&node).ok_or(Error::NotFound("node"))?;
        let Data::File(before) = &expected.data else {
            return Err(Error::InvalidInput("file"));
        };
        if expected.attr(node).size == size {
            return Ok(None);
        }
        let (base, high_water, old, edits) = match before {
            FileData::Base { root, len } => {
                (Some((*root, *len)), 0, PieceTree::base(*root, *len)?, 0)
            }
            FileData::Edited {
                base,
                spool_high_water,
                pieces,
                edits,
            } => (*base, *spool_high_water, pieces.clone(), *edits),
        };
        let old_len = old.len();
        self.next_generation()?;
        expected
            .revision
            .checked_add(1)
            .ok_or(Error::Integrity("inode revision"))?;
        let (start, delete_len, replacement) = if size < old_len {
            (size, old_len - size, None)
        } else {
            (
                old_len,
                0,
                Some(Piece::Zero {
                    len: size - old_len,
                }),
            )
        };
        let next = old.replace(start, delete_len, replacement)?;
        // An explicit empty state retires the old logical edit generation only
        // after checking the existing edit budget; old ranges stay retained.
        let edits = next_edit(edits)?;
        let prepared = PreparedFileEdit {
            node,
            expected_revision: expected.revision,
            before: before.clone(),
            next: FileData::Edited {
                base,
                spool_high_water: high_water,
                pieces: next,
                edits: if size == 0 { 0 } else { edits },
            },
            appended: 0,
            bytes: 0,
            generations: 1,
        };
        self.write_resources(&prepared)?;
        Ok(Some(prepared))
    }

    /// Prepare an atomic ordered batch of inline/zero splices without changing the inode.
    pub fn prepare_splices(
        &self,
        node: NodeId,
        replacements: Vec<(u64, u64, Option<Piece>)>,
    ) -> Result<PreparedFileEdit> {
        if replacements.is_empty() {
            return Err(Error::InvalidInput("workspace edit batch"));
        }
        let expected = self.nodes.get(&node).ok_or(Error::NotFound("node"))?;
        let Data::File(before) = &expected.data else {
            return Err(Error::InvalidInput("file"));
        };
        let next = match before {
            FileData::Base { root, len } => FileData::Edited {
                base: Some((*root, *len)),
                spool_high_water: 0,
                pieces: PieceTree::base(*root, *len)?,
                edits: 0,
            },
            edited => edited.clone(),
        };
        let mut prepared = PreparedFileEdit {
            node,
            expected_revision: expected.revision,
            before: before.clone(),
            next,
            appended: 0,
            bytes: 0,
            generations: 0,
        };
        for (start, delete_len, replacement) in replacements {
            self.extend_splices(&mut prepared, start, delete_len, replacement)?;
        }
        Ok(prepared)
    }

    /// Extend a retained batch without retaining overwritten replacement input.
    pub fn extend_splices(
        &self,
        prepared: &mut PreparedFileEdit,
        start: u64,
        delete_len: u64,
        replacement: Option<Piece>,
    ) -> Result<()> {
        let expected = self
            .nodes
            .get(&prepared.node)
            .ok_or(Error::NotFound("node"))?;
        if expected.revision != prepared.expected_revision
            || !matches!(&expected.data, Data::File(data) if *data == prepared.before)
            || prepared.bytes != 0
            || prepared.appended != 0
        {
            return Err(Error::Integrity("stale prepared splice"));
        }
        let generations = prepared
            .generations
            .checked_add(1)
            .ok_or(Error::InvalidInput("workspace edit generation"))?;
        self.mutation_generation
            .checked_add(generations)
            .ok_or(Error::Integrity("Workspace mutation generation"))?;
        expected
            .revision
            .checked_add(1)
            .ok_or(Error::Integrity("inode revision"))?;
        let (old, prior_edits, old_len) = match &prepared.before {
            FileData::Base { len, .. } => (None, 0, *len),
            FileData::Edited { pieces, edits, .. } => (Some(pieces), *edits, pieces.len()),
        };
        let edits = prior_edits
            .checked_add(generations)
            .ok_or(Error::InvalidInput("workspace edit counter"))?;
        match &replacement {
            Some(Piece::Inline { bytes, .. }) if bytes.len() > MAX_INLINE_PER_EDIT => {
                return Err(Error::InvalidInput("workspace inline edit limit"))
            }
            Some(Piece::Base { .. } | Piece::Spool { .. }) => {
                return Err(Error::InvalidInput("workspace edit replacement"))
            }
            _ => {}
        }
        let FileData::Edited {
            pieces,
            edits: next_edits,
            ..
        } = &mut prepared.next
        else {
            return Err(Error::Integrity("prepared splice"));
        };
        let next = pieces.replace(start, delete_len, replacement)?;
        self.check_piece_resources(old, &next)?;
        *next_edits = if old_len != 0 && next.is_empty() {
            0
        } else {
            edits
        };
        *pieces = next;
        prepared.generations = generations;
        Ok(())
    }

    pub fn apply_edit(&mut self, prepared: PreparedFileEdit) -> Result<usize> {
        if !self.nodes.get(&prepared.node).is_some_and(|node| {
            node.revision == prepared.expected_revision
                && matches!(&node.data, Data::File(data) if *data == prepared.before)
        }) {
            return Err(Error::Integrity("stale prepared write"));
        }
        let generation = self
            .mutation_generation
            .checked_add(prepared.generations)
            .ok_or(Error::Integrity("Workspace mutation generation"))?;
        let revision = prepared
            .expected_revision
            .checked_add(1)
            .ok_or(Error::Integrity("inode revision"))?;
        let (inline, allocation, spool) = self.write_resources(&prepared)?;
        let node = self
            .nodes
            .get_mut(&prepared.node)
            .expect("validated prepared inode");
        node.data = Data::File(prepared.next);
        node.revision = revision;
        self.inline_bytes = inline;
        self.piece_allocation_bytes = allocation;
        self.spool_bytes = spool;
        self.spool_bytes_peak = self.spool_bytes_peak.max(spool);
        self.edited_nodes.insert(prepared.node);
        self.dirty.insert(prepared.node);
        self.mutation_generation = generation;
        for path in &node.paths {
            self.mutation_paths.insert(path.clone(), generation);
        }
        Ok(prepared.bytes)
    }

    fn write_resources(&self, prepared: &PreparedFileEdit) -> Result<(u64, u64, u64)> {
        let old = match &prepared.before {
            FileData::Edited { pieces, .. } => Some(pieces),
            _ => None,
        };
        let FileData::Edited { pieces: next, .. } = &prepared.next else {
            unreachable!("prepared write is edited data")
        };
        let (inline, allocation) = self.check_piece_resources(old, next)?;
        let spool = self
            .spool_bytes
            .checked_add(prepared.appended)
            .ok_or(Error::InvalidInput("workspace spool limit"))?;
        // Truncate keeps the existing payload charge and may proceed after a
        // lowered spool limit; writes retain their existing quota check.
        if prepared.bytes != 0 {
            self.policy.check(spool)?;
        }
        Ok((inline, allocation, spool))
    }

    pub fn check_piece_resources(
        &self,
        old: Option<&PieceTree>,
        next: &PieceTree,
    ) -> Result<(u64, u64)> {
        let inline = self
            .inline_bytes
            .checked_sub(old.map_or(0, PieceTree::inline_len))
            .and_then(|v| v.checked_add(next.inline_len()))
            .filter(|v| *v <= MAX_INLINE_PER_WORKSPACE)
            .ok_or(Error::InvalidInput("workspace inline limit"))?;
        let allocation = self
            .piece_allocation_bytes
            .checked_sub(
                old.map(PieceTree::logical_allocation_charge)
                    .transpose()?
                    .unwrap_or(0),
            )
            .and_then(|v| v.checked_add(next.logical_allocation_charge().ok()?))
            .filter(|v| *v <= MAX_PIECE_ALLOCATION)
            .ok_or(Error::InvalidInput("workspace piece allocation limit"))?;
        Ok((inline, allocation))
    }
}

pub fn next_edit(edits: u64) -> Result<u64> {
    // The cumulative pre-Commit edit count is informational, not a budget: the
    // real bounds are the per-file piece count and the pending-workspace piece,
    // inline and spool charges. Only genuine counter overflow is rejected.
    edits
        .checked_add(1)
        .ok_or(Error::InvalidInput("workspace edit counter"))
}

pub fn check_logical_allocation_charge(bytes: u64) -> Result<()> {
    if bytes <= MAX_PIECE_ALLOCATION {
        Ok(())
    } else {
        Err(Error::InvalidInput("workspace piece allocation limit"))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Piece {
    Base {
        root: FileContentRoot,
        offset: u64,
        len: u64,
    },
    Inline {
        bytes: Arc<[u8]>,
        offset: u64,
        len: u64,
    },
    Zero {
        len: u64,
    },
    Spool {
        segment: crate::backing::BackingRef,
        offset: u64,
        len: u64,
    },
}

impl Piece {
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn len(&self) -> u64 {
        match self {
            Self::Base { len, .. }
            | Self::Inline { len, .. }
            | Self::Zero { len }
            | Self::Spool { len, .. } => *len,
        }
    }

    fn slice(&self, start: u64, len: u64) -> Result<Self> {
        if len == 0 || start.checked_add(len).is_none_or(|end| end > self.len()) {
            return Err(Error::Integrity("piece slice"));
        }
        Ok(match self {
            Self::Base { root, offset, .. } => Self::Base {
                root: *root,
                offset: offset + start,
                len,
            },
            Self::Inline { bytes, offset, .. } => Self::Inline {
                bytes: bytes.clone(),
                offset: offset + start,
                len,
            },
            Self::Zero { .. } => Self::Zero { len },
            Self::Spool {
                segment, offset, ..
            } => Self::Spool {
                segment: segment.clone(),
                offset: offset + start,
                len,
            },
        })
    }

    pub fn inline_len(&self) -> u64 {
        matches!(self, Self::Inline { .. })
            .then(|| self.len())
            .unwrap_or(0)
    }
}

type Link = Option<Arc<PieceNode>>;

#[derive(Clone, Debug, Eq, PartialEq)]
struct PieceNode {
    piece: Piece,
    priority: u64,
    left: Link,
    right: Link,
    len: u64,
    count: usize,
    inline_len: u64,
    zero_len: u64,
    spool_len: u64,
    height: usize,
}

impl PieceNode {
    fn new(piece: Piece, priority: u64, left: Link, right: Link) -> Result<Arc<Self>> {
        let len = link_len(&left)
            .checked_add(piece.len())
            .and_then(|len| len.checked_add(link_len(&right)))
            .ok_or(Error::InvalidInput("file length"))?;
        let count = link_count(&left)
            .checked_add(1)
            .and_then(|count| count.checked_add(link_count(&right)))
            .ok_or(Error::InvalidInput("piece count"))?;
        let inline_len = link_inline_len(&left)
            .checked_add(piece.inline_len())
            .and_then(|len| len.checked_add(link_inline_len(&right)))
            .ok_or(Error::InvalidInput("inline bytes"))?;
        let zero_len = link_zero_len(&left)
            .checked_add(
                matches!(piece, Piece::Zero { .. })
                    .then(|| piece.len())
                    .unwrap_or(0),
            )
            .and_then(|len| len.checked_add(link_zero_len(&right)))
            .ok_or(Error::InvalidInput("logical zero bytes"))?;
        let spool_len = link_spool_len(&left)
            .checked_add(
                matches!(piece, Piece::Spool { .. })
                    .then(|| piece.len())
                    .unwrap_or(0),
            )
            .and_then(|len| len.checked_add(link_spool_len(&right)))
            .ok_or(Error::InvalidInput("spool bytes"))?;
        let height = 1 + link_height(&left).max(link_height(&right));
        Ok(Arc::new(Self {
            piece,
            priority,
            left,
            right,
            len,
            count,
            inline_len,
            zero_len,
            spool_len,
            height,
        }))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpoolSlice {
    pub segment: crate::backing::BackingRef,
    pub offset: u64,
    pub len: u64,
}

/// Bounded compact pending state for the common shape "one equal-length
/// overwrite of a committed base file with inline replacement bytes". It
/// replaces the three treap nodes (left base, splice, right base) that the same
/// shape otherwise needs: the base content root describes both remainders, and
/// the replacement length equals the replaced length, so one length serves the
/// base and the result. The 64-byte descriptor is charged at exactly that size;
/// the replacement bytes stay charged through the workspace inline budget.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactSplice {
    base: FileContentRoot,
    len: u64,
    offset: u64,
    bytes: Arc<[u8]>,
}

/// The same bounded shape when the replacement is an already admitted spool
/// slice rather than resident inline bytes. Charged at exactly its own size.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactSpoolSplice {
    base: FileContentRoot,
    len: u64,
    offset: u64,
    slice: SpoolSlice,
}

impl CompactSplice {
    fn fits(&self) -> bool {
        let replace = self.bytes.len() as u64;
        replace != 0
            && self
                .offset
                .checked_add(replace)
                .is_some_and(|end| end <= self.len)
    }

    fn pieces(&self) -> Vec<Piece> {
        splice_pieces(
            Piece::Base {
                root: self.base,
                offset: 0,
                len: self.len,
            },
            self.offset,
            Piece::Inline {
                bytes: self.bytes.clone(),
                offset: 0,
                len: self.bytes.len() as u64,
            },
        )
    }

    fn charge(&self) -> u64 {
        std::mem::size_of::<Self>() as u64
    }
}

impl CompactSpoolSplice {
    fn fits(&self) -> bool {
        self.slice.len != 0
            && self
                .offset
                .checked_add(self.slice.len)
                .is_some_and(|end| end <= self.len)
    }

    fn pieces(&self) -> Vec<Piece> {
        splice_pieces(
            Piece::Base {
                root: self.base,
                offset: 0,
                len: self.len,
            },
            self.offset,
            Piece::Spool {
                segment: self.slice.segment.clone(),
                offset: self.slice.offset,
                len: self.slice.len,
            },
        )
    }

    fn charge(&self) -> u64 {
        std::mem::size_of::<Self>() as u64
    }
}

/// Logical pieces of one bounded splice: the base remainder before the
/// replacement, the replacement, and the base remainder after it. Empty
/// remainders are omitted, so the logical piece count is 1..=3.
fn splice_pieces(base: Piece, offset: u64, replacement: Piece) -> Vec<Piece> {
    let base_len = base.len();
    let mut pieces = Vec::with_capacity(3);
    if offset != 0 {
        pieces.push(base.slice(0, offset).expect("validated splice lead"));
    }
    let replace = replacement.len();
    pieces.push(replacement);
    let consumed = offset + replace;
    if consumed < base_len {
        pieces.push(
            base.slice(consumed, base_len - consumed)
                .expect("validated splice trail"),
        );
    }
    pieces
}

/// One candidate bounded compact edit, before it is charged and installed.
enum BoundedEdit {
    Inline(CompactSplice),
    Spool(CompactSpoolSplice),
}

impl BoundedEdit {
    fn fits(&self) -> bool {
        match self {
            Self::Inline(splice) => splice.fits(),
            Self::Spool(splice) => splice.fits(),
        }
    }

    fn into_tree(self, source: &PieceTree) -> Result<PieceTree> {
        if !self.fits() {
            return Err(Error::Integrity("bounded splice"));
        }
        // The descriptor replaces the single base node entirely: the base root
        // and length it carries describe both remainders, so no node is retained.
        let mut next = source.clone();
        next.root = None;
        next.compact_spool = None;
        match self {
            Self::Inline(splice) => next.compact_splice = Some(Box::new(splice)),
            Self::Spool(splice) => next.compact_spool_splice = Some(Box::new(splice)),
        }
        Ok(next)
    }
}

/// Read-only wire view of the bounded pending form.
pub enum CompactPending<'a> {
    Inline {
        base: FileContentRoot,
        len: u64,
        offset: u64,
        bytes: &'a [u8],
    },
    Spool {
        base: FileContentRoot,
        len: u64,
        offset: u64,
        slice: &'a SpoolSlice,
    },
}

/// Read-only view over whichever bounded compact form a tree currently holds.
#[derive(Clone, Copy)]
enum CompactView<'a> {
    Inline(&'a CompactSplice),
    Spool(&'a CompactSpoolSplice),
}

impl<'a> CompactView<'a> {
    fn len(&self) -> u64 {
        match self {
            Self::Inline(splice) => splice.len,
            Self::Spool(splice) => splice.len,
        }
    }

    fn offset(&self) -> u64 {
        match self {
            Self::Inline(splice) => splice.offset,
            Self::Spool(splice) => splice.offset,
        }
    }

    fn replacement_len(&self) -> u64 {
        match self {
            Self::Inline(splice) => splice.bytes.len() as u64,
            Self::Spool(splice) => splice.slice.len,
        }
    }

    fn charge(&self) -> u64 {
        match self {
            Self::Inline(splice) => splice.charge(),
            Self::Spool(splice) => splice.charge(),
        }
    }

    fn pieces(&self) -> Vec<Piece> {
        match self {
            Self::Inline(splice) => splice.pieces(),
            Self::Spool(splice) => splice.pieces(),
        }
    }

    /// Bounded contribution of this form to the encoded wire node: the envelope
    /// base root and length, the spool high-water mark and edit counter, one
    /// splice tag, the splice offset and its payload.
    fn encoded_bound(&self, inline_len: u64) -> u64 {
        let payload = match self {
            Self::Inline(_) => 1 + 4 + inline_len,
            Self::Spool(_) => 1 + 8 + 8 + 8,
        };
        1 + 32 + 8 + 8 + 8 + 8 + payload
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PieceTree {
    root: Link,
    serial: u64,
    // A compact physical-range handle replaces per-file path/descriptor ownership.
    // Its one-word logical piece charge preserves the existing dense-file budget.
    compact_spool: Option<Arc<SpoolSlice>>,
    // Bounded compact pending forms for one equal-length overwrite. At most one
    // is set, and both are boxed so an unedited node carries only the slot.
    compact_splice: Option<Box<CompactSplice>>,
    compact_spool_splice: Option<Box<CompactSpoolSplice>>,
}

/// Owned in-order traversal with at most one tree path and three compact pieces
/// resident. A later live edit cannot change this cursor's ranges or backing.
pub struct PieceCursor {
    stack: Vec<Arc<PieceNode>>,
    compact: std::vec::IntoIter<Piece>,
}

impl PieceCursor {
    fn descend(&mut self, mut node: Link) {
        while let Some(current) = node {
            node = current.left.clone();
            self.stack.push(current);
        }
    }
}

impl Iterator for PieceCursor {
    type Item = Piece;

    fn next(&mut self) -> Option<Piece> {
        if let Some(piece) = self.compact.next() {
            return Some(piece);
        }
        let node = self.stack.pop()?;
        self.descend(node.right.clone());
        Some(node.piece.clone())
    }
}

impl PieceTree {
    pub fn cursor(&self) -> PieceCursor {
        let compact = if let Some(compact) = self.compact() {
            compact.pieces()
        } else if let Some(slice) = &self.compact_spool {
            vec![Piece::Spool {
                segment: slice.segment.clone(),
                offset: slice.offset,
                len: slice.len,
            }]
        } else {
            Vec::new()
        };
        let mut cursor = PieceCursor {
            stack: Vec::new(),
            compact: compact.into_iter(),
        };
        cursor.descend(self.root.clone());
        cursor
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn empty() -> Self {
        Self {
            root: None,
            serial: 0,
            compact_spool: None,
            compact_splice: None,
            compact_spool_splice: None,
        }
    }

    /// Whichever bounded compact splice form this tree currently holds.
    fn compact(&self) -> Option<CompactView<'_>> {
        match (&self.compact_splice, &self.compact_spool_splice) {
            (Some(splice), None) => Some(CompactView::Inline(splice)),
            (None, Some(splice)) => Some(CompactView::Spool(splice)),
            _ => None,
        }
    }

    /// Encoded wire bound of the compact form, or the rooted piece-list bound.
    /// Returns `None` for a rooted tree so the caller keeps its own formula.
    pub fn compact_encoded_bound(&self) -> Option<u64> {
        let compact = self.compact()?;
        let inline = self.inline_len();
        Some(compact.encoded_bound(inline))
    }

    /// Bounded pending view for the wire, or `None` for any rooted tree.
    pub fn compact_pending(&self) -> Option<CompactPending<'_>> {
        match self.compact()? {
            CompactView::Inline(splice) => Some(CompactPending::Inline {
                base: splice.base,
                len: splice.len,
                offset: splice.offset,
                bytes: &splice.bytes,
            }),
            CompactView::Spool(splice) => Some(CompactPending::Spool {
                base: splice.base,
                len: splice.len,
                offset: splice.offset,
                slice: &splice.slice,
            }),
        }
    }

    /// Rebuild the bounded inline form from an authenticated wire node.
    pub fn compact_inline(
        base: FileContentRoot,
        len: u64,
        offset: u64,
        bytes: Arc<[u8]>,
    ) -> Result<Self> {
        let splice = CompactSplice {
            base,
            len,
            offset,
            bytes,
        };
        if !splice.fits() {
            return Err(Error::InvalidInput("bounded splice"));
        }
        let mut tree = Self::empty();
        tree.compact_splice = Some(Box::new(splice));
        check_logical_allocation_charge(tree.logical_allocation_charge()?)?;
        Ok(tree)
    }

    /// Rebuild the bounded spool form from an authenticated wire node.
    pub fn compact_spool_splice(
        base: FileContentRoot,
        len: u64,
        offset: u64,
        slice: SpoolSlice,
    ) -> Result<Self> {
        let splice = CompactSpoolSplice {
            base,
            len,
            offset,
            slice,
        };
        if !splice.fits() {
            return Err(Error::InvalidInput("bounded splice"));
        }
        let mut tree = Self::empty();
        tree.compact_spool_splice = Some(Box::new(splice));
        check_logical_allocation_charge(tree.logical_allocation_charge()?)?;
        Ok(tree)
    }

    pub fn base(root: FileContentRoot, len: u64) -> Result<Self> {
        let mut tree = Self::empty();
        if len != 0 {
            let priority = tree.priority()?;
            tree.root = Some(PieceNode::new(
                Piece::Base {
                    root,
                    offset: 0,
                    len,
                },
                priority,
                None,
                None,
            )?);
        }
        Ok(tree)
    }

    pub fn compact_spool(&self) -> Option<&SpoolSlice> {
        self.compact_spool.as_deref()
    }

    fn compact_len(&self) -> u64 {
        self.compact_spool.as_ref().map_or(0, |slice| slice.len)
    }

    pub fn len(&self) -> u64 {
        self.compact().map_or(0, |compact| compact.len())
            + self.compact_len()
            + link_len(&self.root)
    }

    pub fn count(&self) -> usize {
        let compact = self.compact().map_or(0, |compact| {
            let consumed = compact.offset() + compact.replacement_len();
            1 + usize::from(compact.offset() != 0) + usize::from(consumed < compact.len())
        });
        compact + usize::from(self.compact_len() != 0) + link_count(&self.root)
    }

    pub fn inline_len(&self) -> u64 {
        let compact = match self.compact() {
            Some(CompactView::Inline(splice)) => splice.bytes.len() as u64,
            _ => 0,
        };
        compact + link_inline_len(&self.root)
    }

    pub fn height(&self) -> usize {
        let compact = self
            .compact()
            .map_or(0, |compact| if compact.offset() == 0 { 2 } else { 3 });
        compact
            .max(usize::from(self.compact_len() != 0))
            .max(link_height(&self.root))
    }

    pub fn spool_len(&self) -> u64 {
        let compact = match self.compact() {
            Some(CompactView::Spool(splice)) => splice.slice.len,
            _ => 0,
        };
        compact + self.compact_len() + link_spool_len(&self.root)
    }

    pub fn logical_allocation_charge(&self) -> Result<u64> {
        if let Some(compact) = self.compact() {
            return Ok(compact.charge());
        }
        if self.compact_len() != 0 {
            return Ok(std::mem::size_of::<u64>() as u64);
        }
        (self.count() as u64)
            .checked_mul(std::mem::size_of::<PieceNode>() as u64)
            .ok_or(Error::InvalidInput("piece allocation charge"))
    }

    /// The whole content is one base range, so a bounded splice can be described
    /// without a node. `None` for any other tree shape.
    fn single_base(&self) -> Option<(FileContentRoot, u64)> {
        if self.compact_spool.is_some() || self.compact().is_some() {
            return None;
        }
        let node = self.root.as_ref()?;
        if node.left.is_some() || node.right.is_some() {
            return None;
        }
        match &node.piece {
            Piece::Base {
                root,
                offset: 0,
                len,
            } => Some((*root, *len)),
            _ => None,
        }
    }

    /// Materialize the bounded splice into ordinary pieces so a more complex
    /// edit keeps the existing tree representation. Returns the same tree when
    /// no bounded form is set.
    fn materialized(&self) -> Result<Self> {
        if self.compact().is_none() {
            return Ok(self.clone());
        }
        let pieces = self.compact().expect("compact form").pieces();
        let mut next = self.clone();
        next.compact_splice = None;
        next.compact_spool_splice = None;
        let mut root = None;
        for piece in pieces {
            let priority = next.priority()?;
            let node = Some(PieceNode::new(piece, priority, None, None)?);
            root = merge(&root, &node)?;
        }
        next.root = root;
        Ok(next)
    }

    pub fn replace(
        &self,
        start: u64,
        delete_len: u64,
        replacement: impl IntoIterator<Item = Piece>,
    ) -> Result<Self> {
        let end = start
            .checked_add(delete_len)
            .ok_or(Error::InvalidInput("file range"))?;
        if end > self.len() {
            return Err(Error::InvalidInput("file range"));
        }
        let mut replacement = replacement.into_iter();
        let first = replacement.next();
        let mut replacement = replacement.peekable();
        // Bounded compact form for one equal-length overwrite of a committed base
        // file: exactly the information three treap nodes carry (left base,
        // replacement, right base), described by one bounded descriptor. Any
        // other shape, and every further edit of an already compact file, keeps
        // the existing tree, so the growing-file representation is unchanged.
        if replacement.peek().is_none() {
            if let Some((base, base_len)) = self.single_base() {
                if end <= base_len {
                    let bounded = match &first {
                        Some(Piece::Inline { bytes, offset, len })
                            if *offset == 0 && *len == bytes.len() as u64 && *len == delete_len =>
                        {
                            Some(BoundedEdit::Inline(CompactSplice {
                                base,
                                len: base_len,
                                offset: start,
                                bytes: bytes.clone(),
                            }))
                        }
                        Some(Piece::Spool {
                            segment,
                            offset,
                            len,
                        }) if *len == delete_len => Some(BoundedEdit::Spool(CompactSpoolSplice {
                            base,
                            len: base_len,
                            offset: start,
                            slice: SpoolSlice {
                                segment: segment.clone(),
                                offset: *offset,
                                len: *len,
                            },
                        })),
                        _ => None,
                    };
                    if let Some(bounded) = bounded {
                        let next = bounded.into_tree(self)?;
                        check_logical_allocation_charge(next.logical_allocation_charge()?)?;
                        if next.count() > MAX_PIECES_PER_FILE || next.len() > MAX_RESULT_BYTES {
                            return Err(Error::InvalidInput("workspace piece limit"));
                        }
                        return Ok(next);
                    }
                }
            }
        }
        // A compact tree is materialized before any other edit, so the tree path
        // below stays the single rooted implementation.
        let materialized = self.materialized()?;
        if self.compact().is_none()
            && self.root.is_none()
            && start == self.compact_len()
            && delete_len == 0
        {
            if let Some(Piece::Spool {
                segment,
                offset,
                len,
            }) = &first
            {
                if replacement.peek().is_none()
                    && self.compact_spool.as_ref().is_none_or(|slice| {
                        slice.segment == *segment && slice.offset + slice.len == *offset
                    })
                {
                    let len = start
                        .checked_add(*len)
                        .filter(|len| *len <= MAX_RESULT_BYTES)
                        .ok_or(Error::InvalidInput("workspace piece limit"))?;
                    let mut next = self.clone();
                    next.compact_spool = Some(Arc::new(SpoolSlice {
                        segment: segment.clone(),
                        offset: offset - start,
                        len,
                    }));
                    check_logical_allocation_charge(next.logical_allocation_charge()?)?;
                    return Ok(next);
                }
            }
        }
        let mut next = materialized;
        if let Some(slice) = next.compact_spool.take() {
            let piece = Piece::Spool {
                segment: slice.segment.clone(),
                offset: slice.offset,
                len: slice.len,
            };
            let priority = next.priority()?;
            next.root = Some(PieceNode::new(piece, priority, None, None)?);
        }
        let root = next.root.clone();
        let (left, tail) = split(&root, start, &mut next)?;
        let (_, right) = split(&tail, delete_len, &mut next)?;
        let mut middle = None;
        for piece in first.into_iter().chain(replacement) {
            if piece.is_empty() {
                continue;
            }
            let priority = next.priority()?;
            let node = Some(PieceNode::new(piece, priority, None, None)?);
            middle = merge(&middle, &node)?;
        }
        next.root = merge(&merge(&left, &middle)?, &right)?;
        if let Some(node) = &next.root {
            if node.count == 1 {
                if let Piece::Spool {
                    segment,
                    offset,
                    len,
                } = &node.piece
                {
                    next.compact_spool = Some(Arc::new(SpoolSlice {
                        segment: segment.clone(),
                        offset: *offset,
                        len: *len,
                    }));
                    next.root = None;
                }
            }
        }
        let predicted_zero_extents = link_zero_len(&next.root).div_ceil(8_192);
        if next.count() > MAX_PIECES_PER_FILE
            || check_logical_allocation_charge(next.logical_allocation_charge()?).is_err()
            || next.len() > MAX_RESULT_BYTES
            || link_zero_len(&next.root) > MAX_LOGICAL_ZERO_BYTES
            || predicted_zero_extents > MAX_PREDICTED_ZERO_EXTENTS
        {
            return Err(Error::InvalidInput("workspace piece limit"));
        }
        Ok(next)
    }

    pub fn pieces(&self) -> Vec<Piece> {
        if let Some(compact) = self.compact() {
            return compact.pieces();
        }
        if let Some(slice) = &self.compact_spool {
            return vec![Piece::Spool {
                segment: slice.segment.clone(),
                offset: slice.offset,
                len: slice.len,
            }];
        }
        let mut output = Vec::with_capacity(self.count());
        visit(&self.root, &mut |piece, _| output.push(piece.clone()));
        output
    }

    pub fn range(&self, start: u64, end: u64) -> Result<Vec<Piece>> {
        self.range_with_visits(start, end).map(|(pieces, _)| pieces)
    }

    pub fn range_with_visits(&self, start: u64, end: u64) -> Result<(Vec<Piece>, usize)> {
        if start > end || end > self.len() {
            return Err(Error::InvalidInput("file range"));
        }
        if let Some(compact) = self.compact() {
            let mut output = Vec::new();
            let mut visited = 0;
            let mut offset = 0;
            for piece in compact.pieces() {
                let piece_end = offset + piece.len();
                if piece_end > start && offset < end {
                    visited += 1;
                    let local_start = start.saturating_sub(offset);
                    let local_end = (end - offset).min(piece.len());
                    output.push(piece.slice(local_start, local_end - local_start)?);
                }
                offset = piece_end;
            }
            return Ok((output, visited));
        }
        if let Some(slice) = &self.compact_spool {
            let pieces = if start == end {
                Vec::new()
            } else {
                vec![Piece::Spool {
                    segment: slice.segment.clone(),
                    offset: slice.offset + start,
                    len: end - start,
                }]
            };
            let visited = usize::from(start != end);
            return Ok((pieces, visited));
        }
        let mut output = Vec::new();
        let mut visited = 0;
        visit_range(
            &self.root,
            0,
            start,
            end,
            &mut visited,
            &mut |piece, offset| {
                let piece_end = offset + piece.len();
                let local_start = start.saturating_sub(offset);
                let local_end = (end - offset).min(piece.len());
                debug_assert!(piece_end > start && offset < end);
                output.push(
                    piece
                        .slice(local_start, local_end - local_start)
                        .expect("validated piece overlap"),
                );
            },
        );
        Ok((output, visited))
    }

    fn priority(&mut self) -> Result<u64> {
        self.serial = self
            .serial
            .checked_add(1)
            .ok_or(Error::InvalidInput("piece serial"))?;
        let mut value = self.serial.wrapping_add(0x9e37_79b9_7f4a_7c15);
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        Ok(value ^ (value >> 31))
    }
}

fn link_len(link: &Link) -> u64 {
    link.as_ref().map_or(0, |node| node.len)
}

fn link_count(link: &Link) -> usize {
    link.as_ref().map_or(0, |node| node.count)
}

fn link_inline_len(link: &Link) -> u64 {
    link.as_ref().map_or(0, |node| node.inline_len)
}

fn link_zero_len(link: &Link) -> u64 {
    link.as_ref().map_or(0, |node| node.zero_len)
}

fn link_spool_len(link: &Link) -> u64 {
    link.as_ref().map_or(0, |node| node.spool_len)
}

fn link_height(link: &Link) -> usize {
    link.as_ref().map_or(0, |node| node.height)
}

fn merge(left: &Link, right: &Link) -> Result<Link> {
    match (left, right) {
        (None, _) => Ok(right.clone()),
        (_, None) => Ok(left.clone()),
        (Some(left), Some(right)) if left.priority >= right.priority => Ok(Some(PieceNode::new(
            left.piece.clone(),
            left.priority,
            left.left.clone(),
            merge(&left.right, &Some(right.clone()))?,
        )?)),
        (Some(left), Some(right)) => Ok(Some(PieceNode::new(
            right.piece.clone(),
            right.priority,
            merge(&Some(left.clone()), &right.left)?,
            right.right.clone(),
        )?)),
    }
}

fn split(root: &Link, offset: u64, tree: &mut PieceTree) -> Result<(Link, Link)> {
    let Some(node) = root else {
        return if offset == 0 {
            Ok((None, None))
        } else {
            Err(Error::InvalidInput("piece split"))
        };
    };
    let left_len = link_len(&node.left);
    let piece_end = left_len
        .checked_add(node.piece.len())
        .ok_or(Error::InvalidInput("piece split"))?;
    if offset < left_len {
        let (left, middle) = split(&node.left, offset, tree)?;
        return Ok((
            left,
            Some(PieceNode::new(
                node.piece.clone(),
                node.priority,
                middle,
                node.right.clone(),
            )?),
        ));
    }
    if offset > piece_end {
        let (middle, right) = split(&node.right, offset - piece_end, tree)?;
        return Ok((
            Some(PieceNode::new(
                node.piece.clone(),
                node.priority,
                node.left.clone(),
                middle,
            )?),
            right,
        ));
    }
    if offset == left_len {
        let right = Some(PieceNode::new(
            node.piece.clone(),
            node.priority,
            None,
            node.right.clone(),
        )?);
        return Ok((node.left.clone(), right));
    }
    if offset == piece_end {
        let left = Some(PieceNode::new(
            node.piece.clone(),
            node.priority,
            node.left.clone(),
            None,
        )?);
        return Ok((left, node.right.clone()));
    }
    let local = offset - left_len;
    let left_piece = node.piece.slice(0, local)?;
    let right_piece = node.piece.slice(local, node.piece.len() - local)?;
    let left_priority = tree.priority()?;
    let right_priority = tree.priority()?;
    let left = merge(
        &node.left,
        &Some(PieceNode::new(left_piece, left_priority, None, None)?),
    )?;
    let right = merge(
        &Some(PieceNode::new(right_piece, right_priority, None, None)?),
        &node.right,
    )?;
    Ok((left, right))
}

fn visit(root: &Link, visitor: &mut impl FnMut(&Piece, u64)) {
    fn inner(root: &Link, base: u64, visitor: &mut impl FnMut(&Piece, u64)) {
        let Some(node) = root else { return };
        inner(&node.left, base, visitor);
        let offset = base + link_len(&node.left);
        visitor(&node.piece, offset);
        inner(&node.right, offset + node.piece.len(), visitor);
    }
    inner(root, 0, visitor);
}

fn visit_range(
    root: &Link,
    base: u64,
    start: u64,
    end: u64,
    visited: &mut usize,
    visitor: &mut impl FnMut(&Piece, u64),
) {
    let Some(node) = root else { return };
    if base >= end || base + node.len <= start {
        return;
    }
    *visited += 1;
    visit_range(&node.left, base, start, end, visited, visitor);
    let offset = base + link_len(&node.left);
    if offset < end && offset + node.piece.len() > start {
        visitor(&node.piece, offset);
    }
    visit_range(
        &node.right,
        offset + node.piece.len(),
        start,
        end,
        visited,
        visitor,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use layerfs_content::ObjectId;

    #[test]
    fn piece_cursor_retains_old_ranges_with_only_a_tree_path_resident() {
        let mut tree = PieceTree::empty();
        for n in 0..513_u64 {
            let piece = if n % 2 == 0 {
                Piece::Spool {
                    segment: crate::backing::test_backing(),
                    offset: n * 3,
                    len: 3,
                }
            } else {
                Piece::Zero { len: 3 }
            };
            tree = tree.replace(tree.len(), 0, [piece]).unwrap();
        }
        let height = tree.height();
        let mut held = tree.cursor();
        tree = tree
            .replace(0, tree.len(), [Piece::Zero { len: 1 }])
            .unwrap();
        let mut seen = 0_u64;
        while let Some(piece) = held.next() {
            assert!(held.stack.len() <= height);
            match piece {
                Piece::Spool { offset, len, .. } => {
                    assert_eq!(seen % 2, 0);
                    assert_eq!((offset, len), (seen * 3, 3));
                }
                Piece::Zero { len } => {
                    assert_eq!(seen % 2, 1);
                    assert_eq!(len, 3);
                }
                _ => panic!("unexpected range"),
            }
            seen += 1;
        }
        assert_eq!(seen, 513);
        assert_eq!(
            tree.cursor().collect::<Vec<_>>(),
            vec![Piece::Zero { len: 1 }]
        );
        let compact = PieceTree::compact_inline(
            FileContentRoot(ObjectId::for_bytes(b"base")),
            20,
            5,
            Arc::from(&b"abc"[..]),
        )
        .unwrap();
        assert_eq!(
            compact.cursor().map(|p| p.len()).collect::<Vec<_>>(),
            vec![5, 3, 12]
        );
        let spool = PieceTree::empty()
            .replace(
                0,
                0,
                [Piece::Spool {
                    segment: crate::backing::test_backing(),
                    offset: 8,
                    len: 10,
                }],
            )
            .unwrap();
        assert_eq!(
            spool.cursor().map(|p| p.len()).collect::<Vec<_>>(),
            vec![10]
        );
        assert!(PieceTree::empty().cursor().next().is_none());
    }

    #[test]
    fn contiguous_spool_records_fit_workspace_budget_and_preserve_edit_limits() {
        let mut charge = 0;
        let files = (0..100_000)
            .map(|index| {
                let len = [1_024, 8_192, 49_152][index % 3];
                let tree = PieceTree::empty()
                    .replace(
                        0,
                        0,
                        [Piece::Spool {
                            segment: crate::backing::test_backing(),
                            offset: 0,
                            len,
                        }],
                    )
                    .unwrap();
                assert!(tree.root.is_none(), "contiguous data allocated a tree node");
                assert_eq!(tree.count(), 1);
                assert_eq!(tree.len(), len);
                assert_eq!(tree.spool_len(), len);
                charge += tree.logical_allocation_charge().unwrap();
                check_logical_allocation_charge(charge).unwrap();
                tree
            })
            .collect::<Vec<_>>();
        assert_eq!(charge, 800_000);
        let original = &files[1];
        assert_eq!(
            original.range(98, 106).unwrap(),
            vec![Piece::Spool {
                segment: crate::backing::test_backing(),
                offset: 98,
                len: 8
            }]
        );
        let edited = original
            .replace(
                100,
                4,
                [Piece::Inline {
                    bytes: Arc::from(&b"edit"[..]),
                    offset: 0,
                    len: 4,
                }],
            )
            .unwrap();
        assert!(edited.compact_spool.is_none());
        assert_eq!(edited.count(), 3);
        assert_eq!(edited.len(), original.len());
        assert_eq!(edited.inline_len(), 4);
        assert_eq!(edited.spool_len(), original.len() - 4);
        assert_eq!(
            edited.logical_allocation_charge().unwrap(),
            3 * std::mem::size_of::<PieceNode>() as u64
        );
        assert_eq!(
            edited.range(98, 106).unwrap(),
            vec![
                Piece::Spool {
                    segment: crate::backing::test_backing(),
                    offset: 98,
                    len: 2
                },
                Piece::Inline {
                    bytes: Arc::from(&b"edit"[..]),
                    offset: 0,
                    len: 4
                },
                Piece::Spool {
                    segment: crate::backing::test_backing(),
                    offset: 104,
                    len: 2
                },
            ]
        );
        assert!(
            original.root.is_none(),
            "editing changed the original snapshot"
        );
        let truncated = original.replace(1_024, original.len() - 1_024, []).unwrap();
        assert_eq!(
            truncated.pieces(),
            vec![Piece::Spool {
                segment: crate::backing::test_backing(),
                offset: 0,
                len: 1_024
            }]
        );
        assert_eq!(truncated.logical_allocation_charge().unwrap(), 8);
        let empty = truncated.replace(0, 1_024, []).unwrap();
        assert_eq!(empty.logical_allocation_charge().unwrap(), 0);
        assert!(empty.pieces().is_empty());
        assert!(original.replace(original.len(), 1, []).is_err());
        assert!(PieceTree::empty()
            .replace(
                0,
                0,
                [Piece::Spool {
                    segment: crate::backing::test_backing(),
                    offset: 0,
                    len: MAX_RESULT_BYTES + 1
                }]
            )
            .is_err());
        assert!(original
            .replace(
                0,
                0,
                [Piece::Zero {
                    len: MAX_LOGICAL_ZERO_BYTES + 1
                }]
            )
            .is_err());
        check_logical_allocation_charge(MAX_PIECE_ALLOCATION).unwrap();
        assert!(check_logical_allocation_charge(MAX_PIECE_ALLOCATION + 1).is_err());
        let node_charge = edited.logical_allocation_charge().unwrap();
        let remaining = MAX_PIECE_ALLOCATION - charge;
        let accepted = charge + remaining / node_charge * node_charge;
        check_logical_allocation_charge(accepted).unwrap();
        assert!(check_logical_allocation_charge(accepted + node_charge).is_err());
    }

    #[test]
    fn sequential_spool_appends_stay_inline_and_fragmented_writes_fall_back() {
        let mut tree = PieceTree::empty();
        for index in 0..500 {
            tree = tree
                .replace(
                    index * 1024,
                    0,
                    [Piece::Spool {
                        segment: crate::backing::test_backing(),
                        offset: index * 1024,
                        len: 1024,
                    }],
                )
                .unwrap();
            assert!(tree.root.is_none());
            assert_eq!(tree.count(), 1);
            assert_eq!(tree.height(), 1);
            assert_eq!(tree.logical_allocation_charge().unwrap(), 8);
        }
        assert_eq!(
            tree.pieces(),
            vec![Piece::Spool {
                segment: crate::backing::test_backing(),
                offset: 0,
                len: 512000
            }]
        );
        for source in [0, tree.len() - 1, tree.len() + 1] {
            let next = tree
                .replace(
                    tree.len(),
                    0,
                    [Piece::Spool {
                        segment: crate::backing::test_backing(),
                        offset: source,
                        len: 1,
                    }],
                )
                .unwrap();
            assert_eq!(next.count(), 2);
            assert_eq!(
                next.range(tree.len() - 1, tree.len() + 1).unwrap(),
                vec![
                    Piece::Spool {
                        segment: crate::backing::test_backing(),
                        offset: tree.len() - 1,
                        len: 1
                    },
                    Piece::Spool {
                        segment: crate::backing::test_backing(),
                        offset: source,
                        len: 1
                    },
                ]
            );
        }
        let overwritten = tree
            .replace(
                7,
                1,
                [Piece::Spool {
                    segment: crate::backing::test_backing(),
                    offset: tree.len(),
                    len: 1,
                }],
            )
            .unwrap();
        assert_eq!(
            overwritten.range(7, 8).unwrap(),
            vec![Piece::Spool {
                segment: crate::backing::test_backing(),
                offset: tree.len(),
                len: 1
            }]
        );
        assert_eq!(
            tree.range(7, 8).unwrap(),
            vec![Piece::Spool {
                segment: crate::backing::test_backing(),
                offset: 7,
                len: 1
            }]
        );
        assert!(tree
            .replace(
                tree.len(),
                0,
                [Piece::Spool {
                    segment: crate::backing::test_backing(),
                    offset: tree.len(),
                    len: MAX_RESULT_BYTES,
                }]
            )
            .is_err());
    }

    #[test]
    fn implicit_piece_tree_splices_without_rekeying_later_pieces() {
        let root = FileContentRoot(ObjectId::for_bytes(b"base"));
        let tree = PieceTree::base(root, 10).unwrap();
        let tree = tree.replace(3, 2, [Piece::Zero { len: 4 }]).unwrap();
        assert_eq!(tree.len(), 12);
        assert_eq!(
            tree.pieces(),
            vec![
                Piece::Base {
                    root,
                    offset: 0,
                    len: 3
                },
                Piece::Zero { len: 4 },
                Piece::Base {
                    root,
                    offset: 5,
                    len: 5
                }
            ]
        );
        assert_eq!(
            tree.range(2, 8)
                .unwrap()
                .iter()
                .map(Piece::len)
                .sum::<u64>(),
            6
        );
    }

    #[test]
    fn maximum_fragmentation_remains_bounded_and_balanced() {
        fn depth(root: &Link) -> usize {
            root.as_ref()
                .map_or(0, |node| 1 + depth(&node.left).max(depth(&node.right)))
        }
        let root = FileContentRoot(ObjectId::for_bytes(b"fragmented-base"));
        let mut tree = PieceTree::base(root, 8_193).unwrap();
        for offset in (1..8_192).step_by(2) {
            tree = tree.replace(offset, 1, [Piece::Zero { len: 1 }]).unwrap();
        }
        assert_eq!(tree.count(), MAX_PIECES_PER_FILE);
        assert!(tree.logical_allocation_charge().unwrap() <= MAX_PIECE_ALLOCATION);
        assert!(depth(&tree.root) < 64, "depth={}", depth(&tree.root));
        for offset in [0, 4_096, 8_192] {
            let (pieces, visited) = tree.range_with_visits(offset, offset + 1).unwrap();
            assert_eq!(pieces.iter().map(Piece::len).sum::<u64>(), 1);
            assert!(visited < 64, "offset={offset} visited={visited}");
        }
        assert!(tree.replace(0, 0, [Piece::Zero { len: 1 }]).is_err());
        assert_eq!(tree.count(), MAX_PIECES_PER_FILE);
    }

    #[test]
    fn logical_zero_ceiling_is_exact_without_physical_allocation() {
        let tree = PieceTree::empty()
            .replace(
                0,
                0,
                [Piece::Zero {
                    len: MAX_LOGICAL_ZERO_BYTES,
                }],
            )
            .unwrap();
        assert_eq!(tree.len(), MAX_LOGICAL_ZERO_BYTES);
        assert!(PieceTree::empty()
            .replace(
                0,
                0,
                [Piece::Zero {
                    len: MAX_LOGICAL_ZERO_BYTES + 1
                }]
            )
            .is_err());
    }

    #[test]
    fn result_length_ceiling_accepts_exact_and_rejects_plus_one() {
        let root = FileContentRoot(ObjectId::for_bytes(b"large-base"));
        let exact = PieceTree::base(root, MAX_RESULT_BYTES).unwrap();
        assert_eq!(exact.len(), MAX_RESULT_BYTES);
        assert!(exact
            .replace(MAX_RESULT_BYTES, 0, [Piece::Zero { len: 1 }])
            .is_err());
        assert_eq!(exact.len(), MAX_RESULT_BYTES);
    }

    #[test]
    fn logical_allocation_charge_accepts_exact_and_rejects_plus_one() {
        assert!(check_logical_allocation_charge(MAX_PIECE_ALLOCATION).is_ok());
        assert!(check_logical_allocation_charge(MAX_PIECE_ALLOCATION + 1).is_err());
    }

    fn inline(bytes: &[u8]) -> Piece {
        Piece::Inline {
            bytes: Arc::from(bytes),
            offset: 0,
            len: bytes.len() as u64,
        }
    }

    /// The required default-budget capacity: 32,000 distinct files of the exact
    /// sequence shape must fit the unchanged 2 MiB pending piece allocation.
    #[test]
    fn compact_splice_admits_the_required_default_budget_file_count() {
        let root = FileContentRoot(ObjectId::for_bytes(b"sequence-base"));
        let charge = PieceTree::base(root, 49_152)
            .unwrap()
            .replace(1_000, 12, [inline(b"C32000000001")])
            .unwrap()
            .logical_allocation_charge()
            .unwrap();
        assert_eq!(charge, std::mem::size_of::<CompactSplice>() as u64);
        assert_eq!(MAX_PIECE_ALLOCATION / charge, 32_768);
        let required = 32_000u64;
        assert!(
            required * charge <= MAX_PIECE_ALLOCATION,
            "32,000 compact pending files must fit the unchanged 2 MiB budget"
        );
    }

    /// Mutation, exact logical contents, retained-reader snapshots and the
    /// materialization path for a second, more complex edit.
    #[test]
    fn compact_splice_preserves_contents_ranges_and_conversion() {
        let root = FileContentRoot(ObjectId::for_bytes(b"compact-base"));
        let base = PieceTree::base(root, 100).unwrap();
        let edited = base.replace(10, 4, [inline(b"WXYZ")]).unwrap();
        assert_eq!(edited.count(), 3);
        assert_eq!(edited.len(), 100);
        assert_eq!(edited.inline_len(), 4);
        assert_eq!(edited.spool_len(), 0);
        assert_eq!(
            edited.logical_allocation_charge().unwrap(),
            std::mem::size_of::<CompactSplice>() as u64
        );
        assert_eq!(
            edited.range(8, 16).unwrap(),
            vec![
                Piece::Base {
                    root,
                    offset: 8,
                    len: 2
                },
                inline(b"WXYZ"),
                Piece::Base {
                    root,
                    offset: 14,
                    len: 2
                },
            ]
        );
        assert_eq!(edited.range(0, 100).unwrap(), edited.pieces());
        assert_eq!(
            edited
                .range(0, 100)
                .unwrap()
                .iter()
                .map(Piece::len)
                .sum::<u64>(),
            100
        );
        // Edge shapes stay one and two logical pieces.
        assert_eq!(base.replace(0, 4, [inline(b"WXYZ")]).unwrap().count(), 2);
        assert_eq!(base.replace(96, 4, [inline(b"WXYZ")]).unwrap().count(), 2);
        assert_eq!(base.replace(0, 100, [inline(b"WXYZ")]).unwrap().count(), 1);
        // The original snapshot is unchanged and still fully readable.
        assert_eq!(base.count(), 1);
        assert_eq!(base.pieces().len(), 1);
        // A second edit materializes the bounded form and keeps exact contents.
        let twice = edited.replace(50, 2, [inline(b"ab")]).unwrap();
        assert_eq!(twice.count(), 5);
        assert_eq!(twice.len(), 100);
        assert_eq!(
            twice
                .range(8, 16)
                .unwrap()
                .iter()
                .map(Piece::len)
                .sum::<u64>(),
            8
        );
        assert_eq!(twice.inline_len(), 6);
        // Unequal-length replacement is not the bounded shape: it stays a tree.
        let grown = base.replace(10, 4, [inline(b"longer")]).unwrap();
        assert_eq!(grown.len(), 102);
        assert_eq!(grown.count(), 3);
    }

    /// The bounded form is refused, not mis-encoded, when the shape does not
    /// match: a growing write, an empty replacement and an out-of-range range.
    #[test]
    fn compact_splice_rejects_non_matching_shapes() {
        let root = FileContentRoot(ObjectId::for_bytes(b"reject-base"));
        let base = PieceTree::base(root, 32).unwrap();
        assert!(base.replace(8, 4, [inline(b"123456")]).unwrap().len() == 34);
        assert!(
            base.replace(8, 4, [Piece::Zero { len: 4 }])
                .unwrap()
                .count()
                == 3
        );
        assert!(base.replace(40, 4, [inline(b"1234")]).is_err());
        let two_piece = base
            .replace(28, 4, [inline(b"1234"), inline(b"5678")])
            .unwrap();
        assert_eq!(two_piece.len(), 36);
        assert_eq!(two_piece.count(), 3);
        let empty = PieceTree::base(root, 32).unwrap().replace(0, 0, []);
        assert_eq!(empty.unwrap().count(), 1);
    }

    /// Simultaneous ownership: a held snapshot keeps reading the exact bytes of
    /// the bounded form after the owner installs a materialized successor.
    #[test]
    fn compact_splice_retained_snapshot_survives_conversion() {
        let root = FileContentRoot(ObjectId::for_bytes(b"retained-base"));
        let base = PieceTree::base(root, 64).unwrap();
        let held = base.replace(4, 8, [inline(b"01234567")]).unwrap();
        let successor = held.replace(20, 8, [inline(b"abcdefgh")]).unwrap();
        assert_eq!(successor.count(), 5);
        assert_eq!(held.count(), 3);
        assert_eq!(
            held.range(0, 16).unwrap(),
            vec![
                Piece::Base {
                    root,
                    offset: 0,
                    len: 4
                },
                inline(b"01234567"),
                Piece::Base {
                    root,
                    offset: 12,
                    len: 4
                },
            ]
        );
        assert_eq!(held.pieces().iter().map(Piece::len).sum::<u64>(), 64);
        assert_eq!(successor.pieces().iter().map(Piece::len).sum::<u64>(), 64);
    }
}
