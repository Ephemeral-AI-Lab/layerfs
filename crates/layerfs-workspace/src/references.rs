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
    // A flush is committed only after every pending record was appended.
    // Keep the old extent authoritative if append or rollback truncation fails.
    pending_flush_base: Option<u64>,
    pub(crate) events: u64,
    pub(crate) bookkeeping_ns: u64,
}
impl References {
    pub(crate) fn capacity(&self) -> u64 {
        (self.pending.capacity() * RECORD) as u64
    }
    pub(crate) fn bytes(&self) -> u64 {
        (self.pending_flush_base.unwrap_or_else(|| self.raw.as_ref().map_or(0, |r| r.count)) + self.pending.len() as u64) * RECORD as u64
    }
    pub(crate) fn reserve(&mut self, dir: &Path, disk_limit: u64, memory_limit: u64) -> Result<()> {
        self.repair_failed_flush()?;
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
        if self.pending.len() == self.pending.capacity() { self.flush_pending(dir)?; }
        Ok(())
    }
    fn repair_failed_flush(&mut self) -> Result<()> {
        if let Some(before) = self.pending_flush_base {
            #[cfg(test)]
            if TRUNCATE_FAILURES.with(|n| { let count = n.get(); n.set(count.saturating_sub(1)); count != 0 }) {
                return Err(StoreError::Io(std::io::Error::other("injected reference truncate failure")));
            }
            self.raw.as_mut().ok_or(StoreError::Integrity("reference flush owner"))?.truncate(before)?;
            self.pending_flush_base = None;
        }
        Ok(())
    }

    fn flush_pending(&mut self, dir: &Path) -> Result<()> {
        self.repair_failed_flush()?;
        if self.pending.is_empty() { return Ok(()); }
        if self.raw.is_none() { self.raw = Some(Run::create(dir)?); }
        self.pending_flush_base = Some(self.raw.as_ref().unwrap().count);
        for index in 0..self.pending.len() {
            let record = self.pending[index];
            #[cfg(test)]
            let injected = APPEND_FAILURE_AFTER.with(|remaining| match remaining.get() {
                Some(0) => { remaining.set(None); true }
                Some(n) => { remaining.set(Some(n - 1)); false }
                None => false,
            });
            #[cfg(not(test))]
            let injected = false;
            let appended = if injected {
                Err(StoreError::Io(std::io::Error::other("injected reference append failure")))
            } else { self.raw.as_mut().unwrap().push(&record) };
            if let Err(error) = appended {
                self.repair_failed_flush()?;
                return Err(error);
            }
        }
        self.pending.clear();
        self.pending_flush_base = None;
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
        // Seal mutation facts before allocating Commit's sorter. The raw file
        // remains the retry authority; release the now-empty mutation buffer.
        self.flush_pending(dir)?;
        self.pending = Vec::new();
        let mut sorter = Sorter::new(dir, disk_limit, memory)?;
        if let Some(raw) = &mut self.raw {
            raw.rewind()?;
            while let Some(record) = raw.next()? {
                sorter.push(record)?;
            }
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
            for i in 0..self.pending_flush_base.unwrap_or(raw.count) {
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
        self.pending = Vec::new();
        self.pending_flush_base = None;
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
thread_local! {
    static APPEND_FAILURE_AFTER: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
    static TRUNCATE_FAILURES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_append_and_rollback_keep_authoritative_facts_and_retry_exactly_once() {
        let dir = std::env::temp_dir().join(format!("layerfs-reference-rollback-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut refs = References::default();
        let first = InodeId::allocate([41; 32], 1);
        let second = InodeId::allocate([41; 32], 2);
        for (node, inode) in [(NodeId(2), first), (NodeId(3), second)] {
            refs.reserve(&dir, 4096, 2 * RECORD as u64).unwrap();
            refs.push(Some(inode), node, 1);
        }
        // The first append physically succeeds; the second fails, then rollback
        // fails twice. Neither retry may treat the appended prefix as new facts.
        APPEND_FAILURE_AFTER.with(|n| n.set(Some(1)));
        TRUNCATE_FAILURES.with(|n| n.set(2));
        assert!(refs.reserve(&dir, 4096, 2 * RECORD as u64).is_err());
        assert_eq!(refs.raw.as_ref().unwrap().count, 1);
        assert_eq!(refs.pending_flush_base, Some(0));
        assert_eq!(refs.bytes(), 2 * RECORD as u64);
        assert_eq!(refs.existing_delta(first).unwrap(), 1);
        assert_eq!(refs.existing_delta(second).unwrap(), 1);
        assert!(refs.sorted(&dir, 4096, 4096).is_err());
        assert_eq!(refs.pending.len(), 2);
        let mut sorted = refs.sorted(&dir, 4096, 4096).unwrap();
        assert_eq!(refs.pending_flush_base, None);
        assert_eq!(refs.capacity(), 0, "Commit releases sealed mutation allocation");
        assert_eq!(refs.raw.as_ref().unwrap().count, 2);
        assert_eq!(refs.existing_delta(first).unwrap(), 1);
        assert_eq!(refs.existing_delta(second).unwrap(), 1);
        let mut records = 0;
        while let Some(record) = sorted.next().unwrap() { assert_eq!(delta(&record), 1); records += 1; }
        assert_eq!(records, 2);
        sorted.remove().unwrap(); refs.clear().unwrap();
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        std::fs::remove_dir(dir).unwrap();
    }

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
