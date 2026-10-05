//! Initialized host handles outside the borrowed serving registry.
use super::{Authorization, RuntimeError, RuntimeResult, Sessions};
use layerfs_history::HistoryCatalog;
use layerfs_persistence::{Handles, HistoryProvider};
use layerfs_storage::Storage;

/// Processing admission and owner identity, chosen once by the application.
pub struct Config {
    /// Fresh nonzero authority-assigned runtime incarnation; never reused.
    pub incarnation: [u8; 32],
    /// Concurrent live/retained Saves, not a total flow or duration limit.
    pub save_slots: usize,
}

/// Application-embedded runtime. It never creates a provider or a daemon.
pub struct Runtime {
    pub(super) owners: Vec<Storage>,
    pub(super) demand: Storage,
    pub(super) history: HistoryProvider,
    pub(super) authority: Box<dyn Authorization>,
    pub(super) incarnation: [u8; 32],
    pub(super) next_serial: u64,
}
impl Runtime {
    /// Initializes bounded Storage handles once over the supplied open provider.
    pub fn new(
        handles: Handles,
        config: Config,
        authority: Box<dyn Authorization>,
    ) -> RuntimeResult<Self> {
        if config.incarnation == [0; 32] || config.save_slots == 0 {
            return Err(RuntimeError::Invalid("runtime incarnation/session slots"));
        }
        let mut owners = Vec::new();
        owners
            .try_reserve_exact(config.save_slots)
            .map_err(|_| RuntimeError::AdmissionUnavailable)?;
        for _ in 0..config.save_slots {
            owners.push(Storage::new(handles.storage.clone())?);
        }
        Ok(Self {
            owners,
            demand: Storage::new(handles.storage)?,
            history: handles.history,
            authority,
            incarnation: config.incarnation,
            next_serial: 1,
        })
    }

    /// Starts the host serving scope; Saves borrow initialized owners on its stack.
    ///
    /// Keep this scope alive independently of individual transport connections.
    /// No self-reference, lifetime extension or whole-Commit lock is introduced.
    pub fn sessions(&mut self) -> Sessions<'_> {
        Sessions::new(self)
    }

    /// Bound authority identity, without querying or reopening the provider.
    pub fn catalog(&self) -> layerfs_history::CatalogId {
        self.history.catalog_id()
    }
}
