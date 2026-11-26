//! Bounded lookahead over stored inode leaves already named by a branch.
use crate::error::{ContentError, ContentResult};
use crate::object::ObjectId;

use super::{
    format::{Format, WireEntry},
    merge::Changes,
    page::{BatchChildren, Engine, ReadPage},
};

/// The captured Commit may inspect one bounded window of changed records.
const CHANGES: usize = 1_024;
/// Sixty-four 8 KiB leaf pages plus their IDs, one decode slot and the
/// 1,024 inline change records fit below 1 MiB. This uses the existing
/// sorted scratch budget and holds no sibling prefetch across branches.
const LEAVES: usize = 64;

#[derive(Default)]
pub(super) struct Leaves {
    retained: BatchChildren,
}

impl Leaves {
    #[allow(clippy::type_complexity)]
    pub(super) fn read<F, I>(
        &mut self,
        engine: &mut Engine<'_, '_, F>,
        id: ObjectId,
        former: &[WireEntry<F::Key, F::Value>],
        bound: Option<&F::Key>,
        changes: &mut Changes<I, F::Key, F::Value>,
    ) -> ContentResult<ReadPage<F::Key, F::Value>>
    where
        F: Format,
        I: Iterator<Item = ContentResult<(F::Key, Option<F::Value>)>>,
    {
        if self.retained.1.is_empty() && former.len() > 1 {
            let mut keys = changes.keys_ahead(CHANGES).peekable();
            let mut ids = Vec::new();
            for (index, row) in former.iter().enumerate() {
                let limit = if index + 1 == former.len() {
                    bound
                } else {
                    Some(&row.key)
                };
                let within = |key: &&F::Key| limit.is_none_or(|limit| *key <= limit);
                if !keys.peek().is_some_and(within) {
                    continue;
                }
                ids.push(row.child.ok_or(ContentError::InvalidRecord("branch row"))?);
                if ids.len() == LEAVES {
                    break;
                }
                while keys.next_if(within).is_some() {}
                if keys.peek().is_none() {
                    break;
                }
            }
            if ids.len() > 1 {
                self.retained = engine.batch_children(ids.len(), &mut |at| Ok(ids[at]))?;
                if !self.retained.1.is_empty() {
                    engine.work.prefetched_waves += 1;
                    engine.work.prefetched_bytes += self
                        .retained
                        .1
                        .iter()
                        .map(|(_, bytes)| bytes.len() as u64)
                        .sum::<u64>();
                }
            }
        }
        let (_, pages, lease) = &mut self.retained;
        if let Some((_, bytes)) = pages.iter().find(|(known, _)| *known == id) {
            engine.work.prefetched_consumed += bytes.len() as u64;
            engine.fetched(id, pages, lease)
        } else {
            engine.read(id, false)
        }
    }
}
