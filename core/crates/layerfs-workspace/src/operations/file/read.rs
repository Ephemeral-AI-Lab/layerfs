//! Composed file read: one local layered plan, then one inherited base range.
use crate::{OverlayRead, SourceView, WorkspaceError, WorkspaceResult};
use layerfs_content::ContentError;
use layerfs_overlay::{InodeKind, READ_WINDOW};
use std::io::Write;

impl SourceView {
    /// Reads at most `length` bytes at `offset` into `sink` and returns the
    /// count, short at EOF. One owner job composes the local layers; bytes the
    /// base supplies are then fetched outside the owner as a single range that
    /// covers them, however fragmented, through the bounded base read plan.
    /// The window bounds one request, not the file or the total flow.
    pub fn read(
        &self,
        overlay: &impl OverlayRead,
        serial: u64,
        offset: u64,
        length: u32,
        sink: &mut dyn Write,
    ) -> WorkspaceResult<u64> {
        if length as usize > READ_WINDOW {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "read window",
                limit: READ_WINDOW as u64,
                actual: u64::from(length),
            }
            .into());
        }
        let Some(mut local) = overlay.read(self.source, serial, offset, length)? else {
            let plan = self.base.plan_read(serial, offset, length)?;
            plan.emit(sink)?;
            return Ok(plan.length());
        };
        if local.kind != InodeKind::File {
            return Err(ContentError::WrongLogicalRole.into());
        }
        if let Some((from, to)) = local.span {
            let span = u32::try_from(to - from)
                .ok()
                .filter(|span| *span as usize <= READ_WINDOW && from >= local.offset)
                .ok_or(ContentError::InvalidRecord("inherited read span"))?;
            let plan = self.base.plan_read(serial, from, span)?;
            let mut inherited = Vec::with_capacity(plan.length() as usize);
            plan.emit(&mut inherited)?;
            let first = (from - local.offset) as usize;
            for slot in first..first + span as usize {
                if local.inherited[slot / 8] & (1 << (slot % 8)) != 0 {
                    // The base file ends where it ends; beyond that is zero.
                    local.data[slot] = inherited.get(slot - first).copied().unwrap_or(0);
                }
            }
        }
        sink.write_all(&local.data)
            .map_err(|error| WorkspaceError::Service(Box::new(error)))?;
        Ok(local.data.len() as u64)
    }
}
