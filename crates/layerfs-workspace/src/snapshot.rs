//! Owned readers over host-installed overlay state. FUSE kernel visibility must
//! be resolved by the acquisition adapter before this is a full filesystem cut.
use crate::overlay::InodeRecord;
use crate::overlay::{decode_sequence, ChangeKey, OverlayRoot, CHANGE_BATCH, CHANGE_SEQUENCE};
use crate::overlay::{CANONICAL_INODE, REVERSE_BINDING};
use crate::overlay_index::Index;
use crate::overlay_index::Root;
use layerfs_content::tree::inode::InodeId;
use layerfs_layerstack_store::{Result, StoreError};
use layerfs_workspace_core::NodeId;
use std::ops::Bound;
use std::sync::Arc;

pub(crate) struct Snapshot {
    index: Index,
    pub(crate) root: Arc<OverlayRoot>,
}

impl Snapshot {
    pub(crate) fn new(index: Index, root: Arc<OverlayRoot>) -> Self {
        Self { index, root }
    }

    pub(crate) fn inode(&self, inode: NodeId) -> Result<Option<Vec<u8>>> {
        Ok(self
            .index
            .get(&self.root.index, &ChangeKey::Inode(inode).encode()?)?)
    }

    /// The returned range root is independently owned even if this snapshot
    /// leaves memory before content construction finishes.
    pub(crate) fn inode_record(
        &self,
        inode: NodeId,
    ) -> Result<Option<(InodeRecord, Option<Root>)>> {
        self.index
            .get_linked(&self.root.index, &ChangeKey::Inode(inode).encode()?)?
            .map(|(bytes, ranges)| Ok((InodeRecord::decode(inode, &bytes)?, ranges)))
            .transpose()
    }

    pub(crate) fn file_ranges(
        &self,
        ranges: &crate::overlay_ranges::Ranges,
        inode: NodeId,
    ) -> Result<crate::overlay_ranges::Tree> {
        let (record, root) = self
            .inode_record(inode)?
            .ok_or(StoreError::NotFound("snapshot inode"))?;
        if record.attr.kind != layerfs_workspace_core::Kind::File {
            return Err(StoreError::InvalidInput("snapshot regular file"));
        }
        let tree = ranges.restore(root)?;
        if tree.len() != record.attr.size {
            return Err(StoreError::Integrity("snapshot file length"));
        }
        Ok(tree)
    }

    pub(crate) fn read_at(
        &self,
        ranges: &crate::overlay_ranges::Ranges,
        reader: &layerfs_layerstack_store::SnapshotReader,
        inode: NodeId,
        offset: u64,
        output: &mut [u8],
    ) -> Result<usize> {
        let tree = self.file_ranges(ranges, inode)?;
        let start = offset.min(tree.len());
        let stop = tree.len().min(start.saturating_add(output.len() as u64));
        let mut filled = 0;
        for next in ranges.cursor(&tree, start, stop)? {
            let (position, piece) = next?;
            if position != start + filled as u64 {
                return Err(StoreError::Integrity("snapshot range coverage"));
            }
            let length = usize::try_from(piece.piece.length)
                .map_err(|_| StoreError::Integrity("snapshot range length"))?;
            let target = output
                .get_mut(filled..filled + length)
                .ok_or(StoreError::Integrity("snapshot range output"))?;
            ranges.read_piece(&piece, target, 0, |root, bytes, offset| {
                let len = bytes.len() as u64;
                let mut target = bytes;
                let counters = layerfs_content::file::content::read_range(
                    &layerfs_layerstack_store::CoreReader(reader),
                    layerfs_content::file::content::FileContentRoot(root),
                    offset..offset + len,
                    &mut target,
                )
                .map_err(std::io::Error::other)?;
                if !target.is_empty() {
                    return Err(std::io::Error::other("short canonical snapshot read"));
                }
                reader
                    .note_rope_read(counters)
                    .map_err(std::io::Error::other)
            })?;
            filled += length;
        }
        if filled as u64 != stop - start {
            return Err(StoreError::Integrity("short snapshot read"));
        }
        Ok(filled)
    }

    pub(crate) fn canonical_inode(&self, inode: InodeId) -> Result<Option<NodeId>> {
        let key = [vec![CANONICAL_INODE], inode.as_bytes().to_vec()].concat();
        self.index
            .get(&self.root.index, &key)?
            .map(|bytes| {
                let id = decode_sequence(&bytes)?;
                if id == 0 {
                    return Err(StoreError::Integrity("overlay canonical identity"));
                }
                Ok(NodeId(id))
            })
            .transpose()
    }

    /// Read only this inode's overlay aliases; immutable-base aliases are still
    /// resolved from the retained base namespace when first touched.
    pub(crate) fn aliases(
        &self,
        inode: NodeId,
        after: Option<&[u8]>,
    ) -> Result<Vec<(Vec<u8>, NodeId, Vec<u8>)>> {
        if inode.0 == 0 {
            return Err(StoreError::InvalidInput("overlay inode identity"));
        }
        let mut prefix = vec![REVERSE_BINDING];
        prefix.extend_from_slice(&inode.0.to_be_bytes());
        let mut limit = prefix.clone();
        if inode.0 == u64::MAX {
            limit = vec![REVERSE_BINDING + 1];
        } else {
            limit[1..].copy_from_slice(&(inode.0 + 1).to_be_bytes());
        }
        let start = match after {
            None => Bound::Included(prefix.as_slice()),
            Some(key) if key.starts_with(&prefix) => Bound::Excluded(key),
            Some(_) => return Err(StoreError::InvalidInput("overlay alias cursor")),
        };
        self.index
            .scan(
                &self.root.index,
                start,
                Bound::Excluded(&limit),
                CHANGE_BATCH,
            )?
            .into_iter()
            .map(|(key, value)| {
                if key.len() <= 17 || !value.is_empty() {
                    return Err(StoreError::Integrity("overlay alias row"));
                }
                let parent = NodeId(decode_sequence(&key[9..17])?);
                let name = key[17..].to_vec();
                ChangeKey::Binding(parent, name.clone()).encode()?;
                Ok((key, parent, name))
            })
            .collect()
    }

    /// None means consult immutable backing; Some(None) is a retained mask.
    pub(crate) fn binding(&self, parent: NodeId, name: &[u8]) -> Result<Option<Option<NodeId>>> {
        self.index
            .get(
                &self.root.index,
                &ChangeKey::Binding(parent, name.to_vec()).encode()?,
            )?
            .map(|bytes| {
                let value = decode_sequence(&bytes)?;
                Ok((value != 0).then_some(NodeId(value)))
            })
            .transpose()
    }

    pub(crate) fn bindings(
        &self,
        parent: NodeId,
        after: Option<&[u8]>,
    ) -> Result<Vec<(Vec<u8>, Option<NodeId>)>> {
        let encoded = ChangeKey::Binding(parent, b"x".to_vec()).encode()?;
        let prefix = &encoded[..9];
        let first = after
            .map(|name| ChangeKey::Binding(parent, name.to_vec()).encode())
            .transpose()?;
        let start = match &first {
            Some(key) => Bound::Excluded(key.as_slice()),
            None => Bound::Included(prefix),
        };
        let mut limit = prefix.to_vec();
        if parent.0 == u64::MAX {
            limit = vec![prefix[0] + 1];
        } else {
            limit[1..].copy_from_slice(&(parent.0 + 1).to_be_bytes());
        }
        self.index
            .scan(
                &self.root.index,
                start,
                Bound::Excluded(&limit),
                CHANGE_BATCH,
            )?
            .into_iter()
            .map(|(key, value)| {
                let ChangeKey::Binding(found, name) = ChangeKey::decode(&key)? else {
                    return Err(StoreError::Integrity("overlay binding index"));
                };
                if found != parent {
                    return Err(StoreError::Integrity("overlay binding parent"));
                }
                let node = decode_sequence(&value)?;
                Ok((name, (node != 0).then_some(NodeId(node))))
            })
            .collect()
    }

    /// Each batch seeks past the caller's last key, so covered prefixes and
    /// previously returned pages are not scanned again.
    pub(crate) fn changes(
        &self,
        covered: u64,
        after: Option<&[u8]>,
    ) -> Result<Vec<(Vec<u8>, u64, ChangeKey)>> {
        if covered > self.root.sequence {
            return Err(StoreError::InvalidInput("overlay future coverage"));
        }
        if covered == self.root.sequence {
            return Ok(Vec::new());
        }
        let mut first = vec![CHANGE_SEQUENCE];
        first.extend_from_slice(&(covered + 1).to_be_bytes());
        let start = match after {
            Some(after) if after >= first.as_slice() && after.first() == Some(&CHANGE_SEQUENCE) => {
                Bound::Excluded(after)
            }
            Some(_) => return Err(StoreError::InvalidInput("overlay change cursor")),
            None => Bound::Included(first.as_slice()),
        };
        self.index
            .scan(
                &self.root.index,
                start,
                Bound::Excluded(&[CHANGE_SEQUENCE + 1]),
                CHANGE_BATCH,
            )?
            .into_iter()
            .map(|(key, value)| {
                if key.len() < 18 || !value.is_empty() {
                    return Err(StoreError::Integrity("overlay sequence index"));
                }
                let sequence = decode_sequence(&key[1..9])?;
                if sequence > self.root.sequence {
                    return Err(StoreError::Integrity("overlay future change"));
                }
                let change = ChangeKey::decode(&key[9..])?;
                Ok((key, sequence, change))
            })
            .collect()
    }
}
