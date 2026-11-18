//! The inode values one mark has read, kept by runs of consecutive ordinals.
//!
//! A pooled leaf names its values by ordinal, and a leaf written by a later
//! Commit still names the ordinals of every value it did not change. A walk
//! over all retained Commits therefore asks for the same ordinals again and
//! again, scattered over the blocks they were first stored in. The engine
//! decodes a whole block to answer one ordinal of it, so every value of each
//! block read is kept here: the next leaf that names a neighbour is answered
//! without a statement.
//!
//! Kept in a submodule so `objects.rs` stays within the product source-line
//! ceiling.
use crate::backend::Mechanics;
use layerfs_content::inode_leaf::INODE_VALUE_BYTES;
use layerfs_storage::{location::InodeValueRow, StorageError};
use std::collections::BTreeMap;

/// Ordinals past a leaf's own that one inode value read may bring along.
pub(super) const AHEAD_VALUES: u32 = 4096;
/// Inode values kept between reads, in two generations: when the young one
/// reaches half of this, the old one is dropped.
pub(super) const HELD_VALUES: usize = 262_144;

/// One inode value.
type Value = [u8; INODE_VALUE_BYTES];

/// Values by run: the first ordinal of a run, then the values of that
/// ordinal and its successors.
type Runs = BTreeMap<u32, Vec<Value>>;

/// The values a mark holds between reads.
///
/// A value is written once under its ordinal and never changes, so two runs
/// that overlap agree wherever they do, and either may answer.
#[derive(Default)]
pub(super) struct HeldValues {
    young: Runs,
    old: Runs,
    young_values: usize,
}

/// The value of `ordinal` in `runs`: the last run starting at or before it is
/// the only one [`HeldValues::keep`] leaves that can hold it.
fn held(runs: &Runs, ordinal: u32) -> Option<&Value> {
    let (first, values) = runs.range(..=ordinal).next_back()?;
    values.get((ordinal - first) as usize)
}

impl HeldValues {
    fn get(&self, ordinal: u32) -> Option<&Value> {
        held(&self.young, ordinal).or_else(|| held(&self.old, ordinal))
    }

    /// Keeps `rows`, which ascend by ordinal, as runs of consecutive ordinals.
    fn keep(&mut self, rows: &[InodeValueRow]) {
        let mut rest = rows;
        while let Some(first) = rest.first().map(|row| row.ordinal) {
            let length = rest
                .iter()
                .zip(first..=u32::MAX)
                .take_while(|(row, ordinal)| row.ordinal == *ordinal)
                .count();
            let (run, tail) = rest.split_at(length);
            rest = tail;
            if self.young_values + length > HELD_VALUES / 2 {
                self.old = std::mem::take(&mut self.young);
                self.young_values = 0;
            }
            // A run starting inside the new one would answer for ordinals
            // past its own end and hide this one.
            let last = first + (length as u32 - 1);
            let inside: Vec<u32> = self.young.range(first..=last).map(|(at, _)| *at).collect();
            for at in inside {
                if let Some(values) = self.young.remove(&at) {
                    self.young_values -= values.len();
                }
            }
            self.young_values += length;
            self.young
                .insert(first, run.iter().map(|row| row.value).collect());
        }
    }

    /// The values of `ordinals`, in the order asked: the held ones, after one
    /// engine read for the blocks of those that are not.
    pub(super) fn values(
        &mut self,
        mechanics: &dyn Mechanics,
        ordinals: &[u32],
    ) -> Result<Vec<Value>, StorageError> {
        let mut missing: Vec<u32> = ordinals
            .iter()
            .copied()
            .filter(|ordinal| self.get(*ordinal).is_none())
            .collect();
        if !missing.is_empty() {
            missing.sort_unstable();
            missing.dedup();
            let mut rows = Vec::new();
            mechanics
                .inode_values_ahead(&missing, AHEAD_VALUES, &mut rows)
                .map_err(StorageError::from)?;
            // The answer is taken from the rows just read, so it does not
            // depend on what the bound lets this hold.
            let read = |ordinal: u32| {
                rows.binary_search_by_key(&ordinal, |row| row.ordinal)
                    .ok()
                    .map(|at| rows[at].value)
            };
            let answer = ordinals
                .iter()
                .map(|ordinal| {
                    read(*ordinal)
                        .or_else(|| self.get(*ordinal).copied())
                        .ok_or(StorageError::Integrity("mark inode value"))
                })
                .collect();
            self.keep(&rows);
            return answer;
        }
        ordinals
            .iter()
            .map(|ordinal| {
                self.get(*ordinal)
                    .copied()
                    .ok_or(StorageError::Integrity("mark inode value"))
            })
            .collect()
    }
}
