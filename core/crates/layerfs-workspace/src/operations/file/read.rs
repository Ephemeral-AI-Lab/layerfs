//! Composed file read: one local layered plan, then one inherited base range.
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
        if read.source().route() != self.source.route() {
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
        if read.source().route() != self.source.route() {
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
        if reader.capture().route() != self.source.route() {
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
/// which the caller bound to the root those bytes belong to. A window with
/// no inherited byte needs no base.
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
        for slot in first..first + span as usize {
            if local.inherited[slot / 8] & (1 << (slot % 8)) != 0 {
                // The base file ends where it ends; beyond that is zero.
                local.data[slot] = inherited.get(slot - first).copied().unwrap_or(0);
            }
        }
    }
    Ok(local.data)
}
