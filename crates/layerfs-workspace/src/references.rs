//! Minimal binding facts retained before unlinked nodes are reclaimed.
use crate::commit_spool::{Run, Sorter};
use crate::cow_tree::{NodeId, Workspace};
use layerfs_content::tree::inode::InodeId;
use layerfs_layerstack_store::{Result, StoreError};
use std::path::Path;

const RECORD: usize = 41;
const BUFFER: usize = 1024;
pub(crate) const MEMORY: u64 = (RECORD * BUFFER) as u64;
#[derive(Default)]
pub(crate) struct References {
    pending: Vec<[u8; RECORD]>,
    raw: Option<Run<RECORD>>,
    pub(crate) events: u64,
    pub(crate) bookkeeping_ns: u64,
}
impl References {
    pub(crate) fn capacity(&self) -> u64 {
        (self.pending.capacity() * RECORD) as u64
    }
    pub(crate) fn bytes(&self) -> u64 {
        (self.raw.as_ref().map_or(0, |r| r.count) + self.pending.len() as u64) * RECORD as u64
    }
    pub(crate) fn reserve(&mut self, dir: &Path, disk_limit: u64, memory_limit: u64) -> Result<()> {
        if self
            .bytes()
            .checked_add(RECORD as u64)
            .is_none_or(|n| n > disk_limit)
        {
            return Err(StoreError::InvalidInput("workspace spool limit"));
        }
        let capacity = BUFFER.min((memory_limit / RECORD as u64) as usize);
        if capacity == 0 {
            return Err(StoreError::InvalidInput("workspace final-delta limit"));
        }
        if self.pending.capacity() == 0 {
            self.pending
                .try_reserve_exact(capacity)
                .map_err(|_| StoreError::InvalidInput("workspace reference allocation"))?;
        }
        if self.pending.len() == self.pending.capacity() {
            let raw = match &mut self.raw {
                Some(raw) => raw,
                None => self.raw.insert(Run::create(dir)?),
            };
            // Reserve and append before changing a binding. Roll back the full
            // append on error so retry never counts completed facts twice.
            let before = raw.count;
            for record in &self.pending {
                if let Err(error) = raw.push(record) {
                    raw.truncate(before)?;
                    return Err(error);
                }
            }
            self.pending.clear();
        }
        Ok(())
    }
    pub(crate) fn push(&mut self, inode: Option<InodeId>, node: NodeId, delta: i64) {
        debug_assert!(self.pending.len() < self.pending.capacity());
        let mut record = [0; RECORD];
        match inode {
            Some(inode) => {
                record[0] = 0;
                record[1..33].copy_from_slice(inode.as_bytes());
            }
            None => {
                record[0] = 1;
                record[25..33].copy_from_slice(&node.0.to_be_bytes());
            }
        }
        record[33..].copy_from_slice(&delta.to_be_bytes());
        match self
            .pending
            .binary_search_by(|previous| previous[..33].cmp(&record[..33]))
        {
            Ok(index) => {
                let net = super::references::delta(&self.pending[index])
                    .checked_add(delta)
                    .expect("reserved namespace delta");
                if net == 0 {
                    self.pending.remove(index);
                } else {
                    self.pending[index][33..].copy_from_slice(&net.to_be_bytes());
                }
            }
            Err(index) => self.pending.insert(index, record),
        }
        self.events += 1;
    }
    pub(crate) fn sorted(
        &mut self,
        dir: &Path,
        disk_limit: u64,
        memory: u64,
    ) -> Result<Run<RECORD>> {
        let mut sorter = Sorter::new(dir, disk_limit, memory)?;
        if let Some(raw) = &mut self.raw {
            raw.rewind()?;
            while let Some(record) = raw.next()? {
                sorter.push(record)?;
            }
        }
        for &record in &self.pending {
            sorter.push(record)?;
        }
        sorter.finish()
    }
    pub(crate) fn existing_delta(&self, inode: InodeId) -> Result<i64> {
        let wanted = key(Some(inode), NodeId(0));
        let mut total = 0i64;
        let mut add = |record: [u8; RECORD]| -> Result<()> {
            if record[..33] == wanted {
                total = total
                    .checked_add(delta(&record))
                    .ok_or(StoreError::Integrity("namespace reference overflow"))?;
            }
            Ok(())
        };
        if let Some(raw) = &self.raw {
            for i in 0..raw.count {
                add(raw.at(i)?)?;
            }
        }
        for &record in &self.pending {
            add(record)?;
        }
        Ok(total)
    }
    pub(crate) fn clear(&mut self) -> Result<()> {
        if let Some(raw) = self.raw.take() {
            raw.remove()?;
        }
        self.pending.clear();
        self.events = 0;
        Ok(())
    }
}

pub(crate) fn key(inode: Option<InodeId>, node: NodeId) -> [u8; 33] {
    let mut key = [0; 33];
    if let Some(inode) = inode {
        key[1..].copy_from_slice(inode.as_bytes());
    } else {
        key[0] = 1;
        key[25..].copy_from_slice(&node.0.to_be_bytes());
    }
    key
}
pub(crate) fn delta(record: &[u8; RECORD]) -> i64 {
    i64::from_be_bytes(record[33..].try_into().unwrap())
}

impl Workspace {
    pub(crate) fn reserve_reference(&mut self) -> Result<()> {
        let started = std::time::Instant::now();
        let result = self.references.reserve(
            &self.spool,
            self.policy.max_spool_bytes.saturating_sub(self.spool_bytes),
            self.policy.max_final_delta_memory_bytes / 8,
        );
        self.references.bookkeeping_ns = self
            .references
            .bookkeeping_ns
            .saturating_add(started.elapsed().as_nanos().min(u64::MAX as u128) as u64);
        result
    }
    pub(crate) fn note_reference(&mut self, node: NodeId, delta: i64) {
        let started = std::time::Instant::now();
        self.nodes.get_mut(&node).unwrap().commit_dirty = true;
        self.references
            .push(self.nodes[&node].canonical, node, delta);
        self.references.bookkeeping_ns = self
            .references
            .bookkeeping_ns
            .saturating_add(started.elapsed().as_nanos().min(u64::MAX as u128) as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn net_changes_coalesce_and_failed_reservation_preserves_facts() {
        let dir =
            std::env::temp_dir().join(format!("layerfs-reference-check-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut refs = References::default();
        let inode = InodeId::allocate([7; 32], 1);
        for _ in 0..10000 {
            refs.reserve(&dir, 4096, 4096).unwrap();
            refs.push(Some(inode), NodeId(2), 1);
            refs.reserve(&dir, 4096, 4096).unwrap();
            refs.push(Some(inode), NodeId(2), -1);
        }
        assert_eq!(refs.bytes(), 0);
        refs.reserve(&dir, 4096, 4096).unwrap();
        refs.push(Some(inode), NodeId(2), -1);
        assert!(refs.reserve(&dir, 0, 4096).is_err());
        assert_eq!(refs.existing_delta(inode).unwrap(), -1);
        refs.clear().unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
