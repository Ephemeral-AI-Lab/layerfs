//! One fallible provider adapter shared by legacy and indexed edit requests.
use super::input::{EditSource, IndexedEditSource};
use crate::{ContentError, ContentResult, FileRun};

#[derive(Clone, Copy)]
pub(super) enum Source<'a> {
    Legacy(&'a dyn EditSource),
    Indexed(&'a dyn IndexedEditSource),
}
impl Source<'_> {
    pub fn check_legacy_length(self, index: usize, declared: u64) -> ContentResult<()> {
        if let Self::Legacy(source) = self {
            if source.replacement_len(index) != declared {
                return Err(ContentError::InvalidEdit {
                    what: "replacement length",
                });
            }
        }
        Ok(())
    }
    pub fn check_indexed_length(self, index: usize, declared: u64) -> ContentResult<()> {
        if let Self::Indexed(source) = self {
            if source.replacement_len(index)? != declared {
                return Err(ContentError::InvalidEdit {
                    what: "replacement length",
                });
            }
        }
        Ok(())
    }
    pub fn replacement_len(self, index: usize) -> ContentResult<u64> {
        match self {
            Self::Legacy(source) => Ok(source.replacement_len(index)),
            Self::Indexed(source) => source.replacement_len(index),
        }
    }
    pub fn read_run_at(
        self,
        index: usize,
        offset: u64,
        output: &mut [u8],
    ) -> ContentResult<FileRun> {
        match self {
            Self::Legacy(source) => source.read_run_at(index, offset, output),
            Self::Indexed(source) => source.read_run_at(index, offset, output),
        }
    }
}
