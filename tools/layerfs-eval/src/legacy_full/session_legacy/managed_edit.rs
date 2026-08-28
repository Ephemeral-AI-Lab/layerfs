use super::super::{NativeOperationCounters, NativeRoute, OperationCounters};
use layerfs_core::CanonicalPath;
use std::io::{Read, Seek, SeekFrom, Write};

use super::managed_state::managed_rename_edges;
use super::{ExternalWorkspace, ManagedState, ManagedWorkspace, VfsError, VfsResult};
pub(super) enum SpoolPart<'a> {
    Bytes(&'a [u8]),
    Metadata(&'a layerfs_materialization::driver::NativeMetadata),
}

impl ManagedWorkspace {
    fn replace_observed_canonical(
        &mut self,
        path: &CanonicalPath,
        start: u64,
        delete_len: u64,
        bytes: &[u8],
    ) -> VfsResult<OperationCounters> {
        self.require_editable()?;
        let reservation = self
            .external
            .as_ref()
            .ok_or(VfsError::InvalidState)?
            .operation_q
            .reserve();
        if self.edits.len() == 64 {
            return Err(VfsError::InvalidState);
        }
        let replacement_len = u64::try_from(bytes.len()).map_err(|_| VfsError::InvalidState)?;
        let external = self.external.as_mut().ok_or(VfsError::InvalidState)?;
        let scratch_before = external.live_scratch_observation()?;
        let old_hard_link_key = match super::super::managed_edit_legacy::native_hard_link_key(
            external.native.as_ref(),
            path,
        ) {
            Ok(key) => key,
            Err(error) => {
                self.state = ManagedState::ExternalDirtyConflict;
                return Err(error);
            }
        };
        let (metadata, native_counters, sync_required) =
            match super::super::managed_edit_legacy::mutate_native(
                external.native.as_ref(),
                path,
                start,
                delete_len,
                bytes,
            ) {
                Ok(metadata) => metadata,
                Err(error @ VfsError::NativeProtected) => return Err(error),
                Err(error) => {
                    self.state = ManagedState::Indeterminate;
                    return Err(error);
                }
            };
        let new_hard_link_key = match super::super::managed_edit_legacy::native_hard_link_key(
            external.native.as_ref(),
            path,
        ) {
            Ok(key) => key,
            Err(error) => {
                external.live_scratch = None;
                self.state = ManagedState::Indeterminate;
                return Err(error);
            }
        };
        if old_hard_link_key != new_hard_link_key {
            let transfer = external
                .live_scratch
                .as_ref()
                .ok_or(VfsError::InvalidState)
                .and_then(|scratch| {
                    let authority = scratch.namespace(b"authority")?;
                    let inode = authority
                        .get(&old_hard_link_key)?
                        .ok_or(VfsError::InvalidState)?;
                    authority.put(&new_hard_link_key, &inode)?;
                    authority.remove(&old_hard_link_key)?;
                    Ok(())
                });
            if let Err(error) = transfer {
                external.live_scratch = None;
                self.state = ManagedState::Indeterminate;
                return Err(error);
            }
        }
        let _ = external;
        let (offsets, spool_bytes) =
            self.append_spool_parts(&[SpoolPart::Bytes(bytes), SpoolPart::Metadata(&metadata)])?;
        let (spool_offset, _) = offsets[0];
        let (metadata_offset, metadata_len) = offsets[1];
        self.edits
            .push(super::super::managed_edit_legacy::ManagedEdit::Replace {
                path: path.clone(),
                start,
                delete_len,
                spool_offset,
                replacement_len,
                metadata_offset,
                metadata_len,
                sync_required,
                native_identity: new_hard_link_key,
            });
        self.state = ManagedState::Dirty;
        let mut counters = OperationCounters {
            native: native_counters,
            ..OperationCounters::default()
        };
        self.external
            .as_ref()
            .ok_or(VfsError::InvalidState)?
            .add_live_scratch_delta(scratch_before, &mut counters)?;
        Self::record_spool_observation(&mut counters, Some(spool_bytes));
        reservation.finish(&mut counters);
        Ok(counters)
    }
    pub fn replace_observed(
        &mut self,
        path: &str,
        start: u64,
        delete_len: u64,
        bytes: &[u8],
    ) -> VfsResult<OperationCounters> {
        self.replace_observed_canonical(&CanonicalPath::new(path)?, start, delete_len, bytes)
    }
    fn rename_observed_canonical(
        &mut self,
        from: &CanonicalPath,
        to: &CanonicalPath,
    ) -> VfsResult<OperationCounters> {
        self.require_editable()?;
        let reservation = self
            .external
            .as_ref()
            .ok_or(VfsError::InvalidState)?
            .operation_q
            .reserve();
        if self.edits.len() == 64 {
            return Err(VfsError::InvalidState);
        }
        let external = self.external.as_mut().ok_or(VfsError::InvalidState)?;
        let scratch_before = external.live_scratch_observation()?;
        let mut topology_counters = OperationCounters::default();
        let (old_edge, new_edge) = managed_rename_edges(
            &external.engine,
            &external.expected,
            &self.edits,
            from,
            to,
            &mut topology_counters,
        )?;
        let topology = external
            .live_scratch
            .as_ref()
            .ok_or(VfsError::InvalidState)?
            .namespace(b"topology")?;
        if topology.get(&old_edge)?.is_none() {
            return Err(VfsError::InvalidState);
        }
        let result =
            super::super::managed_edit_legacy::rename_native(external.native.as_ref(), from, to);
        match result {
            Ok((source_parent_metadata, target_parent_metadata)) => {
                let topology_update = external
                    .live_scratch
                    .as_ref()
                    .ok_or(VfsError::InvalidState)
                    .and_then(|scratch| {
                        let topology = scratch.namespace(b"topology")?;
                        topology.remove(&old_edge)?;
                        topology.put(&new_edge, &[])?;
                        Ok(())
                    });
                if let Err(error) = topology_update {
                    external.live_scratch = None;
                    self.state = ManagedState::Indeterminate;
                    return Err(error);
                }
                let _ = external;
                let (offsets, spool_bytes) = self.append_spool_parts(&[
                    SpoolPart::Metadata(&source_parent_metadata),
                    SpoolPart::Metadata(&target_parent_metadata),
                ])?;
                let (source_metadata_offset, source_metadata_len) = offsets[0];
                let (target_metadata_offset, target_metadata_len) = offsets[1];
                self.edits
                    .push(super::super::managed_edit_legacy::ManagedEdit::Rename {
                        from: from.clone(),
                        to: to.clone(),
                        source_metadata_offset,
                        source_metadata_len,
                        target_metadata_offset,
                        target_metadata_len,
                    });
                self.state = ManagedState::Dirty;
                let mut counters = topology_counters.merge(OperationCounters {
                    native: NativeOperationCounters {
                        route: Some(NativeRoute::Rename),
                        ..NativeOperationCounters::default()
                    },
                    ..OperationCounters::default()
                })?;
                self.external
                    .as_ref()
                    .ok_or(VfsError::InvalidState)?
                    .add_live_scratch_delta(scratch_before, &mut counters)?;
                Self::record_spool_observation(&mut counters, Some(spool_bytes));
                reservation.finish(&mut counters);
                Ok(counters)
            }
            Err(error @ VfsError::NativeProtected) | Err(error @ VfsError::InvalidState) => {
                Err(error)
            }
            Err(error) => {
                self.state = ManagedState::Indeterminate;
                Err(error)
            }
        }
    }
    pub fn rename_observed(&mut self, from: &str, to: &str) -> VfsResult<OperationCounters> {
        self.rename_observed_canonical(&CanonicalPath::new(from)?, &CanonicalPath::new(to)?)
    }
    pub fn into_external(mut self) -> VfsResult<ExternalWorkspace> {
        self.require_editable()?;
        let _reservation = self
            .external
            .as_ref()
            .ok_or(VfsError::InvalidState)?
            .operation_q
            .reserve();
        self.remove_spool()?;
        self.external.take().ok_or(VfsError::InvalidState)
    }
    pub fn discard(&mut self) -> VfsResult<()> {
        self.discard_observed().map(drop)
    }
    pub fn discard_observed(&mut self) -> VfsResult<OperationCounters> {
        let reservation = self
            .external
            .as_ref()
            .map(|external| external.operation_q.reserve());
        let mut counters = OperationCounters::default();
        if let Some(mut external) = self.external.take() {
            match external.discard_inner() {
                Ok(cleanup) => counters = counters.merge(cleanup)?,
                Err(error) => {
                    self.external = Some(external);
                    return Err(error);
                }
            }
        }
        self.remove_spool()?;
        self.state = ManagedState::Closed;
        self.observe_spool(&mut counters)?;
        if let Some(reservation) = reservation {
            reservation.finish(&mut counters);
        }
        Ok(counters)
    }
    pub(super) fn remove_spool(&mut self) -> VfsResult<()> {
        self.spool.take();
        Ok(())
    }
    pub(super) fn append_spool_parts(
        &mut self,
        parts: &[SpoolPart<'_>],
    ) -> VfsResult<(Vec<(u64, u64)>, u64)> {
        let start = match self.spool.as_mut() {
            Some(spool) => spool.seek(SeekFrom::End(0)).map_err(VfsError::from),
            None => Err(VfsError::InvalidState),
        };
        let start = match start {
            Ok(start) => start,
            Err(error) => {
                self.state = ManagedState::Indeterminate;
                return Err(error);
            }
        };
        let append = (|| {
            let spool = self.spool.as_mut().ok_or(VfsError::InvalidState)?;
            let mut offsets = Vec::with_capacity(parts.len());
            let mut offset = start;
            for part in parts {
                let len = match part {
                    SpoolPart::Bytes(bytes) => {
                        let len = u64::try_from(bytes.len()).map_err(|_| VfsError::InvalidState)?;
                        spool.write_all(bytes)?;
                        len
                    }
                    SpoolPart::Metadata(metadata) => {
                        super::super::managed_edit_legacy::write_spooled_metadata(
                            metadata,
                            spool.as_mut(),
                        )?
                    }
                };
                offsets.push((offset, len));
                offset = offset.checked_add(len).ok_or(VfsError::InvalidState)?;
            }
            Ok((offsets, offset))
        })();
        match append {
            Ok(offsets) => Ok(offsets),
            Err(error) => {
                self.state = ManagedState::Indeterminate;
                self.restore_spool_prefix(start)
                    .map_err(|_| VfsError::Indeterminate)?;
                Err(error)
            }
        }
    }
    pub(super) fn restore_spool_prefix(&mut self, len: u64) -> VfsResult<()> {
        let external = self.external.as_ref().ok_or(VfsError::InvalidState)?;
        let root = external.native.root_directory()?;
        let mut replacement = external.native.create_temp_at(root.as_ref())?;
        let spool = self.spool.as_mut().ok_or(VfsError::InvalidState)?;
        spool.seek(SeekFrom::Start(0))?;
        let copied = std::io::copy(&mut spool.take(len), replacement.as_mut())?;
        if copied != len {
            return Err(VfsError::InvalidState);
        }
        self.spool = Some(replacement);
        Ok(())
    }
    pub(super) fn observe_spool(&mut self, counters: &mut OperationCounters) -> VfsResult<()> {
        let observation = (|| {
            if let Some(spool) = self.spool.as_mut() {
                let position = spool.stream_position()?;
                let bytes = spool.seek(SeekFrom::End(0))?;
                spool.seek(SeekFrom::Start(position))?;
                Ok(Some(bytes))
            } else {
                Ok(None)
            }
        })();
        match observation {
            Ok(bytes) => {
                Self::record_spool_observation(counters, bytes);
                Ok(())
            }
            Err(error) => {
                self.state = ManagedState::Indeterminate;
                Err(error)
            }
        }
    }
    pub(super) fn record_spool_observation(counters: &mut OperationCounters, bytes: Option<u64>) {
        counters.owned_temp_current = u64::from(bytes.is_some());
        counters.owned_temp_terminal = counters.owned_temp_current;
        counters.descriptor_spool_bytes_current = bytes.unwrap_or(0);
        counters.descriptor_spool_bytes_terminal = counters.descriptor_spool_bytes_current;
    }
    pub(super) fn require_live(&self) -> VfsResult<()> {
        match self.state {
            ManagedState::Live => Ok(()),
            ManagedState::Dirty => Err(VfsError::InvalidState),
            ManagedState::Refreshing => Err(VfsError::InvalidState),
            ManagedState::ExternalDirtyConflict => Err(VfsError::ExternalDirtyConflict),
            ManagedState::Indeterminate => Err(VfsError::Indeterminate),
            ManagedState::IncompleteDerived => Err(VfsError::IncompleteDerived),
            ManagedState::Closed => Err(VfsError::InvalidState),
        }
    }
    pub(super) fn require_editable(&self) -> VfsResult<()> {
        match self.state {
            ManagedState::Live | ManagedState::Dirty => Ok(()),
            ManagedState::Refreshing => Err(VfsError::InvalidState),
            ManagedState::ExternalDirtyConflict => Err(VfsError::ExternalDirtyConflict),
            ManagedState::Indeterminate => Err(VfsError::Indeterminate),
            ManagedState::IncompleteDerived => Err(VfsError::IncompleteDerived),
            ManagedState::Closed => Err(VfsError::InvalidState),
        }
    }
    pub(super) fn require_checkpointable(&self) -> VfsResult<()> {
        self.require_editable()?;
        if (self.state == ManagedState::Live) == self.edits.is_empty() {
            Ok(())
        } else {
            Err(VfsError::InvalidState)
        }
    }
}
