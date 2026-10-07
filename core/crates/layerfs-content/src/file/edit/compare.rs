//! Bounded applicability comparison for an exact no-op.
//!
//! An edit stream whose every replacement is byte-identical to the base range it
//! stands in for leaves the file unchanged, so the operation returns the base root
//! itself instead of rebuilding an identical structure. The comparison reads base
//! bytes and replacement bytes in bounded windows and stops at the first
//! difference; a length change is a difference by construction. The comparison is
//! planned work, not a probe: a mismatch replays the same edits for construction,
//! and both passes and every base read stay charged to the operation.

use layerfs_telemetry::timer::TimingScope;

use layerfs_telemetry::timer::Active;

use super::{runs::ReplacementRuns, source::Source, zero::ZeroEvidence};
use crate::error::{ContentError, ContentResult};
use crate::file::edit::input::{EditSequence, EditSource, Plan, Segment};
use crate::file::mapping::{NodeSummary, PageCache, RangeCursor};
use crate::file::view::FileView;
use crate::file::{FileRun, FileRuns};
use crate::object::AuthenticatedObjects;

/// Bytes compared per window.
pub const COMPARE_WINDOW_BYTES: usize = 64 * 1024;

/// Outcome of the applicability comparison.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoOpVerdict {
    /// Every replacement is byte-identical to the range it replaces.
    Equal,
    /// At least one replacement differs; construction must run.
    Differs,
}

/// Compares every replacement with its base range, bounded and in order.
///
/// `pages` is the operation's shared mapping-page memo. A chunked base reads its
/// windows through a cursor over it, so the pages this pass acquires are already
/// held when the construction pass reaches them; a whole-file base has no mapping
/// to navigate and leaves the memo untouched.
pub fn compare_replacements(
    view: &FileView,
    reader: &dyn AuthenticatedObjects,
    stream: &dyn EditSequence,
    source: &dyn EditSource,
    pages: &mut PageCache,
    scope: TimingScope<'_>,
) -> ContentResult<NoOpVerdict> {
    compare_source(view, reader, stream, Source::Legacy(source), pages, scope)
}

pub(super) fn compare_source(
    view: &FileView,
    reader: &dyn AuthenticatedObjects,
    stream: &dyn EditSequence,
    source: Source<'_>,
    pages: &mut PageCache,
    scope: TimingScope<'_>,
) -> ContentResult<NoOpVerdict> {
    scope.run(|compare| match view.file_state()? {
        Some(state) => {
            let mut cursor = RangeCursor::new(reader, state, pages, compare)?;
            compare_windows(
                view,
                reader,
                Some(&mut cursor),
                pages,
                stream,
                source,
                compare,
            )
        }
        None => compare_windows(view, reader, None, pages, stream, source, compare),
    })
}

/// Walks the plan's replacement segments in bounded windows.
fn compare_windows(
    view: &FileView,
    reader: &dyn AuthenticatedObjects,
    mut cursor: Option<&mut RangeCursor<'_, '_, '_>>,
    pages: &mut PageCache,
    stream: &dyn EditSequence,
    source: Source<'_>,
    compare: &TimingScope<'_, Active>,
) -> ContentResult<NoOpVerdict> {
    let mut plan = Plan::new(stream);
    let mut zero = ZeroEvidence::default();
    let mut base_window: Vec<u8> = Vec::new();
    let mut replacement_window: Vec<u8> = Vec::new();
    while let Some(segment) = plan.advance()? {
        let Segment::Replace { index, base, len } = segment else {
            continue;
        };
        if len != base.1 - base.0 {
            source.check_indexed_length(index, len)?;
            return Ok(NoOpVerdict::Differs);
        }
        if len == 0 {
            source.check_indexed_length(index, len)?;
            continue;
        }
        let mut replacement = ReplacementRuns::for_comparison(source, index, len)?;
        replacement_window.resize(COMPARE_WINDOW_BYTES, 0);
        loop {
            let offset = replacement.position();
            let run = replacement.read_run(&mut replacement_window)?;
            let length = match run {
                FileRun::End => break,
                FileRun::Data(count) => count as u64,
                FileRun::Zero(length) => length,
            };
            let start = base
                .0
                .checked_add(offset)
                .ok_or(ContentError::LengthOverflow)?;
            let end = start
                .checked_add(length)
                .ok_or(ContentError::LengthOverflow)?;
            let window = start..end;
            if let FileRun::Zero(_) = run {
                let equal =
                    compare
                        .child("edit.compare.zero")
                        .run(|scope| match view.file_state()? {
                            Some(state) => zero.mapping(
                                reader,
                                pages,
                                NodeSummary {
                                    id: state.mapping_root,
                                    bytes: state.logical_len,
                                    extents: state.extent_count,
                                    level: state.tree_level,
                                },
                                window,
                                true,
                                scope,
                            ),
                            None => {
                                let payload = view
                                    .whole_file_bytes()?
                                    .ok_or(ContentError::WrongLogicalRole)?;
                                let low = usize::try_from(start)
                                    .map_err(|_| ContentError::LengthOverflow)?;
                                let high = usize::try_from(end)
                                    .map_err(|_| ContentError::LengthOverflow)?;
                                Ok(payload
                                    .get(low..high)
                                    .ok_or(ContentError::InvalidRecord("whole-file range"))?
                                    .iter()
                                    .all(|byte| *byte == 0))
                            }
                        })?;
                if !equal {
                    return Ok(NoOpVerdict::Differs);
                }
                continue;
            }
            base_window.clear();
            compare
                .child("edit.compare.window")
                .run(|scope| match cursor.as_mut() {
                    Some(cursor) => cursor
                        .read_segment(window, &mut base_window, pages)
                        .map(|_| ()),
                    None => view.read_range(reader, window, &mut base_window, scope),
                })?;
            if base_window.len() as u64 != length {
                return Err(ContentError::LengthMismatch {
                    expected: length,
                    actual: base_window.len() as u64,
                });
            }
            if base_window != replacement_window[..length as usize] {
                return Ok(NoOpVerdict::Differs);
            }
        }
    }
    Ok(NoOpVerdict::Equal)
}
