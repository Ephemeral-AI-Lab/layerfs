//! Checked replayable replacement runs and one lazy whole-base Plan cursor.
use super::{
    input::{EditSequence, Plan, Segment},
    source::Source,
};
use crate::file::{FileRun, FileRuns, FileView};
use crate::{ContentError, ContentResult};

pub(super) struct ReplacementRuns<'a> {
    source: Source<'a>,
    index: usize,
    position: u64,
    length: u64,
    comparison: bool,
}
impl<'a> ReplacementRuns<'a> {
    pub fn new(source: Source<'a>, index: usize, length: u64) -> ContentResult<Self> {
        if source.replacement_len(index)? != length {
            return Err(ContentError::InvalidEdit {
                what: "replacement length",
            });
        }
        Ok(Self::from_length(source, index, length))
    }
    // The shared chunked driver preserves legacy metadata-check ordering while
    // resolving indexed metadata before any boundary effects. It checks that
    // provider before consuming this cursor; no source request occurs here.
    pub fn from_length(source: Source<'a>, index: usize, length: u64) -> Self {
        Self {
            source,
            index,
            position: 0,
            length,
            comparison: false,
        }
    }
    pub fn for_comparison(source: Source<'a>, index: usize, length: u64) -> ContentResult<Self> {
        let mut cursor = Self::new(source, index, length)?;
        cursor.comparison = true;
        Ok(cursor)
    }
    pub const fn position(&self) -> u64 {
        self.position
    }
}
impl FileRuns for ReplacementRuns<'_> {
    fn read_run(&mut self, output: &mut [u8]) -> ContentResult<FileRun> {
        let remaining = self.length - self.position;
        if remaining == 0 {
            return Ok(FileRun::End);
        }
        if output.is_empty() {
            return Err(ContentError::InvalidRecord("edit run byte window"));
        }
        let want = output
            .len()
            .min(usize::try_from(remaining).unwrap_or(usize::MAX));
        let run = self
            .source
            .read_run_at(self.index, self.position, &mut output[..want])?;
        let length = match run {
            FileRun::Data(count) if count != 0 && count <= want => count as u64,
            FileRun::Zero(length) if length != 0 && length <= remaining => length,
            // Preserve the byte construction path's premature EOF outcome.
            // Actual typed source errors above propagate unchanged.
            FileRun::End => {
                return Err(if self.comparison {
                    ContentError::InvalidEdit {
                        what: "replacement bytes",
                    }
                } else {
                    ContentError::Io
                })
            }
            _ => return Err(ContentError::InvalidRecord("edit run window")),
        };
        self.position = self
            .position
            .checked_add(length)
            .ok_or(ContentError::LengthOverflow)?;
        Ok(run)
    }
}

enum Current<'a> {
    Retained(&'a [u8]),
    Replacement(ReplacementRuns<'a>),
}
pub(super) struct PlanRuns<'a> {
    plan: Plan<'a>,
    source: Source<'a>,
    payload: &'a [u8],
    current: Option<Current<'a>>,
    done: bool,
}
impl<'a> PlanRuns<'a> {
    pub fn new(
        view: &'a FileView,
        stream: &'a dyn EditSequence,
        source: Source<'a>,
    ) -> ContentResult<Self> {
        Ok(Self {
            plan: Plan::new(stream),
            source,
            payload: view
                .whole_file_bytes()?
                .ok_or(ContentError::WrongLogicalRole)?,
            current: None,
            done: false,
        })
    }
}
impl FileRuns for PlanRuns<'_> {
    fn read_run(&mut self, output: &mut [u8]) -> ContentResult<FileRun> {
        if output.is_empty() {
            return Err(ContentError::InvalidRecord("edit run byte window"));
        }
        loop {
            match &mut self.current {
                Some(Current::Retained(bytes)) if !bytes.is_empty() => {
                    let count = bytes.len().min(output.len());
                    output[..count].copy_from_slice(&bytes[..count]);
                    *bytes = &bytes[count..];
                    return Ok(FileRun::Data(count));
                }
                Some(Current::Replacement(source)) => {
                    let run = source.read_run(output)?;
                    if run != FileRun::End {
                        return Ok(run);
                    }
                }
                _ => {}
            }
            self.current = None;
            if self.done {
                return Ok(FileRun::End);
            }
            match self.plan.advance()? {
                None => self.done = true,
                Some(Segment::Retain { base }) => {
                    let start =
                        usize::try_from(base.0).map_err(|_| ContentError::LengthOverflow)?;
                    let end = usize::try_from(base.1).map_err(|_| ContentError::LengthOverflow)?;
                    self.current = Some(Current::Retained(
                        self.payload
                            .get(start..end)
                            .ok_or(ContentError::InvalidRecord("whole-file range"))?,
                    ));
                }
                Some(Segment::Replace { index, len, .. }) => {
                    self.current = Some(Current::Replacement(ReplacementRuns::new(
                        self.source,
                        index,
                        len,
                    )?));
                }
            }
        }
    }
}
