//! The lists one mark keeps of numbers: the packs it retains, the inode
//! value ordinals it read.
//!
//! Neither is held. Pack ids and ordinals are drawn from counters the Store
//! reserves in ranges, so the numbers kept lie far apart and a bitmap over
//! them would follow the highest number ever reserved, not what was kept. A
//! list here is a sorter of the mark's sort space ([`super::spill`]): a
//! bounded buffer, sorted runs in an unnamed file past it, and a merged read
//! in ascending order as often as it is asked for.
//!
//! A reached inode leaf stores ordinals, and its values live in
//! `inode_value_block` under them. Reading a leaf asks for the ordinals of
//! every body of its delta chain, and every member of that chain is itself
//! reached, so the ordinals the walk asked its source for are exactly the
//! ones any later read of a kept leaf can ask for. A rebuild keeps the blocks
//! that hold one of them and leaves the rest behind.
use super::records::{self, Number, Span};
use super::sort::{Sorter, Space};
use layerfs_history::error::{HistoryError, HistoryResult};
use std::sync::{Arc, Mutex};

/// Bytes of a list held before it writes a run.
const LIST_BYTES: usize = 1024 * 1024;

/// Numbers in ascending order, each once: the packs a mark retains.
#[derive(Clone)]
pub struct Sequence {
    numbers: Arc<Mutex<Sorter<Number>>>,
    count: u64,
    last: Option<u64>,
}

impl Sequence {
    /// An empty list whose file, if it needs one, is in `space`.
    pub fn new(space: &Arc<Space>) -> Self {
        Self {
            numbers: Arc::new(Mutex::new(Sorter::new(
                space,
                LIST_BYTES,
                records::numbers_ascending,
            ))),
            count: 0,
            last: None,
        }
    }

    /// Adds `number`, which must follow every number added before it.
    pub fn push(&mut self, number: u64) -> HistoryResult<()> {
        if self.last.is_some_and(|last| last >= number) {
            return Err(HistoryError::Integrity("mark list order"));
        }
        self.last = Some(number);
        self.count += 1;
        self.numbers
            .lock()
            .map_err(|_| HistoryError::Integrity("mark list"))?
            .push(Number(number))
    }

    /// How many numbers the list has.
    pub fn len(&self) -> u64 {
        self.count
    }

    /// Whether the list has no number.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Hands every number to `each`, ascending. May be read again.
    pub fn each(&self, each: &mut dyn FnMut(u64) -> HistoryResult<()>) -> HistoryResult<()> {
        let mut numbers = self
            .numbers
            .lock()
            .map_err(|_| HistoryError::Integrity("mark list"))?;
        let mut sorted = numbers.sorted()?;
        while let Some(number) = sorted.next()? {
            each(number.0)?;
        }
        Ok(())
    }
}

impl std::fmt::Debug for Sequence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sequence")
            .field("count", &self.count)
            .finish()
    }
}

/// Inclusive runs of numbers, noted in any order and as often as they come:
/// the inode value ordinals a mark read.
#[derive(Clone)]
pub struct Spans {
    spans: Arc<Mutex<Sorter<Span>>>,
}

impl Spans {
    /// An empty list whose file, if it needs one, is in `space`.
    pub fn new(space: &Arc<Space>) -> Self {
        Self::of(Sorter::new(space, LIST_BYTES, records::spans_by_first))
    }

    pub(in crate::gc) fn sorter(space: &Arc<Space>) -> Sorter<Span> {
        Sorter::new(space, LIST_BYTES, records::spans_by_first)
    }

    pub(in crate::gc) fn of(spans: Sorter<Span>) -> Self {
        Self {
            spans: Arc::new(Mutex::new(spans)),
        }
    }

    /// Notes the run `first..=last`.
    pub fn push(&mut self, first: u32, last: u32) -> HistoryResult<()> {
        self.spans
            .lock()
            .map_err(|_| HistoryError::Integrity("mark list"))?
            .push(Span { first, last })
    }

    /// Hands the numbers to `each` as inclusive runs, ascending and never
    /// overlapping or adjacent. May be read again.
    pub fn each(&self, each: &mut dyn FnMut(u32, u32) -> HistoryResult<()>) -> HistoryResult<()> {
        let mut spans = self
            .spans
            .lock()
            .map_err(|_| HistoryError::Integrity("mark list"))?;
        let mut sorted = spans.sorted()?;
        let mut open: Option<Span> = None;
        while let Some(next) = sorted.next()? {
            open = Some(match open {
                // The run reaches the next one or the number before it.
                Some(run) if u64::from(next.first) <= u64::from(run.last) + 1 => Span {
                    first: run.first,
                    last: run.last.max(next.last),
                },
                Some(run) => {
                    each(run.first, run.last)?;
                    next
                }
                None => next,
            });
        }
        match open {
            Some(run) => each(run.first, run.last),
            None => Ok(()),
        }
    }
}

impl std::fmt::Debug for Spans {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Spans").finish_non_exhaustive()
    }
}

/// Notes `ordinals` in `spans` as runs of consecutive numbers, without
/// writing: the caller relieves the sorter later.
pub(in crate::gc) fn hold_runs(spans: &mut Sorter<Span>, ordinals: &[u32]) {
    let mut open: Option<Span> = None;
    for ordinal in ordinals {
        open = Some(match open {
            Some(run) if run.last.checked_add(1) == Some(*ordinal) => Span {
                first: run.first,
                last: *ordinal,
            },
            Some(run) => {
                spans.hold(run);
                Span {
                    first: *ordinal,
                    last: *ordinal,
                }
            }
            None => Span {
                first: *ordinal,
                last: *ordinal,
            },
        });
    }
    if let Some(run) = open {
        spans.hold(run);
    }
}
