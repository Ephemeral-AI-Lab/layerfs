//! Explicit real established provider participation, separate from compatibility.
use super::Store;
use crate::sqlite::connection;
use crate::{engine::EngineGuard, policy::StoragePolicy, StorageResult};
use layerfs_telemetry::timer::TimingScope;
use std::path::Path;
impl Store {
    /// Established native guard before connection/schema/Save effects.
    pub fn create_guarded(
        path: impl AsRef<Path>,
        policy: StoragePolicy,
        engine: &'static EngineGuard,
        scope: TimingScope<'_>,
    ) -> StorageResult<Self> {
        engine.validate()?;
        Self::create_participating(path, policy, Some(engine), scope)
    }
    /// Reopen under the same established actual provider; no silent compatibility fallback.
    pub fn open_guarded(
        path: impl AsRef<Path>,
        engine: &'static EngineGuard,
        scope: TimingScope<'_>,
    ) -> StorageResult<Self> {
        engine.validate()?;
        Self::open_participating(path, Some(engine), scope)
    }
    pub(super) fn open_selected(
        path: &Path,
        create: bool,
        engine: Option<&'static crate::engine::EngineGuard>,
        class: crate::engine::ConnectionClass,
    ) -> StorageResult<rusqlite::Connection> {
        match engine {
            Some(guard) => connection::open_guarded(path, create, guard, class),
            None => connection::open(path, create),
        }
    }
}
