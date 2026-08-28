use super::{Diagnostics, IntegrityMode, LayerFs, RefState, RootId, VfsError};
use layerfs_storage::generation::{self, NativeGenerationDriver};
use std::path::Path;

pub(crate) struct OpenedLayerFs {
    pub(crate) fs: LayerFs,
    pub(crate) head: RootId,
    pub(crate) ref_state: RefState,
}

impl LayerFs {
    pub(crate) fn open(path: &Path) -> Result<OpenedLayerFs, VfsError> {
        Self::open_with_integrity(path, IntegrityMode::Verified)
    }

    pub(crate) fn open_with_integrity(
        path: &Path,
        mode: IntegrityMode,
    ) -> Result<OpenedLayerFs, VfsError> {
        let engine = generation::open_or_create(path, &NativeGenerationDriver, mode)?;
        let fs = Self::from_engine(engine, layerfs_materialization::host_driver(), mode)?;
        let ref_state = fs.current_head("main")?;
        Ok(OpenedLayerFs {
            head: ref_state.root,
            ref_state,
            fs,
        })
    }

    pub(crate) fn compact(self, path: &Path) -> Result<OpenedLayerFs, VfsError> {
        let mode = self.integrity_mode();
        let engine = self.into_engine()?;
        let engine = generation::compact(engine, path, &NativeGenerationDriver)?;
        let fs = Self::from_engine(engine, layerfs_materialization::host_driver(), mode)?;
        let ref_state = fs.current_head("main")?;
        Ok(OpenedLayerFs {
            head: ref_state.root,
            ref_state,
            fs,
        })
    }

    pub(crate) fn diagnostics(&self) -> Result<Diagnostics, VfsError> {
        let mut diagnostics = Diagnostics::exact(&self.engine, true, self.integrity_mode())?;
        let operation_q = self.operation_q_observation();
        diagnostics.operation_q_current_bytes = operation_q.current_bytes;
        diagnostics.operation_q_high_water_bytes = operation_q.high_water_bytes;
        Ok(diagnostics)
    }

    pub(crate) fn counter_snapshot(&self) -> Result<Diagnostics, VfsError> {
        let mut diagnostics = Diagnostics::exact(&self.engine, false, self.integrity_mode())?;
        let operation_q = self.operation_q_observation();
        diagnostics.operation_q_current_bytes = operation_q.current_bytes;
        diagnostics.operation_q_high_water_bytes = operation_q.high_water_bytes;
        Ok(diagnostics)
    }

    pub(crate) fn close_primary_connections(&self) -> Result<(), VfsError> {
        self.engine.close_primary_connection()?;
        Ok(())
    }
}
