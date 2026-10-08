//! Original file publication and opened-Store custody during native installation.
use crate::{bootstrap::OpenedStore, store::PortError};
use layerfs_bridge::{
    control::{ControlError, InstallPhase, InstallRefusal, RefusalKind},
    native::ChannelError,
    provision::StoreManifest,
};
use layerfs_content::ContentError;
use layerfs_history::HistoryError;
use layerfs_storage::{ReservationBlocks, StorageError};
use std::{fmt, io, path::PathBuf, sync::Arc};

/// Fixed startup resources; none is a total Store/namespace/file size limit.
#[derive(Clone, Copy, Debug)]
pub struct StoreSettings {
    pub read_handles: usize,
    pub cache_bytes: usize,
    pub reservations: ReservationBlocks,
    pub read_limits: crate::store::ReadLimits,
}
impl Default for StoreSettings {
    fn default() -> Self {
        Self {
            read_handles: 4,
            cache_bytes: 8 * 1024 * 1024,
            reservations: ReservationBlocks::default(),
            read_limits: crate::store::ReadLimits::default(),
        }
    }
}
/// Actual file work in one attempted installation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InstallWork {
    pub records: u64,
    pub write_calls: u64,
    pub written: u64,
    pub sync_calls: u64,
    pub peak_record_bytes: usize,
}
/// Original successful rename/open/acknowledgement and retained data-plane owner.
pub struct InstalledStore {
    pub manifest: StoreManifest,
    pub opened: OpenedStore,
    pub work: InstallWork,
}
#[derive(Debug)]
pub enum InstallError {
    Io(io::Error),
    Channel(ChannelError),
    Protocol(ControlError),
    Store(StorageError),
    History(HistoryError),
    Content {
        error: ContentError,
        provider: Option<Arc<PortError>>,
    },
}
/// Retained partial/output paths and original knowledge; no guessed deletion.
pub struct InstallFailure {
    pub destination: PathBuf,
    pub temporary: Option<PathBuf>,
    /// True only when this attempt's create_new returned its file handle.
    pub claim_acknowledged: bool,
    pub manifest: Option<StoreManifest>,
    pub phase: InstallPhase,
    pub published: bool,
    pub opened: Option<OpenedStore>,
    pub work: InstallWork,
    pub error: InstallError,
    pub reply_error: Option<InstallError>,
    pub fence_error: Option<ChannelError>,
}
impl InstallFailure {
    pub(super) fn refusal(&self) -> InstallRefusal {
        let kind =
            match &self.error {
                InstallError::Store(StorageError::Busy)
                | InstallError::History(HistoryError::Busy) => RefusalKind::Busy,
                InstallError::Store(error) if error.is_unknown_outcome() => RefusalKind::Unknown,
                InstallError::History(error) if error.unknown() => RefusalKind::Unknown,
                InstallError::Io(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    RefusalKind::Exists
                }
                InstallError::Io(_) if self.phase == InstallPhase::Publish => RefusalKind::Unknown,
                InstallError::Content {
                    provider: Some(error),
                    ..
                } => match error.as_ref() {
                    PortError::Storage(StorageError::Busy)
                    | PortError::History(HistoryError::Busy) => RefusalKind::Busy,
                    PortError::Storage(error) if error.is_unknown_outcome() => RefusalKind::Unknown,
                    PortError::History(error) if error.unknown() => RefusalKind::Unknown,
                    PortError::Poisoned => RefusalKind::Unknown,
                    _ => RefusalKind::Failed,
                },
                InstallError::Content {
                    error: ContentError::ProviderFailure { .. } | ContentError::OutputRejected,
                    provider: None,
                } => RefusalKind::Unknown,
                InstallError::Protocol(_) => RefusalKind::Invalid,
                _ => RefusalKind::Failed,
            };
        InstallRefusal {
            phase: self.phase,
            kind,
            claim_acknowledged: self.claim_acknowledged,
            written: self.work.written,
            published: self.published,
            os_code: match &self.error {
                InstallError::Io(error) => error.raw_os_error(),
                _ => None,
            },
            detail: format!("{:?}", self.error),
        }
    }
}
impl fmt::Debug for InstallFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InstallFailure")
            .field("destination", &self.destination)
            .field("temporary", &self.temporary)
            .field("claim_acknowledged", &self.claim_acknowledged)
            .field("manifest", &self.manifest)
            .field("phase", &self.phase)
            .field("published", &self.published)
            .field("opened", &self.opened.is_some())
            .field("work", &self.work)
            .field("error", &self.error)
            .field("reply_error", &self.reply_error)
            .field("fence_error", &self.fence_error)
            .finish()
    }
}
impl fmt::Display for InstallFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Store install at {:?}: {:?}", self.phase, self.error)
    }
}
impl std::error::Error for InstallFailure {}
