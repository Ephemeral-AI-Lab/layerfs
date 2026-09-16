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

use crate::error::{ContentError, ContentResult};
use crate::file::edit::input::{EditSource, EditStream, Plan, Segment};
use crate::file::view::FileView;
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
pub fn compare_replacements(
    view: &FileView,
    reader: &dyn AuthenticatedObjects,
    stream: &EditStream,
    source: &dyn EditSource,
    scope: TimingScope<'_>,
) -> ContentResult<NoOpVerdict> {
    scope.run(|compare| {
        let mut plan = Plan::new(stream);
        let mut base_window: Vec<u8> = Vec::new();
        let mut replacement_window: Vec<u8> = Vec::new();
        while let Some(segment) = plan.advance()? {
            let Segment::Replace { index, base, len } = segment else {
                continue;
            };
            if len != base.1 - base.0 {
                return Ok(NoOpVerdict::Differs);
            }
            if len == 0 {
                continue;
            }
            let mut offset = 0_u64;
            while offset < len {
                let take = (len - offset).min(COMPARE_WINDOW_BYTES as u64);
                base_window.clear();
                compare.child("edit.compare.window").run(|_| {
                    view.read_range(
                        reader,
                        base.0 + offset..base.0 + offset + take,
                        &mut base_window,
                    )
                })?;
                if base_window.len() as u64 != take {
                    return Err(ContentError::LengthMismatch {
                        expected: take,
                        actual: base_window.len() as u64,
                    });
                }
                replacement_window.clear();
                replacement_window.resize(take as usize, 0);
                let read =
                    source.read_at(index, offset, &mut replacement_window[..take as usize])?;
                if read as u64 != take {
                    return Err(ContentError::InvalidEdit {
                        what: "replacement bytes",
                    });
                }
                if base_window != replacement_window {
                    return Ok(NoOpVerdict::Differs);
                }
                offset += take;
            }
        }
        Ok(NoOpVerdict::Equal)
    })
}
