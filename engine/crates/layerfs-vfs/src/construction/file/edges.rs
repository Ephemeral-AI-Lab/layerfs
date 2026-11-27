//! Bounded authentication of literal edges around a shifted captured edit.
//! Whole-chunk write recognition and whole-cell pieces leave ragged copied
//! bytes literal. Equal bytes at either edge can still retain their base
//! mapping; mismatching bytes remain replacement content. No piece is created.
use super::{
    owner::Facts,
    scan::{Scan, Supply},
    state::Shared,
};
use crate::{BaseView, OverlayCapturedRuns, OverlayOperationRecords};
use layerfs_content::{ContentError, ContentResult, Edit, FileRun, FileView};
use layerfs_telemetry::timer::Timing;

/// Two maximum chunks cover a copied chunk edge plus cell rounding. This is
/// a work window, never a bound on file size, replacement size or edit count.
const WINDOW: usize = 2 * layerfs_content::file::cdc::MAXIMUM_CHUNK_BYTES;

pub(super) struct Edges<'s, 'a, P: OverlayCapturedRuns + OverlayOperationRecords + ?Sized> {
    provider: &'a P,
    shared: &'s Shared<'a, P>,
    facts: Facts,
    base: Option<&'s BaseView>,
    file: Option<&'s FileView>,
    /// Original provisional edits translate final offsets to immutable base.
    delta: i128,
    original: Vec<u8>,
    captured: Vec<u8>,
}
impl<'s, 'a, P: OverlayCapturedRuns + OverlayOperationRecords + ?Sized> Edges<'s, 'a, P> {
    pub fn new(
        provider: &'a P,
        shared: &'s Shared<'a, P>,
        facts: Facts,
        base: Option<&'s BaseView>,
        file: Option<&'s FileView>,
    ) -> Self {
        Self {
            provider,
            shared,
            facts,
            base,
            file,
            delta: 0,
            original: Vec::new(),
            captured: Vec::new(),
        }
    }
    pub fn trim(&mut self, edit: Edit, scripted: bool) -> ContentResult<Option<Edit>> {
        let delta = self.delta;
        self.delta = delta
            .checked_add(i128::from(edit.replacement_len()) - i128::from(edit.removed_len()))
            .ok_or(ContentError::LengthOverflow)?;
        // In-place scripts and files without pieces keep their existing
        // normalization. Pure inserts/deletes have no shared edge to compare.
        let overlap = edit.removed_len().min(edit.replacement_len());
        if !scripted
            || self.facts.final_size == self.facts.base_size
            || (delta == 0 && edit.removed_len() == edit.replacement_len())
            || overlap == 0
        {
            return Ok(Some(edit));
        }
        let base_start = u64::try_from(i128::from(edit.start()) - delta)
            .map_err(|_| ContentError::LengthOverflow)?;
        let base_end = base_start
            .checked_add(edit.removed_len())
            .filter(|end| *end <= self.facts.base_size)
            .ok_or(ContentError::InvalidRecord("captured edge base range"))?;
        let first = overlap.min(WINDOW as u64) as usize;
        self.window(base_start, edit.start(), first)?;
        let prefix = self
            .original
            .iter()
            .zip(&self.captured)
            .take_while(|(a, b)| a == b)
            .count() as u64;
        let last = (overlap - prefix).min(WINDOW as u64) as usize;
        let suffix = if last == 0 {
            0
        } else {
            let end = edit
                .start()
                .checked_add(edit.replacement_len())
                .ok_or(ContentError::LengthOverflow)?;
            self.window(base_end - last as u64, end - last as u64, last)?;
            self.original
                .iter()
                .rev()
                .zip(self.captured.iter().rev())
                .take_while(|(a, b)| a == b)
                .count() as u64
        };
        let retained = prefix + suffix;
        let mut owner = self.shared.borrow_mut();
        owner.work.edge_retained_bytes = owner
            .work
            .edge_retained_bytes
            .checked_add(retained)
            .ok_or(ContentError::LengthOverflow)?;
        let trimmed = Edit::new(
            edit.start() + prefix,
            edit.end() - suffix,
            edit.replacement_len() - retained,
        );
        Ok((trimmed.removed_len() != 0 || trimmed.replacement_len() != 0).then_some(trimmed))
    }
    fn window(&mut self, base_at: u64, file_at: u64, length: usize) -> ContentResult<()> {
        self.shared.borrow().ready()?;
        let (Some(base), Some(file)) = (self.base, self.file) else {
            return Err(ContentError::InvalidRecord("captured edge base binding"));
        };
        let client = base.client();
        self.original.clear();
        if self.original.capacity() < WINDOW {
            self.original.reserve_exact(WINDOW);
        }
        Timing::disabled("captured.edge.base", |scope| {
            file.read_range(
                client.as_ref(),
                base_at..base_at + length as u64,
                &mut self.original,
                scope,
            )
        })
        .0?;
        if self.captured.capacity() < WINDOW {
            self.captured.reserve_exact(WINDOW - self.captured.len());
        }
        self.captured.resize(length, 0);
        let mut scan = Scan::new(
            self.facts.reader,
            self.facts.serial,
            self.facts.final_size,
            self.facts.base_size,
        )?;
        let mut done = 0;
        while done < length {
            let at = file_at + done as u64;
            let remaining = (length - done) as u64;
            let supplied = scan.read(
                self.provider,
                self.shared,
                at,
                remaining,
                &mut self.captured[done..],
                true,
            )?;
            let count = match supplied {
                Supply::Run(FileRun::Data(count)) => count,
                Supply::Run(FileRun::Zero(count)) => {
                    let count = usize::try_from(count).map_err(|_| ContentError::LengthOverflow)?;
                    let end = done
                        .checked_add(count)
                        .filter(|end| *end <= length)
                        .ok_or(ContentError::InvalidRecord("captured edge zero range"))?;
                    self.captured[done..end].fill(0);
                    count
                }
                Supply::Committed {
                    root,
                    offset,
                    length: count,
                } => {
                    let count = usize::try_from(count).map_err(|_| ContentError::LengthOverflow)?;
                    let end = done
                        .checked_add(count)
                        .filter(|end| *end <= length)
                        .ok_or(ContentError::InvalidRecord("captured edge committed range"))?;
                    let mut output = &mut self.captured[done..end];
                    if root.is_none() || root == self.facts.file_root.map(|root| root.to_bytes()) {
                        Timing::disabled("captured.edge.committed", |scope| {
                            file.read_range(
                                client.as_ref(),
                                offset..offset + count as u64,
                                &mut output,
                                scope,
                            )
                        })
                        .0?;
                    } else {
                        base.content(root.ok_or(ContentError::UnexpectedEof)?)?
                            .read(offset, count as u64, &mut output)?;
                    }
                    count
                }
                Supply::Run(FileRun::End) => return Err(ContentError::UnexpectedEof),
            };
            if count == 0 || count > length - done {
                return Err(ContentError::InvalidRecord("captured edge progress"));
            }
            done += count;
        }
        if self.original.len() != length {
            return Err(ContentError::InvalidRecord("captured edge base length"));
        }
        let mut owner = self.shared.borrow_mut();
        owner.work.edge_windows = owner
            .work
            .edge_windows
            .checked_add(1)
            .ok_or(ContentError::LengthOverflow)?;
        // Logical bytes acquired from both sources, not physical reads or
        // database/OS cache hits. The Store keeps those separate counters.
        owner.work.edge_read_bytes = owner
            .work
            .edge_read_bytes
            .checked_add(2 * length as u64)
            .ok_or(ContentError::LengthOverflow)?;
        Ok(())
    }
}
