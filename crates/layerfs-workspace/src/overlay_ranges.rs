//! Implicit-offset host range treap using the existing PieceTree split/merge
//! shape. Index leaves retain child graphs and payload tokens on disk.
use crate::correspondence::{Descriptor, Kind, OriginId, OriginSequence};
use crate::overlay_index::{Index, Record, Root};
use crate::overlay_payload::{OwnedRange, Payload};
use layerfs_content::ObjectId;
use std::io::{self, ErrorKind, Read};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const META: &[u8] = &[0];
const LEFT: &[u8] = &[1];
const RIGHT: &[u8] = &[2];
const PROVENANCE: &[u8] = &[3];
const BACKING: &[u8] = &[4];
const MAX_HEIGHT: u64 = 128;
const INLINE: usize = 64;

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::InvalidInput, message)
}
fn corrupt(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}
fn quota(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::StorageFull, message)
}
fn add(a: u64, b: u64) -> io::Result<u64> {
    a.checked_add(b)
        .ok_or_else(|| invalid("range coordinate overflow"))
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    pub max_length: u64,
    pub max_pieces: u64,
    pub max_height: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_length: layerfs_workspace_core::file_edit::MAX_RESULT_BYTES,
            max_pieces: layerfs_workspace_core::file_edit::MAX_PIECES_PER_FILE as u64,
            max_height: MAX_HEIGHT,
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Stats {
    pub node_reads: u64,
    pub node_writes: u64,
    pub split_visits: u64,
    pub merge_visits: u64,
    pub cursor_visits: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Source {
    Base { root: ObjectId, offset: u64 },
    Payload { token: u64 },
    Zero,
    Inline { bytes: [u8; INLINE] },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Piece {
    pub source: Source,
    pub length: u64,
    pub origin: Option<OriginId>,
    pub origin_offset: u64,
    // A logical origin may extend only on its original never-rewound source.
    // Equivalent canonical substitution never grants a new extension frontier.
    extend_source: Option<u64>,
    // SDK inline accounting survives private disk transfer until an exact,
    // admitted canonical backing substitution releases the current inline bytes.
    inline_charge: bool,
}
pub(crate) struct OwnedPiece {
    pub piece: Piece,
    _payload: Option<OwnedRange>,
}
impl OwnedPiece {
    pub(crate) fn base(
        root: ObjectId,
        offset: u64,
        length: u64,
        origin: OriginId,
        origin_offset: u64,
    ) -> io::Result<Self> {
        Self::plain(Piece {
            source: Source::Base { root, offset },
            length,
            origin: Some(origin),
            origin_offset,
            extend_source: None,
            inline_charge: false,
        })
    }
    pub(crate) fn zero(length: u64) -> io::Result<Self> {
        Self::plain(Piece {
            source: Source::Zero,
            length,
            origin: None,
            origin_offset: 0,
            extend_source: None,
            inline_charge: false,
        })
    }
    pub(crate) fn inline(input: &[u8], origin: OriginId) -> io::Result<Self> {
        if input.len() > INLINE {
            return Err(invalid("inline range limit"));
        }
        let mut bytes = [0; INLINE];
        bytes[..input.len()].copy_from_slice(input);
        Self::plain(Piece {
            source: Source::Inline { bytes },
            length: input.len() as u64,
            origin: Some(origin),
            origin_offset: 0,
            extend_source: None,
            inline_charge: true,
        })
    }
    pub(crate) fn payload(range: OwnedRange, origin: OriginId) -> io::Result<Self> {
        let piece = Piece {
            source: Source::Payload {
                token: range.token(),
            },
            length: range.len(),
            origin: Some(origin),
            origin_offset: 0,
            extend_source: Some(range.source()),
            inline_charge: false,
        };
        piece.validate()?;
        Ok(Self {
            piece,
            _payload: Some(range),
        })
    }
    pub(crate) fn inline_payload(range: OwnedRange, origin: OriginId) -> io::Result<Self> {
        let mut owned = Self::payload(range, origin)?;
        owned.piece.inline_charge = true;
        owned.piece.extend_source = None;
        Ok(owned)
    }
    fn plain(piece: Piece) -> io::Result<Self> {
        piece.validate()?;
        Ok(Self {
            piece,
            _payload: None,
        })
    }
    pub(crate) fn descriptor(&self, offset: u64) -> Descriptor {
        self.piece.descriptor(offset)
    }

    /// Caller proves byte equivalence through published origin correspondence.
    /// Preserve occurrence coordinates; canonical backing owns no raw inline
    /// allocation and cannot extend the original mutable source frontier.
    pub(crate) fn canonical_slice(
        piece: Piece,
        local: u64,
        length: u64,
        root: ObjectId,
        offset: u64,
    ) -> io::Result<Self> {
        if !matches!(piece.source, Source::Payload { .. } | Source::Inline { .. })
            || length == 0
            || add(local, length)? > piece.length
        {
            return Err(invalid("canonical replacement slice"));
        }
        Self::plain(Piece {
            source: Source::Base { root, offset },
            length,
            origin: piece.origin,
            origin_offset: add(piece.origin_offset, local)?,
            extend_source: None,
            inline_charge: false,
        })
    }
}
impl Piece {
    pub(crate) fn descriptor(self, offset: u64) -> Descriptor {
        Descriptor {
            offset,
            length: self.length,
            origin: self.origin,
            origin_offset: self.origin_offset,
            kind: match self.source {
                Source::Base { .. } => Kind::Base,
                Source::Payload { .. } => Kind::Temporary,
                Source::Zero => Kind::Zero,
                Source::Inline { .. } => Kind::Inline,
            },
        }
    }
    fn validate(self) -> io::Result<()> {
        if self.length == 0 || matches!(self.source, Source::Zero) != self.origin.is_none() {
            return Err(invalid("range lineage or length"));
        }
        if self.origin.is_some() {
            add(self.origin_offset, self.length)?;
        } else if self.origin_offset != 0 {
            return Err(invalid("zero range lineage"));
        }
        match self.source {
            Source::Base { offset, .. } => {
                add(offset, self.length)?;
            }
            Source::Inline { bytes } => {
                if self.length > INLINE as u64
                    || bytes[self.length as usize..].iter().any(|byte| *byte != 0)
                {
                    return Err(invalid("inline range length"));
                }
            }
            Source::Payload { token } if token == u64::MAX || token == 0 => {
                return Err(invalid("reserved payload token"))
            }
            _ => {}
        }
        if !matches!(self.source, Source::Payload { .. }) && self.extend_source.is_some() {
            return Err(invalid("invalid extendable lineage"));
        }
        Ok(())
    }
    fn metadata(self) -> [u8; 49] {
        let mut bytes = [0; 49];
        bytes[..8].copy_from_slice(&self.length.to_be_bytes());
        if let Some(origin) = self.origin {
            bytes[8..32].copy_from_slice(&origin.to_bytes());
        }
        bytes[32..40].copy_from_slice(&self.origin_offset.to_be_bytes());
        bytes[40..48].copy_from_slice(&self.extend_source.unwrap_or(0).to_be_bytes());
        bytes[48] = (if self.inline_charge { 0x80 } else { 0 })
            | match self.source {
                Source::Base { .. } => 0,
                Source::Payload { .. } => 1,
                Source::Zero => 2,
                Source::Inline { .. } => 3,
            };
        bytes
    }
    fn backing(self) -> Vec<u8> {
        match self.source {
            Source::Base { root, offset } => {
                [root.as_bytes().as_slice(), &offset.to_be_bytes()].concat()
            }
            Source::Payload { token } => token.to_be_bytes().to_vec(),
            Source::Zero => Vec::new(),
            Source::Inline { bytes } => bytes[..self.length as usize].to_vec(),
        }
    }
    fn decode(metadata: &[u8], backing: &[u8]) -> io::Result<Self> {
        if metadata.len() != 49 {
            return Err(corrupt("range piece metadata"));
        }
        let length = u64::from_be_bytes(metadata[..8].try_into().unwrap());
        let source = match metadata[48] & 0x7f {
            0 if backing.len() == 40 => Source::Base {
                root: ObjectId::from_bytes(&backing[..32])
                    .map_err(|_| corrupt("range base identity"))?,
                offset: u64::from_be_bytes(backing[32..].try_into().unwrap()),
            },
            1 if backing.len() == 8 => Source::Payload {
                token: u64::from_be_bytes(backing.try_into().unwrap()),
            },
            2 if backing.is_empty() => Source::Zero,
            3 if backing.len() <= INLINE && backing.len() as u64 == length => {
                let mut bytes = [0; INLINE];
                bytes[..backing.len()].copy_from_slice(backing);
                Source::Inline { bytes }
            }
            _ => return Err(corrupt("range backing locator")),
        };
        let extend = u64::from_be_bytes(metadata[40..48].try_into().unwrap());
        let piece = Self {
            source,
            length,
            origin: if matches!(source, Source::Zero) {
                None
            } else {
                Some(OriginId::from_bytes(metadata[8..32].try_into().unwrap())?)
            },
            origin_offset: u64::from_be_bytes(metadata[32..40].try_into().unwrap()),
            extend_source: (extend != 0).then_some(extend),
            inline_charge: metadata[48] & 0x80 != 0,
        };
        piece.validate()?;
        if piece.metadata().as_slice() != metadata {
            return Err(corrupt("noncanonical range metadata"));
        }
        Ok(piece)
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Summary {
    length: u64,
    count: u64,
    height: u64,
    priority: u64,
    left_length: u64,
    zero_bytes: u64,
    inline_bytes: u64,
    payload_bytes: u64,
}
impl Summary {
    fn encode(self) -> [u8; 64] {
        let mut bytes = [0; 64];
        for (i, value) in [
            self.length,
            self.count,
            self.height,
            self.priority,
            self.left_length,
            self.zero_bytes,
            self.inline_bytes,
            self.payload_bytes,
        ]
        .into_iter()
        .enumerate()
        {
            bytes[i * 8..i * 8 + 8].copy_from_slice(&value.to_be_bytes());
        }
        bytes
    }
    fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() != 64 {
            return Err(corrupt("range node summary"));
        }
        let word = |i: usize| u64::from_be_bytes(bytes[i * 8..i * 8 + 8].try_into().unwrap());
        let summary = Self {
            length: word(0),
            count: word(1),
            height: word(2),
            priority: word(3),
            left_length: word(4),
            zero_bytes: word(5),
            inline_bytes: word(6),
            payload_bytes: word(7),
        };
        if summary.length == 0
            || summary.count == 0
            || summary.height == 0
            || summary.height > MAX_HEIGHT
            || summary.left_length >= summary.length
            || summary
                .zero_bytes
                .checked_add(summary.inline_bytes)
                .is_none_or(|n| n > summary.length)
            || summary
                .zero_bytes
                .checked_add(summary.payload_bytes)
                .is_none_or(|n| n > summary.length)
        {
            return Err(corrupt("invalid range node summary"));
        }
        Ok(summary)
    }
}
#[derive(Clone, Debug, Default)]
pub(crate) struct Tree {
    root: Option<Root>,
    summary: Summary,
}
impl Tree {
    pub(crate) fn len(&self) -> u64 {
        self.summary.length
    }
    pub(crate) fn count(&self) -> u64 {
        self.summary.count
    }
    pub(crate) fn height(&self) -> u64 {
        self.summary.height
    }
    pub(crate) fn zero_bytes(&self) -> u64 {
        self.summary.zero_bytes
    }
    pub(crate) fn inline_bytes(&self) -> u64 {
        self.summary.inline_bytes
    }
    pub(crate) fn payload_bytes(&self) -> u64 {
        self.summary.payload_bytes
    }
    pub(crate) fn root(&self) -> Option<&Root> {
        self.root.as_ref()
    }
}
struct Node {
    piece: Piece,
    priority: u64,
    left: Tree,
    right: Tree,
}
#[derive(Clone)]
pub(crate) struct Ranges(Arc<Inner>);
struct Inner {
    index: Index,
    payload: Payload,
    limits: Limits,
    serial: AtomicU64,
    reads: AtomicU64,
    writes: AtomicU64,
    splits: AtomicU64,
    merges: AtomicU64,
    cursors: AtomicU64,
}
impl Ranges {
    pub(crate) fn new(index: Index, payload: Payload, limits: Limits) -> io::Result<Self> {
        if limits.max_length == 0
            || limits.max_pieces == 0
            || limits.max_height == 0
            || limits.max_height > MAX_HEIGHT
        {
            return Err(invalid("range limits"));
        }
        Ok(Self(Arc::new(Inner {
            index,
            payload,
            limits,
            serial: AtomicU64::new(0),
            reads: AtomicU64::new(0),
            writes: AtomicU64::new(0),
            splits: AtomicU64::new(0),
            merges: AtomicU64::new(0),
            cursors: AtomicU64::new(0),
        })))
    }
    pub(crate) fn root_inline_bytes(index: &Index, root: Option<&Root>) -> io::Result<u64> {
        let Some(root) = root else { return Ok(0) };
        let header = index
            .get(root, META)?
            .ok_or_else(|| corrupt("missing range aggregate"))?;
        Ok(Summary::decode(&header)?.inline_bytes)
    }
    pub(crate) fn empty(&self) -> Tree {
        Tree::default()
    }
    pub(crate) fn max_pieces(&self) -> u64 {
        self.0.limits.max_pieces
    }
    pub(crate) fn restore(&self, root: Option<Root>) -> io::Result<Tree> {
        match root {
            None => Ok(self.empty()),
            Some(root) => {
                let bytes = self
                    .0
                    .index
                    .get(&root, META)?
                    .ok_or_else(|| corrupt("missing range root header"))?;
                let tree = Tree {
                    summary: Summary::decode(&bytes)?,
                    root: Some(root),
                };
                self.check(&tree)?;
                Ok(tree)
            }
        }
    }
    fn check(&self, tree: &Tree) -> io::Result<()> {
        if tree.len() > self.0.limits.max_length
            || tree.count() > self.0.limits.max_pieces
            || tree.height() > self.0.limits.max_height
            || tree.zero_bytes() > layerfs_workspace_core::file_edit::MAX_LOGICAL_ZERO_BYTES
            || tree.zero_bytes().div_ceil(8192)
                > layerfs_workspace_core::file_edit::MAX_PREDICTED_ZERO_EXTENTS
        {
            return Err(quota("range tree limits"));
        }
        Ok(())
    }
    fn child(&self, root: &Root, key: &[u8]) -> io::Result<Tree> {
        match self.0.index.get_linked(root, key)? {
            None => Ok(self.empty()),
            Some((bytes, Some(root))) => {
                let tree = Tree {
                    summary: Summary::decode(&bytes)?,
                    root: Some(root),
                };
                self.check(&tree)?;
                Ok(tree)
            }
            _ => Err(corrupt("range child ownership")),
        }
    }
    fn load(&self, tree: &Tree) -> io::Result<Node> {
        let root = tree
            .root
            .as_ref()
            .ok_or_else(|| corrupt("empty range node"))?;
        self.0.reads.fetch_add(1, Ordering::Relaxed);
        let header = self
            .0
            .index
            .get(root, META)?
            .ok_or_else(|| corrupt("missing range summary"))?;
        if Summary::decode(&header)? != tree.summary {
            return Err(corrupt("range child summary mismatch"));
        }
        let metadata = self
            .0
            .index
            .get(root, PROVENANCE)?
            .ok_or_else(|| corrupt("missing range provenance"))?;
        let backing = self
            .0
            .index
            .get(root, BACKING)?
            .ok_or_else(|| corrupt("missing range backing"))?;
        let piece = Piece::decode(&metadata, &backing)?;
        let left = self.child(root, LEFT)?;
        let right = self.child(root, RIGHT)?;
        let summary = self.summary(piece, tree.summary.priority, &left, &right)?;
        if summary != tree.summary {
            return Err(corrupt("range subtree aggregate mismatch"));
        }
        Ok(Node {
            piece,
            priority: tree.summary.priority,
            left,
            right,
        })
    }
    fn summary(
        &self,
        piece: Piece,
        priority: u64,
        left: &Tree,
        right: &Tree,
    ) -> io::Result<Summary> {
        piece.validate()?;
        let summary = Summary {
            length: add(add(left.len(), piece.length)?, right.len())?,
            count: add(add(left.count(), 1)?, right.count())?,
            height: add(left.height().max(right.height()), 1)?,
            priority,
            left_length: left.len(),
            zero_bytes: add(
                add(
                    left.zero_bytes(),
                    if matches!(piece.source, Source::Zero) {
                        piece.length
                    } else {
                        0
                    },
                )?,
                right.zero_bytes(),
            )?,
            inline_bytes: add(
                add(
                    left.inline_bytes(),
                    if piece.inline_charge { piece.length } else { 0 },
                )?,
                right.inline_bytes(),
            )?,
            payload_bytes: add(
                add(
                    left.payload_bytes(),
                    if matches!(piece.source, Source::Payload { .. }) {
                        piece.length
                    } else {
                        0
                    },
                )?,
                right.payload_bytes(),
            )?,
        };
        if summary.length > self.0.limits.max_length
            || summary.count > self.0.limits.max_pieces
            || summary.height > self.0.limits.max_height
            || summary.zero_bytes > layerfs_workspace_core::file_edit::MAX_LOGICAL_ZERO_BYTES
            || summary.zero_bytes.div_ceil(8192)
                > layerfs_workspace_core::file_edit::MAX_PREDICTED_ZERO_EXTENTS
        {
            return Err(quota("range tree limits"));
        }
        if left.root.is_some() && left.summary.priority > priority
            || right.root.is_some() && right.summary.priority > priority
        {
            return Err(corrupt("range heap invariant"));
        }
        Ok(summary)
    }
    // ponytail: one 4-KiB leaf per range node, plus transient COW pages;
    // pack nodes together if measured metadata pressure requires that change.
    fn node(&self, piece: Piece, priority: u64, left: &Tree, right: &Tree) -> io::Result<Tree> {
        let summary = self.summary(piece, priority, left, right)?;
        // A node's records share one leaf, so they are prepared together and
        // applied with a single path copy. The former per-field sequence
        // published and retired an intermediate root for every record.
        let encoded = summary.encode();
        let provenance = piece.metadata();
        let backing = piece.backing();
        let left_summary = left.root().map(|_| left.summary.encode());
        let right_summary = right.root().map(|_| right.summary.encode());
        let mut records = Vec::with_capacity(5);
        records.push(Record {
            key: META,
            value: &encoded,
            external: None,
            linked: None,
        });
        if let Some(child) = left.root() {
            records.push(Record {
                key: LEFT,
                value: left_summary
                    .as_ref()
                    .ok_or_else(|| corrupt("range node child summary"))?,
                external: None,
                linked: Some(child),
            });
        }
        if let Some(child) = right.root() {
            records.push(Record {
                key: RIGHT,
                value: right_summary
                    .as_ref()
                    .ok_or_else(|| corrupt("range node child summary"))?,
                external: None,
                linked: Some(child),
            });
        }
        records.push(Record {
            key: PROVENANCE,
            value: &provenance,
            external: None,
            linked: None,
        });
        records.push(Record {
            key: BACKING,
            value: &backing,
            external: match piece.source {
                Source::Payload { token } => Some(token),
                _ => None,
            },
            linked: None,
        });
        let empty = self.0.index.empty_root()?;
        let root = self.0.index.set_batch(&empty, &records)?;
        self.0.writes.fetch_add(1, Ordering::Relaxed);
        Ok(Tree {
            root: Some(root),
            summary,
        })
    }
    fn priority(&self) -> io::Result<u64> {
        let serial = self
            .0
            .serial
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| quota("range priority serial exhausted"))?
            + 1;
        // Same SplitMix64 permutation as the existing core PieceTree.
        let mut value = serial.wrapping_add(0x9e37_79b9_7f4a_7c15);
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        Ok(value ^ (value >> 31))
    }
    pub(crate) fn stats(&self) -> Stats {
        Stats {
            node_reads: self.0.reads.load(Ordering::Relaxed),
            node_writes: self.0.writes.load(Ordering::Relaxed),
            split_visits: self.0.splits.load(Ordering::Relaxed),
            merge_visits: self.0.merges.load(Ordering::Relaxed),
            cursor_visits: self.0.cursors.load(Ordering::Relaxed),
        }
    }
}

impl Ranges {
    /// Merge partitioned pieces in logical order. Ordinary replacement inputs
    /// must use fresh occurrence lineage; copying an existing occurrence is not
    /// a license to duplicate its old origin coordinates.
    pub(crate) fn merge(&self, left: &Tree, right: &Tree) -> io::Result<Tree> {
        self.merge_at(left, right, 0)
    }
    fn merge_at(&self, left: &Tree, right: &Tree, depth: u64) -> io::Result<Tree> {
        if depth > self.0.limits.max_height {
            return Err(quota("range merge depth"));
        }
        if left.root.is_none() {
            return Ok(right.clone());
        }
        if right.root.is_none() {
            return Ok(left.clone());
        }
        self.0.merges.fetch_add(1, Ordering::Relaxed);
        if left.summary.priority >= right.summary.priority {
            let node = self.load(left)?;
            let joined = self.merge_at(&node.right, right, depth + 1)?;
            self.node(node.piece, node.priority, &node.left, &joined)
        } else {
            let node = self.load(right)?;
            let joined = self.merge_at(left, &node.left, depth + 1)?;
            self.node(node.piece, node.priority, &joined, &node.right)
        }
    }
    fn slice(&self, piece: Piece, offset: u64, length: u64) -> io::Result<OwnedPiece> {
        if length == 0 || add(offset, length)? > piece.length {
            return Err(invalid("range piece slice"));
        }
        let mut sliced = piece;
        sliced.length = length;
        if piece.origin.is_some() {
            sliced.origin_offset = add(piece.origin_offset, offset)?;
        }
        let mut owner = None;
        sliced.source = match piece.source {
            Source::Base { root, offset: base } => Source::Base {
                root,
                offset: add(base, offset)?,
            },
            Source::Payload { token } => {
                let range = self.0.payload.subrange(token, offset, length)?;
                let token = range.token();
                owner = Some(range);
                Source::Payload { token }
            }
            Source::Zero => Source::Zero,
            Source::Inline { bytes } => {
                let mut sliced = [0; INLINE];
                sliced[..length as usize]
                    .copy_from_slice(&bytes[offset as usize..(offset + length) as usize]);
                Source::Inline { bytes: sliced }
            }
        };
        sliced.validate()?;
        Ok(OwnedPiece {
            piece: sliced,
            _payload: owner,
        })
    }
    pub(crate) fn split(&self, tree: &Tree, offset: u64) -> io::Result<(Tree, Tree)> {
        if offset > tree.len() {
            return Err(invalid("range split offset"));
        }
        self.split_at(tree, offset, 0)
    }
    fn split_at(&self, tree: &Tree, offset: u64, depth: u64) -> io::Result<(Tree, Tree)> {
        if offset == 0 {
            return Ok((self.empty(), tree.clone()));
        }
        if offset == tree.len() {
            return Ok((tree.clone(), self.empty()));
        }
        if depth > self.0.limits.max_height {
            return Err(quota("range split depth"));
        }
        self.0.splits.fetch_add(1, Ordering::Relaxed);
        let node = self.load(tree)?;
        let piece_end = add(node.left.len(), node.piece.length)?;
        if offset < node.left.len() {
            let (left, middle) = self.split_at(&node.left, offset, depth + 1)?;
            let tail = self.node(node.piece, node.priority, &self.empty(), &node.right)?;
            return Ok((left, self.merge(&middle, &tail)?));
        }
        if offset > piece_end {
            let (middle, right) = self.split_at(&node.right, offset - piece_end, depth + 1)?;
            let prefix = self.node(node.piece, node.priority, &node.left, &self.empty())?;
            return Ok((self.merge(&prefix, &middle)?, right));
        }
        if offset == node.left.len() {
            return Ok((
                node.left,
                self.node(node.piece, node.priority, &self.empty(), &node.right)?,
            ));
        }
        if offset == piece_end {
            return Ok((
                self.node(node.piece, node.priority, &node.left, &self.empty())?,
                node.right,
            ));
        }
        let local = offset - node.left.len();
        let first = self.slice(node.piece, 0, local)?;
        let last = self.slice(node.piece, local, node.piece.length - local)?;
        let first_tree = self.node(first.piece, self.priority()?, &self.empty(), &self.empty())?;
        let last_tree = self.node(last.piece, self.priority()?, &self.empty(), &self.empty())?;
        Ok((
            self.merge(&node.left, &first_tree)?,
            self.merge(&last_tree, &node.right)?,
        ))
    }
    pub(crate) fn replace(
        &self,
        tree: &Tree,
        start: u64,
        delete_length: u64,
        replacement: impl IntoIterator<Item = io::Result<OwnedPiece>>,
    ) -> io::Result<Tree> {
        if start > tree.len() || add(start, delete_length)? > tree.len() {
            return Err(invalid("range replacement bounds"));
        }
        let (left, tail) = self.split(tree, start)?;
        let (_, right) = self.split(&tail, delete_length)?;
        let mut middle = self.empty();
        for item in replacement {
            let owned = item?;
            owned.piece.validate()?;
            let node = self.node(owned.piece, self.priority()?, &self.empty(), &self.empty())?;
            middle = self.merge(&middle, &node)?;
        }
        let next = self.merge(&self.merge(&left, &middle)?, &right)?;
        self.check(&next)?;
        Ok(next)
    }
    pub(crate) fn truncate(&self, tree: &Tree, length: u64) -> io::Result<Tree> {
        if length <= tree.len() {
            return self.split(tree, length).map(|(prefix, _)| prefix);
        }
        self.replace(tree, tree.len(), 0, [OwnedPiece::zero(length - tree.len())])
    }
    pub(crate) fn append(&self, tree: &Tree, piece: OwnedPiece) -> io::Result<Tree> {
        self.replace(tree, tree.len(), 0, [Ok(piece)])
    }
    /// Sequential payload additions use the host source's never-reused frontier,
    /// retaining one range node when exact physical and logical lineage permits.
    pub(crate) fn append_from(
        &self,
        tree: &Tree,
        reader: &mut impl Read,
        length: u64,
        origins: &OriginSequence,
    ) -> io::Result<Tree> {
        if length == 0 {
            return Ok(tree.clone());
        }
        if add(tree.len(), length)? > self.0.limits.max_length {
            return Err(quota("range file length"));
        }
        if let Some((_, node)) = self.last(tree)? {
            if let (Source::Payload { token }, Some(source)) =
                (node.piece.source, node.piece.extend_source)
            {
                add(node.piece.origin_offset, add(node.piece.length, length)?)?;
                if let Some(appended) = self.0.payload.append(token, reader, length)? {
                    if appended.source() != source {
                        return Err(corrupt("append lineage source changed"));
                    }
                    let union = self.0.payload.join_tokens(token, appended.token())?;
                    let mut piece = node.piece;
                    piece.length = add(piece.length, length)?;
                    piece.source = Source::Payload {
                        token: union.token(),
                    };
                    piece.validate()?;
                    return self.replace(
                        tree,
                        tree.len() - node.piece.length,
                        node.piece.length,
                        [Ok(OwnedPiece {
                            piece,
                            _payload: Some(union),
                        })],
                    );
                }
            }
        }
        let owner = self.0.payload.write_from(reader, length)?;
        let input = OwnedPiece::payload(owner, origins.allocate()?)?;
        self.append(tree, input)
    }
    fn last(&self, tree: &Tree) -> io::Result<Option<(Tree, Node)>> {
        let mut current = tree.clone();
        for _ in 0..=self.0.limits.max_height {
            if current.root.is_none() {
                return Ok(None);
            }
            let node = self.load(&current)?;
            if node.right.root.is_none() {
                return Ok(Some((current, node)));
            }
            current = node.right;
        }
        Err(corrupt("range right path depth"))
    }
    pub(crate) fn cursor(&self, tree: &Tree, start: u64, stop: u64) -> io::Result<Cursor> {
        if start > stop || stop > tree.len() {
            return Err(invalid("range cursor bounds"));
        }
        let mut stack = Vec::new();
        stack
            .try_reserve_exact(tree.height() as usize)
            .map_err(|_| quota("range cursor stack"))?;
        let mut cursor = Cursor {
            ranges: self.clone(),
            snapshot: tree.clone(),
            stack,
            start,
            stop,
            done: false,
        };
        cursor.descend(tree.clone(), 0)?;
        Ok(cursor)
    }
    /// Transient reads are already protected by this owned root. Do not allocate
    /// a fresh payload-owner token for every streaming read buffer.
    pub(crate) fn read_range(
        &self,
        tree: &Tree,
        output: &mut [u8],
        offset: u64,
        mut base: impl FnMut(ObjectId, &mut [u8], u64) -> io::Result<()>,
    ) -> io::Result<usize> {
        let start = offset.min(tree.len());
        let stop = tree.len().min(start.saturating_add(output.len() as u64));
        let mut cursor = self.cursor(tree, start, stop)?;
        let mut filled = 0;
        while let Some((position, piece, local, length)) = cursor.next_raw()? {
            if position != start + filled as u64 {
                return Err(corrupt("range read coverage"));
            }
            let length = usize::try_from(length).map_err(|_| corrupt("range read width"))?;
            let target = output
                .get_mut(filled..filled + length)
                .ok_or_else(|| corrupt("range read output"))?;
            self.read_source(piece, target, local, &mut base)?;
            filled += length;
        }
        if filled as u64 != stop - start {
            return Err(corrupt("short range read"));
        }
        Ok(filled)
    }
    pub(crate) fn read_piece(
        &self,
        piece: &OwnedPiece,
        output: &mut [u8],
        offset: u64,
        base: impl FnMut(ObjectId, &mut [u8], u64) -> io::Result<()>,
    ) -> io::Result<()> {
        self.read_source(piece.piece, output, offset, base)
    }
    fn read_source(
        &self,
        piece: Piece,
        output: &mut [u8],
        offset: u64,
        mut base: impl FnMut(ObjectId, &mut [u8], u64) -> io::Result<()>,
    ) -> io::Result<()> {
        if add(offset, output.len() as u64)? > piece.length {
            return Err(invalid("range read bounds"));
        }
        match piece.source {
            Source::Base {
                root,
                offset: start,
            } => base(root, output, add(start, offset)?),
            Source::Payload { token } => self.0.payload.read_exact_at(token, output, offset),
            Source::Zero => {
                output.fill(0);
                Ok(())
            }
            Source::Inline { bytes } => {
                output.copy_from_slice(&bytes[offset as usize..offset as usize + output.len()]);
                Ok(())
            }
        }
    }
}
struct Frame {
    node: Node,
    offset: u64,
}
pub(crate) struct Cursor {
    ranges: Ranges,
    snapshot: Tree,
    stack: Vec<Frame>,
    start: u64,
    stop: u64,
    done: bool,
}
impl Cursor {
    fn descend(&mut self, mut tree: Tree, mut base: u64) -> io::Result<()> {
        while tree.root.is_some() {
            if base >= self.stop || add(base, tree.len())? <= self.start {
                return Ok(());
            }
            if self.stack.len() as u64 >= self.ranges.0.limits.max_height {
                return Err(quota("range cursor depth"));
            }
            self.ranges.0.cursors.fetch_add(1, Ordering::Relaxed);
            let node = self.ranges.load(&tree)?;
            let offset = add(base, node.left.len())?;
            let finish = add(offset, node.piece.length)?;
            if finish <= self.start {
                base = finish;
                tree = node.right;
                continue;
            }
            let left = node.left.clone();
            self.stack.push(Frame { node, offset });
            tree = left;
        }
        Ok(())
    }
    pub(crate) fn next_raw(&mut self) -> io::Result<Option<(u64, Piece, u64, u64)>> {
        if self.done {
            return Ok(None);
        }
        // Bound retired lookup-root ownership during arbitrarily long scans.
        // This performs metadata release only; it never copies payload bytes.
        self.ranges.0.index.reclaim(16)?;
        loop {
            let Some(frame) = self.stack.pop() else {
                self.done = true;
                self.snapshot = Tree::default();
                return Ok(None);
            };
            let right_start = add(frame.offset, frame.node.piece.length)?;
            self.descend(frame.node.right, right_start)?;
            let start = frame.offset.max(self.start);
            let stop = right_start.min(self.stop);
            if start < stop {
                return Ok(Some((
                    start,
                    frame.node.piece,
                    start - frame.offset,
                    stop - start,
                )));
            }
        }
    }
    /// Metadata-only initial predecessor mapping. The optional location is a
    /// canonical Base root/offset, never an unowned raw payload token.
    pub(crate) fn next_base_descriptor(
        &mut self,
    ) -> io::Result<Option<(Descriptor, Option<(ObjectId, u64)>)>> {
        self.next_raw()?
            .map(|(offset, piece, local, length)| {
                let mut descriptor = piece.descriptor(offset);
                descriptor.length = length;
                if descriptor.origin.is_some() {
                    descriptor.origin_offset = add(descriptor.origin_offset, local)?;
                }
                let base = match piece.source {
                    Source::Base { root, offset } => Some((root, add(offset, local)?)),
                    _ => None,
                };
                Ok((descriptor, base))
            })
            .transpose()
    }
    /// Descriptor-only iteration keeps the root during its metadata pass, without
    /// acquiring a new payload token for every visited descriptor.
    pub(crate) fn next_descriptor(&mut self) -> io::Result<Option<Descriptor>> {
        self.next_raw()?
            .map(|(offset, piece, local, length)| {
                let mut descriptor = piece.descriptor(offset);
                descriptor.length = length;
                if descriptor.origin.is_some() {
                    descriptor.origin_offset = add(descriptor.origin_offset, local)?;
                }
                Ok(descriptor)
            })
            .transpose()
    }
}
impl Iterator for Cursor {
    type Item = io::Result<(u64, OwnedPiece)>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let next = self.next_raw().and_then(|entry| {
            entry
                .map(|(offset, piece, local, length)| {
                    self.ranges
                        .slice(piece, local, length)
                        .map(|owned| (offset, owned))
                })
                .transpose()
        });
        match next {
            Ok(Some(piece)) => Some(Ok(piece)),
            Ok(None) => None,
            Err(error) => {
                self.done = true;
                self.stack.clear();
                self.snapshot = Tree::default();
                Some(Err(error))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay_index::Limits as IndexLimits;
    use crate::overlay_payload::Limits as PayloadLimits;
    use std::fs;

    struct Fixture {
        directory: std::path::PathBuf,
        ranges: Ranges,
        origins: OriginSequence,
        base: Vec<u8>,
        base_root: ObjectId,
    }
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let directory = std::env::temp_dir().join(format!(
                "layerfs-ranges-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&directory).unwrap();
            let index_limits = IndexLimits {
                max_pages: 65536,
                max_roots: 4096,
                ..IndexLimits::default()
            };
            let payload = Payload::temporary(
                &directory,
                PayloadLimits {
                    physical_bytes: 2 * 1024 * 1024,
                    owners: 4096,
                    readers: 32,
                    writes: 8,
                    index: index_limits,
                },
            )
            .unwrap();
            let index =
                Index::temporary_with_owner(&directory, index_limits, Arc::new(payload.clone()))
                    .unwrap();
            let ranges = Ranges::new(index, payload, Limits::default()).unwrap();
            let base = b"0123456789abcdefghijklmnopqrstuvwxyz".to_vec();
            let base_root = ObjectId::for_bytes(&base);
            Self {
                directory,
                ranges,
                origins: OriginSequence::new([8; 16]),
                base,
                base_root,
            }
        }
        fn inline(&self, bytes: &[u8]) -> OwnedPiece {
            OwnedPiece::inline(bytes, self.origins.allocate().unwrap()).unwrap()
        }
        fn initial(&self) -> Tree {
            self.ranges
                .append(
                    &self.ranges.empty(),
                    OwnedPiece::base(
                        self.base_root,
                        0,
                        self.base.len() as u64,
                        self.origins.allocate().unwrap(),
                        0,
                    )
                    .unwrap(),
                )
                .unwrap()
        }
        fn bytes(&self, tree: &Tree) -> Vec<u8> {
            let mut bytes = Vec::new();
            for item in self.ranges.cursor(tree, 0, tree.len()).unwrap() {
                let (_, piece) = item.unwrap();
                let mut data = vec![0; piece.piece.length as usize];
                self.ranges
                    .read_piece(&piece, &mut data, 0, |root, output, offset| {
                        assert_eq!(root, self.base_root);
                        output.copy_from_slice(
                            &self.base[offset as usize..offset as usize + output.len()],
                        );
                        Ok(())
                    })
                    .unwrap();
                bytes.extend(data);
            }
            bytes
        }
        fn drain(&self) {
            for _ in 0..20000 {
                self.ranges.0.index.reclaim(128).unwrap();
                self.ranges.0.payload.reclaim_step().unwrap();
                let index = self.ranges.0.index.stats().unwrap();
                let payload = self.ranges.0.payload.stats().unwrap();
                if !index.reclamation_pending && !payload.cleanup_pending {
                    return;
                }
            }
            panic!("range ownership cleanup did not converge")
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.directory).unwrap();
        }
    }

    #[test]
    fn logical_inline_charge_survives_payload_backing_and_splits() {
        let f = Fixture::new();
        let bytes = vec![9; 1024];
        let owner = f
            .ranges
            .0
            .payload
            .write_from(&mut bytes.as_slice(), bytes.len() as u64)
            .unwrap();
        let tree = f
            .ranges
            .append(
                &f.ranges.empty(),
                OwnedPiece::inline_payload(owner, f.origins.allocate().unwrap()).unwrap(),
            )
            .unwrap();
        assert_eq!((tree.inline_bytes(), tree.payload_bytes()), (1024, 1024));
        assert_eq!(
            Ranges::root_inline_bytes(&f.ranges.0.index, tree.root()).unwrap(),
            1024
        );
        let sliced = f.ranges.truncate(&tree, 17).unwrap();
        assert_eq!((sliced.inline_bytes(), sliced.payload_bytes()), (17, 17));
        let appended = f
            .ranges
            .append_from(&sliced, &mut &b"x"[..], 1, &f.origins)
            .unwrap();
        assert_eq!(appended.inline_bytes(), 17);
        assert_eq!(appended.payload_bytes(), 18);
    }

    #[test]
    fn repeated_snapshot_reads_allocate_no_per_buffer_payload_owners() {
        let f = Fixture::new();
        let bytes = vec![42; 4096];
        let payload = f
            .ranges
            .0
            .payload
            .write_from(&mut bytes.as_slice(), bytes.len() as u64)
            .unwrap();
        let tree = f
            .ranges
            .append(
                &f.ranges.empty(),
                OwnedPiece::payload(payload, f.origins.allocate().unwrap()).unwrap(),
            )
            .unwrap();
        f.drain();
        let before = f.ranges.0.payload.stats().unwrap();
        for offset in 0..5000 {
            let mut byte = [0];
            assert_eq!(
                f.ranges
                    .read_range(&tree, &mut byte, offset % 4096, |_, _, _| panic!(
                        "payload called canonical reader"
                    ))
                    .unwrap(),
                1
            );
            assert_eq!(byte, [42]);
        }
        let after = f.ranges.0.payload.stats().unwrap();
        assert_eq!(after.owners, before.owners);
        assert_eq!(after.pending_releases, 0);
        assert_eq!(after.readers, 0);
    }

    #[test]
    fn exact_zero_limit_and_cached_backing_aggregates_survive_disk_restore() {
        let f = Fixture::new();
        let limit = layerfs_workspace_core::file_edit::MAX_LOGICAL_ZERO_BYTES;
        let exact = f
            .ranges
            .append(&f.ranges.empty(), OwnedPiece::zero(limit).unwrap())
            .unwrap();
        assert_eq!(exact.zero_bytes(), limit);
        assert!(f
            .ranges
            .append(&exact, OwnedPiece::zero(1).unwrap())
            .is_err());
        assert_eq!(exact.len(), limit);
        let inline = f.ranges.append(&exact, f.inline(b"abc")).unwrap();
        let payload = f
            .ranges
            .0
            .payload
            .write_from(&mut &b"12345"[..], 5)
            .unwrap();
        let mixed = f
            .ranges
            .append(
                &inline,
                OwnedPiece::payload(payload, f.origins.allocate().unwrap()).unwrap(),
            )
            .unwrap();
        let restored = f.ranges.restore(mixed.root().cloned()).unwrap();
        assert_eq!(
            (
                restored.zero_bytes(),
                restored.inline_bytes(),
                restored.payload_bytes()
            ),
            (limit, 3, 5)
        );
        let shortened = f.ranges.truncate(&restored, limit + 4).unwrap();
        assert_eq!(
            (
                shortened.zero_bytes(),
                shortened.inline_bytes(),
                shortened.payload_bytes()
            ),
            (limit, 3, 1)
        );
        let mut cursor = f.ranges.cursor(&shortened, limit, limit + 4).unwrap();
        let (descriptor, base) = cursor.next_base_descriptor().unwrap().unwrap();
        assert_eq!(descriptor.kind, Kind::Inline);
        assert!(base.is_none());
        assert_eq!(descriptor.length, 3);
    }

    #[test]
    fn unequal_edits_split_merge_truncate_and_snapshot_match_byte_oracle() {
        let f = Fixture::new();
        let original = f.initial();
        let snapshot = original.clone();
        let mut tree = original;
        let mut expected = f.base.clone();
        for (start, delete, bytes) in [
            (5, 7, &b"XYZ"[..]),
            (0, 0, &b"prefix"[..]),
            (10, 1, &b"unequal"[..]),
            (4, 9, &b""[..]),
        ] {
            let inputs = (!bytes.is_empty()).then(|| Ok(f.inline(bytes)));
            tree = f.ranges.replace(&tree, start, delete, inputs).unwrap();
            expected.splice(
                start as usize..(start + delete) as usize,
                bytes.iter().copied(),
            );
            assert_eq!(f.bytes(&tree), expected);
        }
        let split = tree.len() / 2;
        let (left, right) = f.ranges.split(&tree, split).unwrap();
        let joined = f.ranges.merge(&left, &right).unwrap();
        assert_eq!(f.bytes(&joined), expected);
        tree = f.ranges.truncate(&tree, 7).unwrap();
        expected.truncate(7);
        assert_eq!(f.bytes(&tree), expected);
        tree = f.ranges.truncate(&tree, 12).unwrap();
        expected.resize(12, 0);
        assert_eq!(f.bytes(&tree), expected);
        assert_eq!(f.bytes(&snapshot), f.base);
        let before = f.bytes(&tree);
        assert!(f
            .ranges
            .replace(&tree, 13, 0, [Ok(f.inline(b"bad"))])
            .is_err());
        assert_eq!(f.bytes(&tree), before);
        assert!(f.ranges.cursor(&tree, 8, 7).is_err());
    }

    #[test]
    fn disk_child_links_and_exact_payload_slice_own_bytes_without_old_tree() {
        let f = Fixture::new();
        let data = (0..64 * 1024).map(|i| (i % 251) as u8).collect::<Vec<_>>();
        let origin = f.origins.allocate().unwrap();
        let payload = f
            .ranges
            .0
            .payload
            .write_from(&mut data.as_slice(), data.len() as u64)
            .unwrap();
        let tree = f
            .ranges
            .append(
                &f.ranges.empty(),
                OwnedPiece::payload(payload, origin).unwrap(),
            )
            .unwrap();
        let inode = f.ranges.0.index.empty_root().unwrap();
        let inode = f
            .ranges
            .0
            .index
            .set_linked(&inode, b"ranges", b"inode", tree.root())
            .unwrap();
        drop(tree);
        f.drain();
        let (_, linked) = f
            .ranges
            .0
            .index
            .get_linked(&inode, b"ranges")
            .unwrap()
            .unwrap();
        let tree = f.ranges.restore(linked).unwrap();
        let start = 9 * 4096 + 123;
        let mut cursor = f.ranges.cursor(&tree, start, start + 1).unwrap();
        let (_, one) = cursor.next().unwrap().unwrap();
        drop(cursor);
        let descriptor = one.descriptor(0);
        assert_eq!(descriptor.origin, Some(origin));
        assert_eq!(descriptor.origin_offset, start);
        drop(tree);
        drop(inode);
        f.drain();
        assert_eq!(
            f.ranges.0.index.stats().unwrap().live_pages,
            0,
            "piece must not retain the old file root"
        );
        assert_eq!(f.ranges.0.payload.stats().unwrap().live_block_bytes, 4096);
        let mut byte = [0];
        f.ranges
            .read_piece(&one, &mut byte, 0, |_, _, _| {
                panic!("payload read asked for base")
            })
            .unwrap();
        assert_eq!(byte[0], data[start as usize]);
        drop(one);
        f.drain();
        assert_eq!(f.ranges.0.payload.stats().unwrap().live_block_bytes, 0);
    }

    #[test]
    fn append_frontier_coalesces_without_reusing_truncated_origin_coordinates() {
        let f = Fixture::new();
        let mut tree = f
            .ranges
            .append_from(&f.ranges.empty(), &mut &b"abc"[..], 3, &f.origins)
            .unwrap();
        let snapshot = tree.clone();
        let first = f
            .ranges
            .cursor(&tree, 0, tree.len())
            .unwrap()
            .next_descriptor()
            .unwrap()
            .unwrap();
        for i in 0..80 {
            let bytes = [i as u8; 17];
            tree = f
                .ranges
                .append_from(&tree, &mut bytes.as_slice(), 17, &f.origins)
                .unwrap();
            f.drain();
        }
        assert_eq!(tree.count(), 1);
        assert_eq!(f.bytes(&snapshot), b"abc");
        let mut descriptors = f.ranges.cursor(&tree, 0, tree.len()).unwrap();
        let descriptor = descriptors.next_descriptor().unwrap().unwrap();
        assert_eq!(descriptor.origin, first.origin);
        assert_eq!(descriptor.origin_offset, 0);
        assert_eq!(descriptor.length, 1363);
        assert!(descriptors.next_descriptor().unwrap().is_none());
        drop(descriptors);
        let truncated = f.ranges.truncate(&tree, 2).unwrap();
        let appended = f
            .ranges
            .append_from(&truncated, &mut &b"z"[..], 1, &f.origins)
            .unwrap();
        assert_eq!(f.bytes(&appended), b"abz");
        let mut descriptors = f.ranges.cursor(&appended, 0, appended.len()).unwrap();
        let prefix = descriptors.next_descriptor().unwrap().unwrap();
        let fresh = descriptors.next_descriptor().unwrap().unwrap();
        assert_eq!(prefix.origin, first.origin);
        assert_ne!(fresh.origin, first.origin);
        assert_eq!(fresh.origin_offset, 0);
        assert_eq!(f.bytes(&tree).len(), 1363);
    }

    #[test]
    fn thousand_piece_small_edit_and_read_follow_paths_without_suffix_rekey() {
        let f = Fixture::new();
        let mut tree = f.ranges.empty();
        for i in 0..1024 {
            tree = f
                .ranges
                .append(&tree, f.inline(&[(i % 251) as u8]))
                .unwrap();
        }
        f.drain();
        let snapshot = tree.clone();
        let before = f.ranges.stats();
        let changed = f
            .ranges
            .replace(&tree, 511, 1, [Ok(f.inline(b"three"))])
            .unwrap();
        let after = f.ranges.stats();
        assert!(
            after.node_reads - before.node_reads < 256,
            "small edit scanned nodes: {:?}",
            after
        );
        assert!(
            after.node_writes - before.node_writes < 128,
            "small edit rewrote nodes: {:?}",
            after
        );
        assert_eq!(changed.len(), 1028);
        assert_eq!(changed.count(), 1024);
        let before = f.ranges.stats();
        let pieces = f
            .ranges
            .cursor(&changed, 510, 517)
            .unwrap()
            .collect::<io::Result<Vec<_>>>()
            .unwrap();
        let after = f.ranges.stats();
        assert!(after.cursor_visits - before.cursor_visits < 64);
        assert_eq!(pieces.iter().map(|(_, p)| p.piece.length).sum::<u64>(), 7);
        let mut expected = (0..1024).map(|i| (i % 251) as u8).collect::<Vec<_>>();
        expected.splice(511..512, b"three".iter().copied());
        assert_eq!(f.bytes(&changed), expected);
        assert_eq!(
            f.bytes(&snapshot),
            (0..1024).map(|i| (i % 251) as u8).collect::<Vec<_>>()
        );
        assert!(changed.height() < 64);
        assert_eq!(fs::read_dir(&f.directory).unwrap().count(), 0);
    }
}
