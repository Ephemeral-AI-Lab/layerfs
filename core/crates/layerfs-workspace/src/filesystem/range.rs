//! Checked local and projected range replacement admission.
use super::write::FileMutation;
use crate::backing::metadata_pages::PageRef;
use crate::overlay::pieces::{Piece, PieceKind};
use crate::{runtime::coherence::MutationOrigin, *};
use layerfs_bridge::contract::MAX_FILE;
use std::time::Instant;

pub(super) fn logical_length(
    edit: &RangeEdit,
    parts: Option<&[RangePart]>,
) -> Result<u64, WorkspaceError> {
    let Some(parts) = parts else {
        return Ok(edit.replacement.len());
    };
    if parts.len() > 1024 {
        return Err(WorkspaceError::Capacity);
    }
    let mut logical = 0u64;
    let mut literal = 0u64;
    for part in parts {
        let length = match part {
            RangePart::Bytes(length) | RangePart::Zero(length) => *length,
        };
        if length == 0 {
            return Err(WorkspaceError::InvalidInput);
        }
        logical = logical
            .checked_add(length)
            .ok_or(WorkspaceError::Capacity)?;
        if matches!(part, RangePart::Bytes(_)) {
            literal = literal
                .checked_add(length)
                .ok_or(WorkspaceError::Capacity)?;
        }
    }
    if literal != edit.replacement.len() {
        return Err(WorkspaceError::InvalidInput);
    }
    if logical > 8 * 1024 * 1024 {
        return Err(WorkspaceError::Capacity);
    }
    Ok(logical)
}

pub(super) fn replacement_pieces(
    parts: &[RangePart],
    local: Piece,
) -> Result<Vec<Piece>, WorkspaceError> {
    let mut pieces = Vec::new();
    pieces
        .try_reserve_exact(parts.len())
        .map_err(|_| WorkspaceError::Capacity)?;
    let mut offset = 0u64;
    for part in parts {
        let mut piece = local;
        match *part {
            RangePart::Bytes(length) => {
                piece.length = length;
                piece.offset = offset;
                offset = offset.checked_add(length).ok_or(WorkspaceError::Capacity)?;
            }
            RangePart::Zero(length) => {
                piece.kind = PieceKind::Zero;
                piece.length = length;
                piece.offset = 0;
                piece.payload = 0;
                piece.custody = PageRef::NULL;
            }
        }
        pieces.push(piece);
    }
    Ok(pieces)
}

impl Workspace {
    pub fn edit_file_range(
        &self,
        path: &WorkspacePath,
        edit: &RangeEdit,
        deadline: Instant,
    ) -> Result<MutationReceipt, WorkspaceError> {
        if self.inner.access != WorkspaceAccess::LocalEdit {
            return Err(WorkspaceError::ReadOnly);
        }
        let deadline = Self::callback_deadline(deadline);
        let mut operation = self.begin(false, deadline)?;
        self.check_payload_owner(&edit.replacement)?;
        if edit.start > edit.end {
            return Err(WorkspaceError::InvalidInput);
        }
        if edit.replacement.len() > 8 * 1024 * 1024 {
            return Err(WorkspaceError::Capacity);
        }
        let original = self.edit_original(path, deadline, &mut operation)?;
        self.mutate_file(
            original,
            FileMutation::Range {
                edit,
                parts: None,
                handle: None,
                expected: None,
                origin: MutationOrigin::Local,
            },
            deadline,
            None,
        )
        .map(|published| published.receipt)
    }
    pub(crate) fn edit_file_range_from(
        &self,
        handle: HandleId,
        expected: RangeStamp,
        edit: &RangeEdit,
        deadline: Instant,
    ) -> Result<MutationReceipt, WorkspaceError> {
        self.edit_file_range_from_parts(handle, expected, edit, None, deadline)
    }
    pub(crate) fn edit_file_range_stream_from(
        &self,
        handle: HandleId,
        expected: RangeStamp,
        edit: &RangeEdit,
        parts: &[RangePart],
        deadline: Instant,
    ) -> Result<MutationReceipt, WorkspaceError> {
        self.edit_file_range_from_parts(handle, expected, edit, Some(parts), deadline)
    }
    fn edit_file_range_from_parts<'a>(
        &self,
        handle: HandleId,
        expected: RangeStamp,
        edit: &'a RangeEdit,
        parts: Option<&'a [RangePart]>,
        deadline: Instant,
    ) -> Result<MutationReceipt, WorkspaceError> {
        if self.inner.access != WorkspaceAccess::LocalEdit {
            return Err(WorkspaceError::ReadOnly);
        }
        let deadline = Self::callback_deadline(deadline);
        let mut operation = self.begin(false, deadline)?;
        let logical = logical_length(edit, parts)?;
        if edit.start > edit.end || (edit.start == edit.end && logical == 0) {
            return Err(WorkspaceError::InvalidInput);
        }
        if logical > 8 * 1024 * 1024 {
            return Err(WorkspaceError::Capacity);
        }
        let mutation = FileMutation::Range {
            edit,
            parts,
            handle: Some(handle),
            expected: Some(expected),
            origin: MutationOrigin::ProjectionRange,
        };
        let serial = {
            let state = self.state()?;
            self.available(&state)?;
            let opened = self.write_handle(&state, handle, MutationOrigin::ProjectionRange)?;
            self.mutation_handle(&state, mutation, opened.serial)?;
            let length = state.presented(state.node(opened.serial)?.attr).size;
            if edit.end > length {
                return Err(WorkspaceError::InvalidInput);
            }
            if length
                .checked_sub(edit.end - edit.start)
                .and_then(|left| left.checked_add(logical))
                .is_none_or(|result| result > MAX_FILE)
            {
                return Err(WorkspaceError::Capacity);
            }
            opened.serial
        };
        self.check_payload_owner(&edit.replacement)?;
        let original = self.serial_original(serial, deadline, &mut operation)?;
        {
            let state = self.state()?;
            self.available(&state)?;
            self.mutation_handle(&state, mutation, serial)?;
        }
        self.mutate_file(original, mutation, deadline, None)
            .map(|published| published.receipt)
    }
}
