//! One operation-local authenticated view of an immutable base.
//!
//! The base root is acquired and classified exactly once per operation; every
//! later read works from that classification and asks the provider only for the
//! mapping pages and payloads it actually needs. A range read appends to its sink
//! in logical order, and an extent walk visits one decoded leaf at a time, so a
//! whole-file pass never holds the mapping.

use std::io::Write;
use std::ops::Range;

use layerfs_telemetry::timer::TimingScope;

use crate::error::{ContentError, ContentResult};
use crate::file::content::{self, FileContent};
use crate::file::mapping::{self, FileState};
use crate::object::{AuthenticatedObjects, ObjectId};

/// Immutable, already-authenticated base of one operation.
pub struct FileView {
    root: ObjectId,
    canonical: Vec<u8>,
    content: FileContent,
}

impl FileView {
    /// Acquires and classifies the base root once.
    pub fn open(
        reader: &dyn AuthenticatedObjects,
        root: ObjectId,
        scope: TimingScope<'_>,
    ) -> ContentResult<Self> {
        scope.run(|open| {
            let canonical = open
                .child("edit.base_read")
                .run(|_| reader.read_canonical(root))?;
            let content = content::classify(&canonical)?;
            Ok(Self {
                root,
                canonical,
                content,
            })
        })
    }

    /// Identity of the base this view was opened on.
    pub const fn root(&self) -> ObjectId {
        self.root
    }

    /// Logical byte length of the base.
    pub const fn logical_len(&self) -> u64 {
        self.content.logical_len()
    }

    /// Recorded representation of the base.
    pub const fn content(&self) -> FileContent {
        self.content
    }

    /// File state of a chunked base.
    pub fn file_state(&self) -> ContentResult<Option<FileState>> {
        match self.content {
            FileContent::Chunked(state) => Ok(Some(state)),
            FileContent::WholeFile { .. } => Ok(None),
        }
    }

    /// Whole-file payload of a non-chunked base.
    pub fn whole_file_bytes(&self) -> ContentResult<Option<&[u8]>> {
        match self.content {
            FileContent::WholeFile { .. } => Ok(Some(
                content::whole_file_payload(&self.canonical)?
                    .ok_or(ContentError::WrongLogicalRole)?,
            )),
            FileContent::Chunked(_) => Ok(None),
        }
    }

    /// Reads one logical base range, appending it to `sink`.
    pub fn read_range(
        &self,
        reader: &dyn AuthenticatedObjects,
        range: Range<u64>,
        sink: &mut dyn Write,
    ) -> ContentResult<()> {
        if range.start > range.end || range.end > self.logical_len() {
            return Err(ContentError::InvalidRange {
                start: range.start,
                end: range.end,
                length: self.logical_len(),
            });
        }
        if range.start == range.end {
            return Ok(());
        }
        match self.content {
            FileContent::WholeFile { .. } => {
                let payload = self
                    .whole_file_bytes()?
                    .ok_or(ContentError::WrongLogicalRole)?;
                let slice = payload
                    .get(range.start as usize..range.end as usize)
                    .ok_or(ContentError::InvalidRecord("whole-file range"))?;
                sink.write_all(slice).map_err(|_| ContentError::Io)
            }
            FileContent::Chunked(state) => {
                mapping::read_range(reader, state, range, sink).map(|_| ())
            }
        }
    }
}
