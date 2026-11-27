//! Composed file read: one local layered plan, then one inherited base range
//! and the committed ranges its reference pieces name.
use crate::{BaseView, OverlayFileRead, OverlayRead, SourceView, WorkspaceError, WorkspaceResult};
use layerfs_content::ContentError;
use layerfs_overlay::{CapturedReader, FileRead, InodeKind, LocalRead, READ_WINDOW};
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
        let local = overlay.read(self.source, serial, offset, length)?;
        self.emit_local(
            serial,
            offset,
            length,
            local,
            self.base.identity().0.to_bytes(),
            sink,
        )
    }
    /// Descriptor-read processing custody is independent of descriptor lifetime.
    pub fn read_file(
        &self,
        overlay: &impl OverlayFileRead,
        read: FileRead,
        offset: u64,
        length: u32,
        sink: &mut dyn Write,
    ) -> WorkspaceResult<u64> {
        if read.source().engine() != self.source.engine() {
            return Err(layerfs_overlay::OverlayError::Stale.into());
        }
        let local = overlay.file_read(read, offset, length)?;
        self.read_file_window(read, offset, length, local, sink)
    }
    /// Compose a completed owner window using the same bounded inherited-range
    /// algorithm. The caller retains that completion and the FileRead owner
    /// through all provider/output consumers; this method submits no SQL.
    pub fn read_file_window(
        &self,
        read: FileRead,
        offset: u64,
        length: u32,
        local: Option<LocalRead>,
        sink: &mut dyn Write,
    ) -> WorkspaceResult<u64> {
        if read.source().engine() != self.source.engine() {
            return Err(layerfs_overlay::OverlayError::Stale.into());
        }
        self.emit_local(
            read.serial(),
            offset,
            length,
            local,
            read.source().root(),
            sink,
        )
    }
    /// Sealed input retains its original root even after a known install.
    pub fn read_captured(
        &self,
        overlay: &impl OverlayFileRead,
        reader: CapturedReader,
        serial: u64,
        offset: u64,
        length: u32,
        sink: &mut dyn Write,
    ) -> WorkspaceResult<u64> {
        if reader.capture().engine() != self.source.engine() {
            return Err(layerfs_overlay::OverlayError::Stale.into());
        }
        let local = overlay.captured_read(reader, serial, offset, length)?;
        self.emit_local(serial, offset, length, local, reader.root(), sink)
    }
    fn emit_local(
        &self,
        serial: u64,
        offset: u64,
        length: u32,
        local: Option<LocalRead>,
        root: [u8; 32],
        sink: &mut dyn Write,
    ) -> WorkspaceResult<u64> {
        if length as usize > READ_WINDOW {
            return Err(layerfs_overlay::OverlayError::Invalid("read window").into());
        }
        let root = local
            .as_ref()
            .and_then(|local| local.base_root)
            .unwrap_or(root);
        let base = self.base.at(root)?;
        let Some(local) = local else {
            let plan = base.plan_read(serial, offset, length)?;
            plan.emit(sink)?;
            return Ok(plan.length());
        };
        let data = file_window(Some(base.as_ref()), serial, local)?;
        sink.write_all(&data)
            .map_err(|error| WorkspaceError::Service(Box::new(error)))?;
        Ok(data.len() as u64)
    }
}
/// A regular file's local window with its inherited bytes read from `base`,
/// which the caller bound to the root those bytes belong to, and the bytes
/// of each reference range read from the committed content the range names,
/// at its source offset, through the same client. A window with no inherited
/// byte and no reference range needs no base.
pub(crate) fn file_window(
    base: Option<&BaseView>,
    serial: u64,
    mut local: LocalRead,
) -> WorkspaceResult<Vec<u8>> {
    if local.kind != InodeKind::File {
        return Err(ContentError::WrongLogicalRole.into());
    }
    if let Some((from, to)) = local.span {
        let base = base.ok_or(ContentError::InvalidRecord("inherited window without base"))?;
        let span = u32::try_from(to - from)
            .ok()
            .filter(|span| *span as usize <= READ_WINDOW && from >= local.offset)
            .ok_or(ContentError::InvalidRecord("inherited read span"))?;
        let plan = base.plan_read(serial, from, span)?;
        let mut inherited = Vec::with_capacity(plan.length() as usize);
        plan.emit(&mut inherited)?;
        let first = (from - local.offset) as usize;
        let end = first + span as usize;
        let bit = |slot: usize| local.inherited[slot / 8] & (1 << (slot % 8)) != 0;
        let mut slot = first;
        while slot < end {
            // Whole mask bytes move eight slots at a time, in a run or a gap.
            let whole = |slot: usize, bits: u8| {
                slot % 8 == 0 && slot + 8 <= end && local.inherited[slot / 8] == bits
            };
            if whole(slot, 0) {
                slot += 8;
                continue;
            }
            if !bit(slot) {
                slot += 1;
                continue;
            }
            let start = slot;
            while slot < end {
                if whole(slot, u8::MAX) {
                    slot += 8;
                } else if bit(slot) {
                    slot += 1;
                } else {
                    break;
                }
            }
            // The base file ends where it ends; beyond that is zero.
            let held = inherited
                .len()
                .saturating_sub(start - first)
                .min(slot - start);
            local.data[start..start + held]
                .copy_from_slice(&inherited[start - first..start - first + held]);
            local.data[start + held..slot].fill(0);
        }
    }
    if !local.references.is_empty() {
        let base = base.ok_or(ContentError::InvalidRecord(
            "referenced window without base",
        ))?;
        if local.referenced.len() != local.data.len().div_ceil(8) {
            return Err(ContentError::InvalidRecord("reference mask of a window").into());
        }
        let mut held: Option<crate::BaseContent> = None;
        let mut bytes = Vec::new();
        for range in &local.references {
            let first = range
                .offset
                .checked_sub(local.offset)
                .filter(|first| {
                    range.length <= READ_WINDOW as u64
                        && first + range.length <= local.data.len() as u64
                })
                .ok_or(ContentError::InvalidRecord("reference range of a window"))?
                as usize;
            if held
                .as_ref()
                .is_none_or(|content| content.root() != range.source)
            {
                held = Some(base.content(range.source)?);
            }
            let content = held.as_ref().ok_or(ContentError::UnexpectedEof)?;
            bytes.clear();
            // A range beyond the end of its source is an error, never zeros.
            content.read(range.source_offset, range.length, &mut bytes)?;
            // Ranges never overlap: a set bit inside one is a byte it supplies,
            // and a clear one a byte a cell decided.
            for (slot, byte) in (first..).zip(&bytes) {
                if local.referenced[slot / 8] & (1 << (slot % 8)) != 0 {
                    local.data[slot] = *byte;
                }
            }
        }
    }
    Ok(local.data)
}
