//! Fallible indexed edits and exact captured replacement runs over one owner.
use super::{
    context,
    owner::Facts,
    scan::{Scan, Supply},
    state::Shared,
};
use crate::construction::outcome::nanos;
use crate::{BaseContent, BaseView, OverlayCapturedRuns, OverlayOperationRecords};
use layerfs_content::{
    ContentError, ContentResult, Edit, EditRecordApply, EditRecordChange, EditRecordKey,
    EditSequence, FileRun, FileRuns, IndexedEditBacking, IndexedEditSource,
};
use std::{
    cell::{Cell, RefCell},
    time::Instant,
};

pub(super) struct Input<'s, 'a, P: OverlayCapturedRuns + OverlayOperationRecords + ?Sized> {
    pub provider: &'a P,
    pub shared: &'s Shared<'a, P>,
    pub facts: Facts,
    pub count: usize,
    pub context: &'s [u8],
    scan: RefCell<Scan>,
    position: Cell<u64>,
    restarted: Cell<bool>,
    base: Option<&'s BaseView>,
    /// The committed content last read for a span that could not be retained.
    committed: RefCell<Option<BaseContent>>,
}
impl<'s, 'a, P: OverlayCapturedRuns + OverlayOperationRecords + ?Sized> Input<'s, 'a, P> {
    pub fn new(
        provider: &'a P,
        shared: &'s Shared<'a, P>,
        facts: Facts,
        count: usize,
        context: &'s [u8],
        base: Option<&'s BaseView>,
    ) -> ContentResult<Self> {
        Ok(Self {
            provider,
            shared,
            facts,
            count,
            context,
            scan: RefCell::new(Scan::new(
                facts.reader,
                facts.serial,
                facts.final_size,
                facts.base_size,
            )?),
            position: Cell::new(0),
            restarted: Cell::new(false),
            base,
            committed: RefCell::new(None),
        })
    }
    /// Bytes of a reference or of inherited content that the script could
    /// not retain: read from committed content and offered as new bytes.
    /// Correct for any source, and linear in these bytes.
    fn committed(
        &self,
        root: Option<[u8; 32]>,
        offset: u64,
        length: u64,
        output: &mut [u8],
    ) -> ContentResult<FileRun> {
        let count = usize::try_from(length)
            .unwrap_or(usize::MAX)
            .min(output.len());
        let root = root.or(self.facts.file_root.map(|root| root.to_bytes()));
        let (Some(root), Some(base), true) = (root, self.base, count != 0) else {
            return Err(ContentError::InvalidRecord("captured committed source"));
        };
        let mut held = self.committed.borrow_mut();
        if held.as_ref().is_none_or(|content| content.root() != root) {
            *held = Some(base.content(root)?);
        }
        let content = held.as_ref().ok_or(ContentError::UnexpectedEof)?;
        content.read(offset, count as u64, &mut &mut output[..count])?;
        let mut owner = self.shared.borrow_mut();
        owner.work.committed_bytes = owner
            .work
            .committed_bytes
            .checked_add(count as u64)
            .ok_or(ContentError::LengthOverflow)?;
        Ok(if output[..count].iter().all(|byte| *byte == 0) {
            FileRun::Zero(count as u64)
        } else {
            FileRun::Data(count)
        })
    }
    fn edit(&self, index: usize) -> ContentResult<Edit> {
        context::edit(self.shared, self.context, self.facts, self.count, index)
    }
    fn run(&self, at: u64, length: u64, output: &mut [u8]) -> ContentResult<FileRun> {
        self.shared.borrow().ready()?;
        if at < self.position.get() {
            if self.restarted.replace(true) {
                return Err(self
                    .shared
                    .borrow_mut()
                    .content(ContentError::InvalidRecord(
                        "captured source extra backwards pass",
                    )));
            }
            *self.scan.borrow_mut() = Scan::new(
                self.facts.reader,
                self.facts.serial,
                self.facts.final_size,
                self.facts.base_size,
            )?;
            let mut owner = self.shared.borrow_mut();
            owner.work.source_resets = owner
                .work
                .source_resets
                .checked_add(1)
                .ok_or(ContentError::LengthOverflow)?;
        }
        let started = Instant::now();
        let result = self
            .scan
            .borrow_mut()
            .read(self.provider, self.shared, at, length, output, false)
            .and_then(|supply| match supply {
                Supply::Run(run) => Ok(run),
                Supply::Committed {
                    root,
                    offset,
                    length,
                } => self.committed(root, offset, length, output),
            });
        {
            let mut owner = self.shared.borrow_mut();
            owner.work.source_ns = owner.work.source_ns.saturating_add(nanos(started));
        }
        let run = result.map_err(|error| self.shared.borrow_mut().content(error))?;
        let length = match run {
            FileRun::Data(count) => count as u64,
            FileRun::Zero(count) => count,
            FileRun::End => {
                return Err(self
                    .shared
                    .borrow_mut()
                    .content(ContentError::UnexpectedEof))
            }
        };
        self.position
            .set(at.checked_add(length).ok_or(ContentError::LengthOverflow)?);
        Ok(run)
    }
}
impl<P: OverlayCapturedRuns + OverlayOperationRecords + ?Sized> EditSequence for Input<'_, '_, P> {
    fn base_len(&self) -> u64 {
        self.facts.base_size
    }
    fn final_len(&self) -> u64 {
        self.facts.final_size
    }
    fn len(&self) -> usize {
        self.count
    }
    fn edit_at(&self, index: usize) -> ContentResult<Edit> {
        self.edit(index)
    }
}
impl<P: OverlayCapturedRuns + OverlayOperationRecords + ?Sized> IndexedEditSource
    for Input<'_, '_, P>
{
    fn replacement_len(&self, index: usize) -> ContentResult<u64> {
        Ok(self.edit(index)?.replacement_len())
    }
    fn read_run_at(&self, index: usize, offset: u64, output: &mut [u8]) -> ContentResult<FileRun> {
        let edit = self.edit(index)?;
        if offset > edit.replacement_len() {
            return Err(self.shared.borrow_mut().content(ContentError::InvalidEdit {
                what: "captured replacement offset",
            }));
        }
        if offset == edit.replacement_len() {
            return Ok(FileRun::End);
        }
        let at = edit
            .start()
            .checked_add(offset)
            .ok_or(ContentError::LengthOverflow)?;
        self.run(at, edit.replacement_len() - offset, output)
    }
}
pub(super) struct Complete<'s, 'a, P: OverlayCapturedRuns + OverlayOperationRecords + ?Sized> {
    pub input: Input<'s, 'a, P>,
    pub position: u64,
    /// Whether the runs follow a sealed context record, checked again before
    /// the stream ends. A base-less file on the direct route has none.
    pub sealed: bool,
}
impl<P: OverlayCapturedRuns + OverlayOperationRecords + ?Sized> FileRuns for Complete<'_, '_, P> {
    fn read_run(&mut self, output: &mut [u8]) -> ContentResult<FileRun> {
        // The caller checked the sealed context before the first run; it is
        // checked again before the stream may end, so no file completes over
        // a context that changed underneath it.
        if self.position == self.input.facts.final_size {
            if self.sealed {
                context::verify(self.input.shared, self.input.context)?;
            }
            return Ok(FileRun::End);
        }
        let run = self.input.run(
            self.position,
            self.input.facts.final_size - self.position,
            output,
        )?;
        self.position = self
            .position
            .checked_add(match run {
                FileRun::Data(count) => count as u64,
                FileRun::Zero(count) => count,
                FileRun::End => 0,
            })
            .ok_or(ContentError::LengthOverflow)?;
        Ok(run)
    }
}
pub(super) struct Backing<'s, 'a, P: OverlayOperationRecords + ?Sized>(pub &'s Shared<'a, P>);
impl<P: OverlayOperationRecords + ?Sized> IndexedEditBacking for Backing<'_, '_, P> {
    fn contains(&mut self, key: EditRecordKey) -> ContentResult<bool> {
        let mut owner = self.0.borrow_mut();
        owner.ready()?;
        let result = owner.records.contains(key);
        result.map_err(|error| owner.content(error))
    }
    fn get(&mut self, key: EditRecordKey) -> ContentResult<Option<Vec<u8>>> {
        self.0.borrow_mut().get(key)
    }
    fn apply(&mut self, changes: Vec<EditRecordChange>) -> ContentResult<EditRecordApply> {
        let mut owner = self.0.borrow_mut();
        owner.ready()?;
        let result = owner.records.apply(changes);
        result.map_err(|error| owner.content(error))
    }
    fn first_keys(
        &mut self,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> ContentResult<Vec<[u8; 32]>> {
        let mut owner = self.0.borrow_mut();
        owner.ready()?;
        let result = owner.records.first_keys(kind, excluded);
        result.map_err(|error| owner.content(error))
    }
    fn keys_after(&mut self, kind: u32, after: Option<[u8; 32]>) -> ContentResult<Vec<[u8; 32]>> {
        let mut owner = self.0.borrow_mut();
        owner.ready()?;
        let result = owner.records.keys_after(kind, after);
        result.map_err(|error| owner.content(error))
    }
}
