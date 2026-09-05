use layerfs_content::file::rope::FileStateRoot;
use layerfs_layerstack_store::{Result, StoreError};
use std::sync::Arc;

pub(crate) const MAX_EDITS_PER_FILE: u32 = 4_096;
pub(crate) const MAX_PIECES_PER_FILE: usize = 8_193;
pub(crate) const MAX_INLINE_PER_EDIT: usize = 1024 * 1024;
pub(crate) const MAX_INLINE_PER_WORKSPACE: u64 = 8 * 1024 * 1024;
pub(crate) const MAX_PIECE_ALLOCATION: u64 = 2 * 1024 * 1024;
pub(crate) const MAX_RESULT_BYTES: u64 = 1024 * 1024 * 1024 * 1024;
pub(crate) const MAX_LOGICAL_ZERO_BYTES: u64 = 1024 * 1024 * 1024;
pub(crate) const MAX_PREDICTED_ZERO_EXTENTS: u64 = 131_072;

pub(crate) fn check_logical_allocation_charge(bytes: u64) -> Result<()> {
    if bytes <= MAX_PIECE_ALLOCATION {
        Ok(())
    } else {
        Err(StoreError::InvalidInput("workspace piece allocation limit"))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Piece {
    Base {
        root: FileStateRoot,
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
        offset: u64,
        len: u64,
    },
}

impl Piece {
    pub(crate) fn len(&self) -> u64 {
        match self {
            Self::Base { len, .. }
            | Self::Inline { len, .. }
            | Self::Zero { len }
            | Self::Spool { len, .. } => *len,
        }
    }

    fn slice(&self, start: u64, len: u64) -> Result<Self> {
        if len == 0 || start.checked_add(len).is_none_or(|end| end > self.len()) {
            return Err(StoreError::Integrity("piece slice"));
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
            Self::Spool { offset, .. } => Self::Spool {
                offset: offset + start,
                len,
            },
        })
    }

    pub(crate) fn inline_len(&self) -> u64 {
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
            .ok_or(StoreError::InvalidInput("file length"))?;
        let count = link_count(&left)
            .checked_add(1)
            .and_then(|count| count.checked_add(link_count(&right)))
            .ok_or(StoreError::InvalidInput("piece count"))?;
        let inline_len = link_inline_len(&left)
            .checked_add(piece.inline_len())
            .and_then(|len| len.checked_add(link_inline_len(&right)))
            .ok_or(StoreError::InvalidInput("inline bytes"))?;
        let zero_len = link_zero_len(&left)
            .checked_add(
                matches!(piece, Piece::Zero { .. })
                    .then(|| piece.len())
                    .unwrap_or(0),
            )
            .and_then(|len| len.checked_add(link_zero_len(&right)))
            .ok_or(StoreError::InvalidInput("logical zero bytes"))?;
        let spool_len = link_spool_len(&left)
            .checked_add(
                matches!(piece, Piece::Spool { .. })
                    .then(|| piece.len())
                    .unwrap_or(0),
            )
            .and_then(|len| len.checked_add(link_spool_len(&right)))
            .ok_or(StoreError::InvalidInput("spool bytes"))?;
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
pub(crate) struct PieceTree {
    root: Link,
    serial: u64,
    // A complete, contiguous spool beginning at zero needs only its length.
    // Keep this record inline; allocate tree nodes only for fragmented edits.
    contiguous_spool_len: u64,
}

impl PieceTree {
    pub(crate) fn empty() -> Self {
        Self {
            root: None,
            serial: 0,
            contiguous_spool_len: 0,
        }
    }

    pub(crate) fn base(root: FileStateRoot, len: u64) -> Result<Self> {
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

    pub(crate) fn len(&self) -> u64 {
        self.contiguous_spool_len + link_len(&self.root)
    }

    pub(crate) fn count(&self) -> usize {
        usize::from(self.contiguous_spool_len != 0) + link_count(&self.root)
    }

    pub(crate) fn inline_len(&self) -> u64 {
        link_inline_len(&self.root)
    }

    pub(crate) fn height(&self) -> usize {
        usize::from(self.contiguous_spool_len != 0).max(link_height(&self.root))
    }

    pub(crate) fn spool_len(&self) -> u64 {
        self.contiguous_spool_len + link_spool_len(&self.root)
    }

    pub(crate) fn logical_allocation_charge(&self) -> Result<u64> {
        if self.contiguous_spool_len != 0 {
            return Ok(std::mem::size_of::<u64>() as u64);
        }
        (self.count() as u64)
            .checked_mul(std::mem::size_of::<PieceNode>() as u64)
            .ok_or(StoreError::InvalidInput("piece allocation charge"))
    }

    pub(crate) fn replace(
        &self,
        start: u64,
        delete_len: u64,
        replacement: impl IntoIterator<Item = Piece>,
    ) -> Result<Self> {
        let end = start
            .checked_add(delete_len)
            .ok_or(StoreError::InvalidInput("file range"))?;
        if end > self.len() {
            return Err(StoreError::InvalidInput("file range"));
        }
        let mut replacement = replacement.into_iter();
        let first = replacement.next();
        let mut replacement = replacement.peekable();
        if self.root.is_none() && start == self.contiguous_spool_len && delete_len == 0 {
            if let Some(Piece::Spool { offset, len }) = &first {
                if *offset == self.contiguous_spool_len && replacement.peek().is_none() {
                    let len = offset
                        .checked_add(*len)
                        .filter(|len| *len <= MAX_RESULT_BYTES)
                        .ok_or(StoreError::InvalidInput("workspace piece limit"))?;
                    // One contiguous spool has no inline/zero bytes and costs
                    // at most one length record, regardless of write count.
                    let mut next = self.clone();
                    next.contiguous_spool_len = len;
                    check_logical_allocation_charge(next.logical_allocation_charge()?)?;
                    return Ok(next);
                }
            }
        }
        let mut next = self.clone();
        if next.contiguous_spool_len != 0 {
            let piece = Piece::Spool {
                offset: 0,
                len: next.contiguous_spool_len,
            };
            let priority = next.priority()?;
            next.root = Some(PieceNode::new(piece, priority, None, None)?);
            next.contiguous_spool_len = 0;
        }
        let root = next.root.clone();
        let (left, tail) = split(&root, start, &mut next)?;
        let (_, right) = split(&tail, delete_len, &mut next)?;
        let mut middle = None;
        for piece in first.into_iter().chain(replacement) {
            if piece.len() == 0 {
                continue;
            }
            let priority = next.priority()?;
            let node = Some(PieceNode::new(piece, priority, None, None)?);
            middle = merge(&middle, &node)?;
        }
        next.root = merge(&merge(&left, &middle)?, &right)?;
        if let Some(node) = &next.root {
            if node.count == 1 {
                if let Piece::Spool { offset: 0, len } = node.piece {
                    next.contiguous_spool_len = len;
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
            return Err(StoreError::InvalidInput("workspace piece limit"));
        }
        Ok(next)
    }

    pub(crate) fn pieces(&self) -> Vec<Piece> {
        if self.contiguous_spool_len != 0 {
            return vec![Piece::Spool {
                offset: 0,
                len: self.contiguous_spool_len,
            }];
        }
        let mut output = Vec::with_capacity(self.count());
        visit(&self.root, &mut |piece, _| output.push(piece.clone()));
        output
    }

    pub(crate) fn range(&self, start: u64, end: u64) -> Result<Vec<Piece>> {
        self.range_inner(start, end).map(|(pieces, _)| pieces)
    }

    fn range_inner(&self, start: u64, end: u64) -> Result<(Vec<Piece>, usize)> {
        if start > end || end > self.len() {
            return Err(StoreError::InvalidInput("file range"));
        }
        if self.contiguous_spool_len != 0 {
            let pieces = if start == end {
                Vec::new()
            } else {
                vec![Piece::Spool {
                    offset: start,
                    len: end - start,
                }]
            };
            let visited = usize::from(start != end);
            layerfs_layerstack_store::note_workspace_commit_tree_visits(visited as u64);
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
        layerfs_layerstack_store::note_workspace_commit_tree_visits(visited as u64);
        Ok((output, visited))
    }

    fn priority(&mut self) -> Result<u64> {
        self.serial = self
            .serial
            .checked_add(1)
            .ok_or(StoreError::InvalidInput("piece serial"))?;
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
            Err(StoreError::InvalidInput("piece split"))
        };
    };
    let left_len = link_len(&node.left);
    let piece_end = left_len
        .checked_add(node.piece.len())
        .ok_or(StoreError::InvalidInput("piece split"))?;
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
    fn contiguous_spool_records_fit_workspace_budget_and_preserve_edit_limits() {
        let mut charge = 0;
        let files = (0..100_000)
            .map(|index| {
                let len = [1_024, 8_192, 49_152][index % 3];
                let tree = PieceTree::empty()
                    .replace(0, 0, [Piece::Spool { offset: 0, len }])
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
            vec![Piece::Spool { offset: 98, len: 8 }]
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
        assert_eq!(edited.contiguous_spool_len, 0);
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
                Piece::Spool { offset: 98, len: 2 },
                Piece::Inline {
                    bytes: Arc::from(&b"edit"[..]),
                    offset: 0,
                    len: 4
                },
                Piece::Spool {
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
                        offset: tree.len() - 1,
                        len: 1
                    },
                    Piece::Spool {
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
                    offset: tree.len(),
                    len: 1,
                }],
            )
            .unwrap();
        assert_eq!(
            overwritten.range(7, 8).unwrap(),
            vec![Piece::Spool {
                offset: tree.len(),
                len: 1
            }]
        );
        assert_eq!(
            tree.range(7, 8).unwrap(),
            vec![Piece::Spool { offset: 7, len: 1 }]
        );
        assert!(tree
            .replace(
                tree.len(),
                0,
                [Piece::Spool {
                    offset: tree.len(),
                    len: MAX_RESULT_BYTES,
                }]
            )
            .is_err());
    }

    #[test]
    fn implicit_piece_tree_splices_without_rekeying_later_pieces() {
        let root = FileStateRoot(ObjectId::for_bytes(b"base"));
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
        let root = FileStateRoot(ObjectId::for_bytes(b"fragmented-base"));
        let mut tree = PieceTree::base(root, 8_193).unwrap();
        for offset in (1..8_192).step_by(2) {
            tree = tree.replace(offset, 1, [Piece::Zero { len: 1 }]).unwrap();
        }
        assert_eq!(tree.count(), MAX_PIECES_PER_FILE);
        assert!(tree.logical_allocation_charge().unwrap() <= MAX_PIECE_ALLOCATION);
        assert!(depth(&tree.root) < 64, "depth={}", depth(&tree.root));
        for offset in [0, 4_096, 8_192] {
            let (pieces, visited) = tree.range_inner(offset, offset + 1).unwrap();
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
        let root = FileStateRoot(ObjectId::for_bytes(b"large-base"));
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
}
