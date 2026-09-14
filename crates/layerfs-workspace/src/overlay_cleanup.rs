//! Bounded removal of unreachable directory masks after final inode retirement.
use super::{decode_sequence, ChangeKey, Mutation, Overlay, BINDING, CHANGE_KEY};
use crate::overlay_index::{Index, Root};
use layerfs_layerstack_store::{Result, StoreError};
use layerfs_workspace_core::NodeId;
use std::ops::Bound;

const DELETED_DIRECTORY: u8 = 11;
const ROWS_PER_STEP: usize = 128;

fn next_directory(index: &Index, root: &Root) -> Result<Option<(Vec<u8>, NodeId)>> {
    let rows = index.scan(
        root,
        Bound::Included(&[DELETED_DIRECTORY]),
        Bound::Excluded(&[DELETED_DIRECTORY + 1]),
        1,
    )?;
    rows.into_iter()
        .next()
        .map(|(key, value)| {
            if key.len() != 9 || !value.is_empty() {
                return Err(StoreError::Integrity("deleted directory queue"));
            }
            let node = NodeId(decode_sequence(&key[1..])?);
            if node.0 == 0 {
                return Err(StoreError::Integrity("deleted directory identity"));
            }
            Ok((key, node))
        })
        .transpose()
}

impl Overlay {
    pub(crate) fn directory_cleanup_pending(&self) -> Result<bool> {
        Ok(next_directory(&self.index, &self.acquire()?.index)?.is_some())
    }
}

impl Mutation<'_> {
    pub(super) fn enqueue_deleted_directory(&mut self, node: NodeId) -> Result<()> {
        let key = [vec![DELETED_DIRECTORY], node.0.to_be_bytes().to_vec()].concat();
        self.candidate.index = self.index.set(&self.candidate.index, &key, &[])?;
        Ok(())
    }

    /// Directory identities are never reused. Prove absence in this leased
    /// candidate before retiring masks and their now-unreachable dirty effects.
    /// The surviving parent's deletion and moved-out inode effects stay intact.
    /// Every write is private until installation; an I/O failure keeps the old
    /// root and queue. Count all index-row removals, including both change keys.
    pub(crate) fn cleanup_deleted_directory(&mut self) -> Result<usize> {
        let Some((queue_key, node)) = next_directory(self.index, &self.candidate.index)? else {
            return Ok(0);
        };
        if self
            .index
            .get(&self.candidate.index, &ChangeKey::Inode(node).encode()?)?
            .is_some()
        {
            return Err(StoreError::Integrity("queued directory still live"));
        }
        let mut start = vec![BINDING];
        start.extend_from_slice(&node.0.to_be_bytes());
        let stop = if node.0 == u64::MAX {
            vec![BINDING + 1]
        } else {
            [vec![BINDING], (node.0 + 1).to_be_bytes().to_vec()].concat()
        };
        let rows = self.index.scan(
            &self.candidate.index,
            Bound::Included(&start),
            Bound::Excluded(&stop),
            ROWS_PER_STEP,
        )?;
        let mut removed = 0;
        let mut processed = 0;
        for (key, value) in &rows {
            if decode_sequence(value)? != 0 {
                return Err(StoreError::Integrity("deleted directory has live binding"));
            }
            let change_key = [vec![CHANGE_KEY], key.clone()].concat();
            let sequence = self.index.get(&self.candidate.index, &change_key)?;
            let cost = 1 + 2 * usize::from(sequence.is_some());
            if removed + cost > ROWS_PER_STEP {
                break;
            }
            self.candidate.index = self.index.remove(&self.candidate.index, key)?;
            if let Some(sequence) = sequence {
                self.forget_covered(&ChangeKey::decode(key)?, decode_sequence(&sequence)?)?;
            }
            removed += cost;
            processed += 1;
        }
        if rows.len() < ROWS_PER_STEP && processed == rows.len() && removed < ROWS_PER_STEP {
            self.candidate.index = self.index.remove(&self.candidate.index, &queue_key)?;
            removed += 1;
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use crate::host_overlay::tests::Fixture;
    use crate::overlay::ChangeKey;
    use layerfs_workspace_core::{ResourcePolicy, ROOT};

    #[test]
    fn cleanup_counts_every_removed_row_and_keeps_live_base_masks() {
        let fixture = Fixture::new(
            |path| {
                std::fs::write(path.join("required-mask"), b"canonical").unwrap();
            },
            ResourcePolicy::default(),
        );
        let host = &fixture.host;
        host.unlink(ROOT, b"required-mask", false).unwrap();
        let directory = host.mkdir(ROOT, b"gone", 0o700).unwrap().node;
        host.overlay
            .mutate(|m| {
                for n in 0..129 {
                    m.put_binding(directory, format!("mask{n:03}").as_bytes(), None)?;
                }
                Ok(())
            })
            .unwrap();
        host.unlink(ROOT, b"gone", true).unwrap();
        let before = host.snapshot().unwrap();
        let sequence = before.root.sequence;
        let mut steps = 0;
        while host.overlay.directory_cleanup_pending().unwrap() {
            let removed = host.mutate(|m| m.cleanup_deleted_directory()).unwrap();
            assert!((1..=super::ROWS_PER_STEP).contains(&removed));
            steps += 1;
            if steps == 1 {
                assert_eq!(removed, 126, "42 masks each own two change rows");
                assert_eq!(
                    host.snapshot()
                        .unwrap()
                        .bindings(directory, None)
                        .unwrap()
                        .len(),
                    87
                );
            }
            assert!(steps <= 4);
        }
        let after = host.snapshot().unwrap();
        assert_eq!(steps, 4);
        assert_eq!(after.root.sequence, sequence);
        assert!(after.bindings(directory, None).unwrap().is_empty());
        assert_eq!(after.binding(ROOT, b"required-mask").unwrap(), Some(None));
        assert!(host.lookup(ROOT, b"required-mask").is_err());
        assert_eq!(before.bindings(directory, None).unwrap().len(), 128);
        assert_eq!(
            before.bindings(directory, Some(b"mask127")).unwrap().len(),
            1
        );
    }

    #[test]
    fn deleted_directory_masks_retire_without_changing_retained_snapshots() {
        let fixture = Fixture::new(|_| {}, ResourcePolicy::default());
        let host = &fixture.host;
        let directory = host.mkdir(ROOT, b"removed", 0o700).unwrap().node;
        let mut sample = None;
        for n in 0..64 {
            let name = format!("child{n:02}");
            let node = host
                .create_file(directory, name.as_bytes(), 0o600)
                .unwrap()
                .node;
            if n == 0 {
                host.write(node, 0, b"retained").unwrap();
                sample = Some(node);
            }
        }
        let before = host.snapshot().unwrap();
        for n in 0..64 {
            host.unlink(directory, format!("child{n:02}").as_bytes(), false)
                .unwrap();
        }
        host.unlink(ROOT, b"removed", true).unwrap();
        let removed = host.snapshot().unwrap();
        assert!(removed.inode_record(directory).unwrap().is_none());
        assert_eq!(removed.bindings(directory, None).unwrap().len(), 64);

        // Clearing covered dirty bookkeeping must not conceal the independently
        // owned unreachable masks. Keep the surviving-parent removal pending.
        let mut after = None;
        loop {
            let page = removed.changes(0, after.as_deref()).unwrap();
            if page.is_empty() {
                break;
            }
            after = page.last().map(|(key, _, _)| key.clone());
            host.overlay
                .mutate(|m| {
                    for (_, sequence, key) in &page {
                        if matches!(key, ChangeKey::Binding(parent, _) if *parent == directory) {
                            m.forget_covered(key, *sequence)?;
                        }
                    }
                    Ok(())
                })
                .unwrap();
        }
        let mut steps = 0;
        while host.maintain().unwrap() {
            steps += 1;
            assert!(steps < 20_000, "bounded release work did not drain");
        }
        let current = host.snapshot().unwrap();
        assert!(
            current.bindings(directory, None).unwrap().is_empty(),
            "removed parent retained unreachable binding masks"
        );
        assert_eq!(current.root.sequence, removed.root.sequence);
        assert_eq!(current.binding(ROOT, b"removed").unwrap(), Some(None));
        let mut after = None;
        let mut parent_removal = false;
        loop {
            let page = current.changes(0, after.as_deref()).unwrap();
            if page.is_empty() {
                break;
            }
            after = page.last().map(|(key, _, _)| key.clone());
            for (_, _, key) in page {
                assert!(!matches!(&key, ChangeKey::Binding(parent, _) if *parent == directory));
                parent_removal |= key == ChangeKey::Binding(ROOT, b"removed".to_vec());
            }
        }
        assert!(
            parent_removal,
            "cleanup removed the surviving namespace effect"
        );
        assert_eq!(before.bindings(directory, None).unwrap().len(), 64);
        assert_eq!(removed.bindings(directory, None).unwrap().len(), 64);
        let mut bytes = [0; 8];
        assert_eq!(
            before
                .read_at(&host.ranges, &host.reader, sample.unwrap(), 0, &mut bytes)
                .unwrap(),
            8
        );
        assert_eq!(&bytes, b"retained");
        println!("deleted-parent cleanup PASS:64 masks, covered keys, retained roots/payload, surviving parent removal");
    }
}
