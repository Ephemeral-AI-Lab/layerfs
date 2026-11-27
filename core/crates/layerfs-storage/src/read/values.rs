//! Stored inode values a demand finds, absent ones left out (#818 H5).

use super::Fetch;
use crate::{
    error::{StorageError, StorageResult},
    policy::READ_OBJECT_LIMIT,
};
use layerfs_content::inode_leaf::INODE_VALUE_BYTES;
use std::collections::{BTreeMap, BTreeSet};

impl Fetch {
    /// The stored values of `ordinals` found (held, published, then looked
    /// up); one with no stored value is absent ([`Source::inode_values`]).
    pub(crate) fn inode_values_found(
        &self,
        ordinals: &[u32],
    ) -> StorageResult<BTreeMap<u32, [u8; INODE_VALUE_BYTES]>> {
        if ordinals.contains(&0) {
            return Err(StorageError::Integrity("metadata ordinal"));
        }
        let wanted: Vec<u32> = ordinals
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut found = BTreeMap::new();
        // Held values first: the Store does not have them yet.
        let wanted: Vec<u32> = wanted
            .into_iter()
            .filter(|ordinal| match self.held_value(*ordinal) {
                Some(value) => {
                    found.insert(*ordinal, value);
                    false
                }
                None => true,
            })
            .collect();
        let wanted: Vec<u32> = if self.serve_published.get() {
            let published = self.published.borrow();
            wanted
                .into_iter()
                .filter(|ordinal| match published.values.get(ordinal) {
                    Some(value) => {
                        found.insert(*ordinal, *value);
                        false
                    }
                    None => true,
                })
                .collect()
        } else {
            wanted
        };
        // A paired read answers exactly the two pages `chunks` makes below,
        // and its second half stays within one provider page.
        const _: () = assert!(
            crate::policy::POOLED_VALUE_CACHE_BYTES / INODE_VALUE_BYTES <= 2 * READ_OBJECT_LIMIT
        );
        let mut batches = [Vec::new(), Vec::new()];
        let paired = if self.paired_inode_values.get()
            && wanted.len() > READ_OBJECT_LIMIT
            && wanted.len() <= crate::policy::POOLED_VALUE_CACHE_BYTES / INODE_VALUE_BYTES
        {
            let (first, last) = wanted.split_at(READ_OBJECT_LIMIT);
            self.metadata
                .inode_values_pair([first, last], &mut batches)?
        } else {
            false
        };
        let mut ordinary = Vec::new();
        for (index, page) in wanted.chunks(READ_OBJECT_LIMIT).enumerate() {
            self.note(|c| c.value_groups += 1);
            let rows = if paired {
                &batches[index]
            } else {
                self.metadata.inode_values(page, &mut ordinary)?;
                &ordinary
            };
            // The reply may omit an ordinal but never add, repeat or reorder one.
            let mut asked = page.iter();
            for row in rows {
                if !asked.any(|ordinal| *ordinal == row.ordinal) {
                    return Err(StorageError::Integrity("unrequested inode value"));
                }
                found.insert(row.ordinal, row.value);
            }
            // A stored ordinal's value never changes: values a Save (or a
            // reader continuing one) read are kept beside the ones it
            // published, within the same bound, so the next Save of this
            // handle and a Commit's logical count read the base leaf's
            // values once (#768 stage 2).
            if self.serve_published.get() {
                let mut published = self.published.borrow_mut();
                for row in rows {
                    published.value(row.ordinal, row.value);
                }
            }
        }
        Ok(found)
    }
}
