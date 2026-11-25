//! Whether a capture left a base file's bytes as the base holds them (#810 F5).
use super::owner::{CapturedFileCustody, CapturedFileWork};
use crate::{
    BaseView, IndexedConstructionRecords, OverlayCapturedRuns, OverlayOperationRecords,
    WorkspaceError, WorkspaceResult,
};
use layerfs_content::ContentError;
use layerfs_overlay::{
    CapturedGap, CapturedReader, CapturedRunCursor, CapturedRunStep, IndexedOperationRecordScope,
};

/// True only when every byte of the captured file `serial`, `size` bytes
/// long, is the lower view's byte at the same offset: no retained layer
/// holds a reference piece, and the first unit of a shapes-only pass from
/// offset 0 is one inherited gap over `[0, size)` (`End` for an empty file).
/// Such a gap leaves no live written cell under any byte and no layer's
/// cutoff or end makes any byte zero. Stale rows on the way return
/// `Continue`, each consuming a row, as normalization's scan sees them.
///
/// The caller still compares `size` with the base file's length: an
/// inherited byte is the base's byte only below that length, and a shrink
/// leaves an inherited gap over the shorter size. Any other unit answers
/// false, and the file is constructed from its captured runs as before.
/// Reads only the Workspace's overlay; no Store demand.
pub(crate) fn inherited<P: OverlayCapturedRuns + ?Sized>(
    provider: &P,
    reader: CapturedReader,
    serial: u64,
    size: u64,
) -> WorkspaceResult<bool> {
    let mut cursor = CapturedRunCursor::new(reader, serial, 0, size)
        .map_err(|_| ContentError::InvalidRecord("captured file cursor"))?
        .shapes_only();
    loop {
        let reply = provider.captured_run_step(cursor)?;
        if reply.referenced {
            return Ok(false);
        }
        match reply.step {
            CapturedRunStep::Continue(next) => {
                if !next.metadata_advanced_from(cursor) || reply.work.metadata_rows == 0 {
                    return Err(ContentError::InvalidRecord("captured file cursor progress").into());
                }
                cursor = next;
            }
            CapturedRunStep::Gap {
                kind: CapturedGap::Inherited,
                offset: 0,
                length,
                ..
            } => return Ok(length == size),
            CapturedRunStep::End => return Ok(size == 0),
            _ => return Ok(false),
        }
    }
}

/// The custody of file `serial` whose [`inherited`] question failed: what a
/// failed preparation of that file keeps, with its original cause, its
/// reader and its (untouched) record scope. Nothing of it was written.
pub(crate) fn refused<P: OverlayOperationRecords + ?Sized>(
    provider: &P,
    reader: CapturedReader,
    serial: u64,
    records_scope: IndexedOperationRecordScope,
    base: &BaseView,
    failure: WorkspaceError,
) -> CapturedFileCustody {
    CapturedFileCustody {
        reader,
        serial,
        base: Some(base.clone()),
        file: None,
        records: IndexedConstructionRecords::new(provider, records_scope).into_custody(),
        failure: Some(failure),
        work: CapturedFileWork::default(),
    }
}
