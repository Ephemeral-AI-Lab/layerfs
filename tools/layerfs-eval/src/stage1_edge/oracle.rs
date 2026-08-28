use super::artifact::{display_error, hex, io_error};
use super::limits::{BUFFER_BYTES, FILE_PATH, FIXTURE_MODE};
use super::schedule_model::{EditKind, EditSpec, Piece, PieceTable};
use crate::legacy_full::{Diagnostics, LayerFs, NativeMetadata, RootId};
use crate::stage1_fixture::{self, EvalResult};
use std::fs::File;
use std::io::{Read, Write};
pub(crate) struct PieceCursor<'a> {
    pub(crate) table: &'a PieceTable,
    pub(crate) backing: &'a [u8],
    pub(crate) piece_index: usize,
    pub(crate) within_piece: u64,
    pub(crate) position: u64,
    pub(crate) original_scratch: Vec<u8>,
    pub(crate) original_scratch_block: Option<u64>,
    pub(crate) original_blocks_generated: u64,
}
impl<'a> PieceCursor<'a> {
    pub(crate) fn new(table: &'a PieceTable, backing: &'a [u8]) -> Self {
        Self {
            table,
            backing,
            piece_index: 0,
            within_piece: 0,
            position: 0,
            original_scratch: vec![0_u8; BUFFER_BYTES],
            original_scratch_block: None,
            original_blocks_generated: 0,
        }
    }
    pub(crate) fn read_exact_expected(&mut self, mut output: &mut [u8]) -> EvalResult<()> {
        while !output.is_empty() {
            let piece = self
                .table
                .pieces
                .get(self.piece_index)
                .ok_or_else(|| "physical/canonical stream exceeded oracle".to_owned())?;
            let remaining = piece
                .length()
                .checked_sub(self.within_piece)
                .ok_or_else(|| "piece cursor underflow".to_owned())?;
            let take = output
                .len()
                .min(usize::try_from(remaining).map_err(display_error)?);
            match piece {
                Piece::Original { offset, .. } => {
                    let source = offset
                        .checked_add(self.within_piece)
                        .ok_or_else(|| "original cursor overflow".to_owned())?;
                    let block = source / BUFFER_BYTES as u64 * BUFFER_BYTES as u64;
                    if self.original_scratch_block != Some(block) {
                        stage1_fixture::fill_retained_buffer(&mut self.original_scratch, block);
                        self.original_scratch_block = Some(block);
                        self.original_blocks_generated = self
                            .original_blocks_generated
                            .checked_add(1)
                            .ok_or_else(|| "original oracle block count overflow".to_owned())?;
                    }
                    let within = usize::try_from(source - block).map_err(display_error)?;
                    let block_take = take.min(BUFFER_BYTES - within);
                    output[..block_take]
                        .copy_from_slice(&self.original_scratch[within..within + block_take]);
                    self.advance(block_take)?;
                    output = &mut output[block_take..];
                }
                Piece::Inserted { offset, .. } => {
                    let start = offset
                        .checked_add(usize::try_from(self.within_piece).map_err(display_error)?)
                        .ok_or_else(|| "inserted cursor overflow".to_owned())?;
                    let end = start
                        .checked_add(take)
                        .ok_or_else(|| "inserted cursor end overflow".to_owned())?;
                    output[..take].copy_from_slice(
                        self.backing
                            .get(start..end)
                            .ok_or_else(|| "inserted cursor exceeds backing".to_owned())?,
                    );
                    self.advance(take)?;
                    output = &mut output[take..];
                }
            }
        }
        Ok(())
    }
    pub(crate) fn advance(&mut self, bytes: usize) -> EvalResult<()> {
        let bytes = u64::try_from(bytes).map_err(display_error)?;
        self.within_piece = self
            .within_piece
            .checked_add(bytes)
            .ok_or_else(|| "piece cursor offset overflow".to_owned())?;
        self.position = self
            .position
            .checked_add(bytes)
            .ok_or_else(|| "piece cursor position overflow".to_owned())?;
        if self.within_piece
            == self
                .table
                .pieces
                .get(self.piece_index)
                .ok_or_else(|| "piece cursor index overflow".to_owned())?
                .length()
        {
            self.piece_index += 1;
            self.within_piece = 0;
        }
        Ok(())
    }
    pub(crate) fn finish(&self) -> EvalResult<()> {
        if self.position != self.table.logical_length
            || self.piece_index != self.table.pieces.len()
            || self.within_piece != 0
        {
            return Err(format!(
                "oracle stream length mismatch: position={} expected={} piece={}/{} within={}",
                self.position,
                self.table.logical_length,
                self.piece_index,
                self.table.pieces.len(),
                self.within_piece
            ));
        }
        Ok(())
    }
}
pub(crate) struct PieceCompareWriter<'a> {
    pub(crate) cursor: PieceCursor<'a>,
    pub(crate) expected: Vec<u8>,
    pub(crate) hasher: blake3::Hasher,
}
impl<'a> PieceCompareWriter<'a> {
    pub(crate) fn new(table: &'a PieceTable, backing: &'a [u8]) -> Self {
        Self {
            cursor: PieceCursor::new(table, backing),
            expected: vec![0_u8; BUFFER_BYTES],
            hasher: blake3::Hasher::new(),
        }
    }
    pub(crate) fn finish(self) -> EvalResult<String> {
        self.cursor.finish()?;
        Ok(self.hasher.finalize().to_hex().to_string())
    }
}
impl Write for PieceCompareWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.expected.len() {
            return Err(std::io::Error::other(
                "product write exceeds 1 MiB oracle buffer",
            ));
        }
        self.cursor
            .read_exact_expected(&mut self.expected[..bytes.len()])
            .map_err(std::io::Error::other)?;
        if self.expected[..bytes.len()] != *bytes {
            return Err(std::io::Error::other("independent byte oracle mismatch"));
        }
        self.hasher.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(crate) fn compare_managed(
    managed: &crate::legacy_full::ManagedWorkspace,
    table: &PieceTable,
    backing: &[u8],
) -> EvalResult<(String, crate::legacy_full::OperationDiagnostics)> {
    let mut sink = PieceCompareWriter::new(table, backing);
    let counters = managed
        .read_to(FILE_PATH, &mut sink)
        .map_err(display_error)?;
    let digest = sink.finish()?;
    Ok((digest, counters))
}
pub(crate) fn compare_canonical(
    fs: &LayerFs,
    root: RootId,
    table: &PieceTable,
    backing: &[u8],
) -> EvalResult<(String, crate::legacy_full::OperationDiagnostics)> {
    let mut sink = PieceCompareWriter::new(table, backing);
    let counters = fs
        .read_to(root, FILE_PATH, &mut sink)
        .map_err(display_error)?;
    let digest = sink.finish()?;
    Ok((digest, counters))
}
pub(crate) fn compare_canonical_range(
    fs: &LayerFs,
    root: RootId,
    table: &PieceTable,
    backing: &[u8],
    start: u64,
    length: u64,
) -> EvalResult<crate::legacy_full::OperationDiagnostics> {
    let range = table.range(start, length)?;
    let mut sink = PieceCompareWriter::new(&range, backing);
    let counters = fs
        .read_range(
            root,
            FILE_PATH,
            start
                ..start
                    .checked_add(length)
                    .ok_or_else(|| "canonical range overflow".to_owned())?,
            &mut sink,
        )
        .map_err(display_error)?;
    sink.finish()?;
    Ok(counters)
}
pub(crate) fn compare_external(
    external: &crate::legacy_full::ExternalWorkspace,
    table: &PieceTable,
    backing: &[u8],
) -> EvalResult<String> {
    let mut file = File::open(external.path().join(FILE_PATH)).map_err(io_error)?;
    let mut sink = PieceCompareWriter::new(table, backing);
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    loop {
        let read = file.read(&mut buffer).map_err(io_error)?;
        if read == 0 {
            break;
        }
        sink.write_all(&buffer[..read]).map_err(io_error)?;
    }
    sink.finish()
}
pub(crate) fn metadata_exact(actual: &NativeMetadata, expected: &NativeMetadata) -> bool {
    actual == expected
}
pub(crate) fn verify_supported_metadata(metadata: &NativeMetadata, label: &str) -> EvalResult<()> {
    if metadata.mode != FIXTURE_MODE
        || metadata.mtime_nanoseconds >= 1_000_000_000
        || !metadata.xattrs.is_empty()
        || metadata.acl.is_some()
        || metadata.bsd_flags != 0
    {
        return Err(format!(
            "{label} mode/xattr/ACL/BSD-flags frozen supported invariant"
        ));
    }
    Ok(())
}
pub(crate) fn metadata_receipt_json(metadata: &NativeMetadata) -> String {
    let xattrs = metadata
        .xattrs
        .iter()
        .map(|(name, value)| {
            format!(
                "{{\"name_hex\":\"{}\",\"value_hex\":\"{}\"}}",
                hex(&name),
                hex(&value)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let acl_hex = metadata
        .acl
        .as_ref()
        .map_or_else(|| "null".to_owned(), |acl| format!("\"{}\"", hex(acl)));
    format!(
        concat!(
            "{{\"mode\":{},\"mtime_seconds\":{},\"mtime_nanoseconds\":{},",
            "\"xattr_count\":{},\"xattrs\":[{}],",
            "\"acl_present\":{},\"acl_hex\":{},\"bsd_flags\":{}}}"
        ),
        metadata.mode,
        metadata.mtime_seconds,
        metadata.mtime_nanoseconds,
        metadata.xattrs.len(),
        xattrs,
        metadata.acl.is_some(),
        acl_hex,
        metadata.bsd_flags,
    )
}
pub(crate) fn native_route_name(route: Option<crate::legacy_full::NativeRoute>) -> &'static str {
    match route {
        None => "NotApplicable",
        Some(crate::legacy_full::NativeRoute::ExactNoop) => "ExactNoop",
        Some(crate::legacy_full::NativeRoute::ClonePatch) => "ClonePatch",
        Some(crate::legacy_full::NativeRoute::CloneShift) => "CloneShift",
        Some(crate::legacy_full::NativeRoute::InPlacePatch) => "InPlacePatch",
        Some(crate::legacy_full::NativeRoute::InPlaceShift) => "InPlaceShift",
        Some(crate::legacy_full::NativeRoute::FullFallback) => "FullFallback",
        Some(crate::legacy_full::NativeRoute::MaterializeStream)
        | Some(crate::legacy_full::NativeRoute::NativeDurableOutput)
        | Some(crate::legacy_full::NativeRoute::CaptureStream)
        | Some(crate::legacy_full::NativeRoute::Rename)
        | Some(crate::legacy_full::NativeRoute::ProtectedExactNoop) => "NotApplicable",
    }
}
pub(crate) fn verify_native_edit(
    edit: &EditSpec,
    operation: &crate::legacy_full::OperationDiagnostics,
) -> EvalResult<()> {
    let native = operation.native;
    if edit.delete_bytes == edit.insert_bytes {
        if !matches!(
            native.route,
            Some(
                crate::legacy_full::NativeRoute::ClonePatch
                    | crate::legacy_full::NativeRoute::InPlacePatch
            )
        ) || native.patch_bytes != edit.insert_bytes
            || native.suffix_bytes_shifted != 0
            || native.bytes_written != edit.insert_bytes
            || native.clone_attempts != 1
            || (native.route == Some(crate::legacy_full::NativeRoute::ClonePatch)
                && (native.clone_successes != 1 || native.clone_fallbacks != 0))
            || (native.route == Some(crate::legacy_full::NativeRoute::InPlacePatch)
                && (native.clone_successes != 0 || native.clone_fallbacks != 1))
        {
            return Err(format!("{} same-length native route equation", edit.tag));
        }
    } else {
        let suffix = edit
            .before_bytes
            .checked_sub(
                edit.offset
                    .checked_add(edit.delete_bytes)
                    .ok_or_else(|| "native suffix equation overflow".to_owned())?,
            )
            .ok_or_else(|| "native suffix equation underflow".to_owned())?;
        if native.route != Some(crate::legacy_full::NativeRoute::InPlaceShift)
            || native.suffix_bytes_shifted != suffix
            || native.bytes_read != suffix
            || native.bytes_written
                != suffix
                    .checked_add(edit.insert_bytes)
                    .ok_or_else(|| "native write equation overflow".to_owned())?
        {
            return Err(format!(
                "{} count-changing native read=S; write=S+B equation",
                edit.tag
            ));
        }
    }
    if operation.operation_q_terminal_bytes != 0
        || operation.operation_q_high_water_bytes > 8_388_608
    {
        return Err(format!("{} operation Q closure", edit.tag));
    }
    Ok(())
}
pub(crate) fn verify_refresh(
    edit: &EditSpec,
    operation: &crate::legacy_full::OperationDiagnostics,
) -> EvalResult<()> {
    if edit.kind == EditKind::Overwrite {
        if !matches!(
            operation.native.route,
            Some(
                crate::legacy_full::NativeRoute::ClonePatch
                    | crate::legacy_full::NativeRoute::InPlacePatch
            )
        ) || operation.full_fallback_files != 0
            || operation.rematerializations != 0
        {
            return Err(format!("{} same-size refresh route", edit.tag));
        }
    } else {
        let suffix = edit
            .before_bytes
            .checked_sub(
                edit.offset
                    .checked_add(edit.delete_bytes)
                    .ok_or_else(|| "refresh suffix equation overflow".to_owned())?,
            )
            .ok_or_else(|| "refresh suffix equation underflow".to_owned())?;
        if !matches!(
            operation.native.route,
            Some(
                crate::legacy_full::NativeRoute::CloneShift
                    | crate::legacy_full::NativeRoute::InPlaceShift
            )
        ) || operation.full_fallback_files != 0
            || operation.rematerializations != 0
            || operation.native.suffix_bytes_shifted != suffix
            || operation.native.bytes_read != suffix
            || operation.native.bytes_written
                != suffix
                    .checked_add(edit.insert_bytes)
                    .ok_or_else(|| "refresh write equation overflow".to_owned())?
            || operation.native.patch_bytes != edit.insert_bytes
            || (operation.native.route == Some(crate::legacy_full::NativeRoute::CloneShift)
                && (operation.native.clone_attempts != 1
                    || operation.native.clone_successes != 1
                    || operation.native.clone_fallbacks != 0))
            || (operation.native.route == Some(crate::legacy_full::NativeRoute::InPlaceShift)
                && (operation.native.clone_successes != 0
                    || operation.native.clone_attempts != operation.native.clone_fallbacks))
        {
            return Err(format!(
                "{} count-changing shift read=S; write=S+B route",
                edit.tag
            ));
        }
    }
    if operation.workspace_reuses != 1
        || operation.operation_q_terminal_bytes != 0
        || operation.operation_q_high_water_bytes > 8_388_608
    {
        return Err(format!("{} refresh reuse/Q closure", edit.tag));
    }
    Ok(())
}
pub(crate) fn verify_storage_transition(
    before: &Diagnostics,
    after: &Diagnostics,
) -> EvalResult<()> {
    let (before_database, after_database) = before
        .database_bytes
        .zip(after.database_bytes)
        .ok_or_else(|| "transition database_bytes observation is required".to_owned())?;
    let (before_engine, after_engine) = before
        .logical_engine_bytes
        .zip(after.logical_engine_bytes)
        .ok_or_else(|| "transition logical_engine_bytes observation is required".to_owned())?;
    if after_database < before_database
        || after_engine < before_engine
        || after.object_bytes_written < before.object_bytes_written
    {
        return Err(format!(
            concat!(
                "transition storage monotonicity database={}->{} ",
                "logical_engine={}->{} object_bytes_written={}->{}"
            ),
            before_database,
            after_database,
            before_engine,
            after_engine,
            before.object_bytes_written,
            after.object_bytes_written
        ));
    }
    Ok(())
}
pub(crate) fn combine_physical_checkpoint(
    native: crate::legacy_full::OperationDiagnostics,
    checkpoint: crate::legacy_full::OperationDiagnostics,
) -> EvalResult<crate::legacy_full::OperationDiagnostics> {
    native.merge(checkpoint).map_err(display_error)
}
pub(crate) fn combine_logical_refresh(
    mut logical: crate::legacy_full::OperationDiagnostics,
    refresh: crate::legacy_full::OperationDiagnostics,
) -> EvalResult<crate::legacy_full::OperationDiagnostics> {
    logical.native = refresh.native;
    logical.workspace_reuses = refresh.workspace_reuses;
    logical.rematerializations = refresh.rematerializations;
    logical.full_fallback_files = refresh.full_fallback_files;
    logical.root_diff_nodes = refresh.root_diff_nodes;
    logical.changed_paths = refresh.changed_paths;
    logical.plan_rows = refresh.plan_rows;
    logical.plan_scratch_high_water_bytes = refresh.plan_scratch_high_water_bytes;
    logical.scratch_tables = refresh.scratch_tables;
    logical.scratch_statements = refresh.scratch_statements;
    logical.scratch_rows = refresh.scratch_rows;
    logical.scratch_high_water_bytes = refresh.scratch_high_water_bytes;
    logical.operation_q_current_bytes = logical
        .operation_q_current_bytes
        .max(refresh.operation_q_current_bytes);
    logical.operation_q_high_water_bytes = logical
        .operation_q_high_water_bytes
        .max(refresh.operation_q_high_water_bytes);
    logical.operation_q_terminal_bytes = logical
        .operation_q_terminal_bytes
        .max(refresh.operation_q_terminal_bytes);
    Ok(logical)
}
