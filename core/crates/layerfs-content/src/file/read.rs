//! Logical file and range reads over the authenticated object provider.
//!
//! The root object is acquired once per read and classified from those bytes; a
//! whole-file read then emits its slice directly, and a chunked read hands the
//! decoded file state to the bounded extent traversal. Bytes reach the sink in
//! logical order, whether a range crosses extents or mapping pages.

use std::io::Write;
use std::ops::Range;

use layerfs_telemetry::timer::TimingScope;

use crate::error::{ContentError, ContentResult};
use crate::file::content::{self, FileContent};
use crate::file::mapping::{self, ReadCounters};
use crate::object::{AuthenticatedObjects, ObjectId};

/// Reads a whole file, bounded by a caller-declared maximum length.
pub fn read_all_bounded(
    reader: &dyn AuthenticatedObjects,
    root: ObjectId,
    maximum: u64,
    sink: &mut dyn Write,
    scope: TimingScope<'_>,
) -> ContentResult<ReadCounters> {
    scope.run(|read| {
        let canonical = read
            .child("content.acquire")
            .run(|_| reader.read_canonical(root))?;
        let content = content::classify(&canonical)?;
        if content.logical_len() > maximum {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "read.logical_length",
                limit: maximum,
                actual: content.logical_len(),
            });
        }
        emit_classified(
            reader,
            &canonical,
            content,
            0..content.logical_len(),
            sink,
            read,
        )
    })
}

/// Reads a whole file.
pub fn read_all(
    reader: &dyn AuthenticatedObjects,
    root: ObjectId,
    sink: &mut dyn Write,
    scope: TimingScope<'_>,
) -> ContentResult<ReadCounters> {
    read_all_bounded(reader, root, u64::MAX, sink, scope)
}

/// Reads a logical range of a file root.
pub fn read_range(
    reader: &dyn AuthenticatedObjects,
    root: ObjectId,
    range: Range<u64>,
    sink: &mut dyn Write,
    scope: TimingScope<'_>,
) -> ContentResult<ReadCounters> {
    scope.run(|read| {
        let canonical = read
            .child("content.acquire")
            .run(|_| reader.read_canonical(root))?;
        let content = content::classify(&canonical)?;
        emit_classified(reader, &canonical, content, range, sink, read)
    })
}

fn emit_classified(
    reader: &dyn AuthenticatedObjects,
    canonical: &[u8],
    content: FileContent,
    range: Range<u64>,
    sink: &mut dyn Write,
    scope: &TimingScope<'_, layerfs_telemetry::timer::Active>,
) -> ContentResult<ReadCounters> {
    let logical_len = content.logical_len();
    if range.start > range.end || range.end > logical_len {
        return Err(ContentError::InvalidRange {
            start: range.start,
            end: range.end,
            length: logical_len,
        });
    }
    match content {
        FileContent::WholeFile { .. } => {
            let mut counters = ReadCounters {
                nodes_read: 1,
                ..ReadCounters::default()
            };
            if range.start == range.end {
                return Ok(counters);
            }
            let payload =
                content::whole_file_payload(canonical)?.ok_or(ContentError::WrongLogicalRole)?;
            let slice = payload
                .get(range.start as usize..range.end as usize)
                .ok_or(ContentError::InvalidRecord("whole-file range"))?;
            scope
                .child("content.emit")
                .run(|_| sink.write_all(slice).map_err(|_| ContentError::Io))?;
            counters.payload_ids_read = 1;
            counters.payload_batches_read = 1;
            counters.max_payload_batch = 1;
            counters.payload_bytes_read = slice.len() as u64;
            Ok(counters)
        }
        FileContent::Chunked(state) => {
            if range.start == range.end {
                return Ok(ReadCounters {
                    nodes_read: 1,
                    ..ReadCounters::default()
                });
            }
            scope
                .child("content.traverse")
                .run(|traverse| mapping::read_range(reader, state, range, sink, traverse))
        }
    }
}
