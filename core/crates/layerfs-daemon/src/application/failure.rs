//! Original application operation custody, with credential-free diagnostics.
use crate::{
    control::{ServeFailure, Served},
    install_types::{InstallFailure, InstalledStore},
};
use layerfs_bridge::{control::ControlError, native::ChannelError};
use std::{fmt, io};

/// Exact startup or original connection failure, never followed by a replay.
pub enum ApplicationError {
    /// Before-effect configuration/listener/native file error.
    Io(io::Error),
    /// Original initialized Overlay owner failure.
    Owner(Box<crate::OwnerStart>),
    /// Original later startup failure, retaining the successfully initialized Overlay owner.
    AfterOverlay {
        /// Actual Overlay service/creation receipt; no cleanup or zero work is inferred.
        owner: Box<crate::Owner>,
        /// Exact subsequent startup boundary.
        cause: Box<ApplicationError>,
    },
    /// Original direct existing-Store open failure.
    Store(layerfs_storage::StorageError),
    /// Original authentication/native transport failure.
    Channel(ChannelError),
    /// Original malformed request, with bounded bytes retained separately.
    Protocol(ControlError),
    /// Original installation custody, including partial/output/opened owners.
    Install(Box<InstallFailure>),
    /// Original control outcome and failed delivery/fence.
    Control(Box<ServeFailure>),
    /// Original final session reply send completed; terminal peer observation/fence then failed.
    SessionEnd {
        /// Original call/outcome whose reply send completed before the terminal failure.
        served: Box<Served>,
        /// Original terminal byte/work/cause, not a replayed final reply.
        ending: Box<layerfs_bridge::native::PeerEndFailure>,
    },
    /// Original control receipt retained after an unresolved operation or a
    /// separate failed diagnostic transfer. Its known outcome stays unchanged.
    RetainedControl(Box<Served>),
    /// Original install result retained after application publication synchronization failed.
    InstallPublication(Box<Result<InstalledStore, Box<InstallFailure>>>),
    /// Actual application state synchronization failed; ownership remains held.
    Poisoned,
}
impl fmt::Display for ApplicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "daemon I/O: {e}"),
            Self::Owner(e) => write!(f, "Overlay startup: {:?}", e.result.as_ref().err()),
            Self::AfterOverlay { cause, .. } => write!(f, "after Overlay startup: {cause}"),
            Self::Store(e) => write!(f, "Store open: {e}"),
            Self::Channel(e) => write!(f, "native channel: {e}"),
            Self::Protocol(e) => write!(f, "native request: {e}"),
            Self::Install(e) => write!(f, "installation: {e}"),
            Self::Control(e) => write!(f, "control: {e}"),
            Self::SessionEnd { ending, .. } => {
                write!(f, "after original session reply: {}", ending.cause)
            }
            Self::RetainedControl(_) => f.write_str("original control receipt retained"),
            Self::InstallPublication(_) => {
                f.write_str("original install result retained; application publication poisoned")
            }
            Self::Poisoned => f.write_str("application state poisoned; original custody retained"),
        }
    }
}
impl std::error::Error for ApplicationError {}
/// Original failed connection admission/dispatch result and input custody.
pub struct ConnectionFailure {
    /// Exact admitted slot identity; never recycled while this custody remains.
    pub slot: usize,
    /// Original malformed metadata bytes only; successful requests own decoded fields.
    pub received: Option<Vec<u8>>,
    /// Owned original decoded request when a before-effect application boundary fails.
    pub original: Option<layerfs_bridge::initial_record::InitialRecord>,
    /// Exact original deciding boundary.
    pub cause: ApplicationError,
    /// Original failed socket fence, independent of the cause.
    pub fence_error: Option<ChannelError>,
    /// Distinct failed one-attempt diagnostic transfer; original cause is unchanged.
    pub diagnostic_error: Option<io::Error>,
}

impl fmt::Debug for ApplicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => f.debug_tuple("Io").field(error).finish(),
            Self::Channel(error) => f.debug_tuple("Channel").field(error).finish(),
            _ => fmt::Display::fmt(self, f),
        }
    }
}
impl fmt::Debug for ConnectionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnectionFailure")
            .field("slot", &self.slot)
            .field("received_bytes", &self.received.as_ref().map(Vec::len))
            .field(
                "original_kind",
                &self.original.as_ref().map(|request| match request {
                    layerfs_bridge::initial_record::InitialRecord::Control(_) => "control",
                    layerfs_bridge::initial_record::InitialRecord::Install(_) => "install",
                }),
            )
            .field("cause", &self.cause)
            .field("fence_error", &self.fence_error)
            .field("diagnostic_error", &self.diagnostic_error)
            .finish()
    }
}
