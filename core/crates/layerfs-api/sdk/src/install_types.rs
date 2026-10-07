//! Exact local stream progress and original remote acknowledgement custody.
use layerfs_bridge::{
    control::{ControlError, InstallPhase, InstallRefusal},
    native::ChannelError,
    provision::StoreManifest,
};
use std::{fmt, io, path::PathBuf};
/// Actual work from one host file stream, separate from channel crypto/I/O work.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InstallWork {
    /// Original file read attempts, including EOF and failed reads.
    pub read_calls: u64,
    /// Bytes returned by successful file reads.
    pub read_bytes: u64,
    /// Completed native data-record sends; no remote file-write inference.
    pub sent_records: u64,
    /// Bytes in completed native data-record sends.
    pub sent_bytes: u64,
    /// Fixed retained file window.
    pub buffer_bytes: usize,
}
/// Original acknowledged handoff and exact stream work.
#[derive(Debug)]
pub struct Installed {
    /// Original remote acknowledgement, with both SQLite versions.
    pub manifest: StoreManifest,
    /// Work of this single attempt.
    pub work: InstallWork,
}
/// Deciding original installation failure.
#[derive(Debug)]
pub enum InstallError {
    /// Local sealed-file I/O, never followed by another attempt.
    Io(io::Error),
    /// Authentication/record delivery error; remote disposition may be unknown.
    Channel(ChannelError),
    /// Exact control validation refusal.
    Protocol(ControlError),
    /// Original daemon refusal and its explicit publication knowledge.
    Remote(InstallRefusal),
}
/// The caller keeps the sealed source and original uncertainty; nothing is deleted.
#[derive(Debug)]
pub struct InstallFailure {
    /// Original local file, which was never opened as a database by install.
    pub source: PathBuf,
    /// Original manifest sent or refused before send.
    pub manifest: StoreManifest,
    /// Original attempted boundary.
    pub phase: InstallPhase,
    /// Whether sending the request was attempted; a failed send is not absence.
    pub request_attempted: bool,
    /// Whether the original Ready acknowledgement arrived.
    pub accepted: bool,
    /// Original installed reply, retained even when its fields fail validation.
    pub acknowledged: Option<StoreManifest>,
    /// Stream work before failure.
    pub work: InstallWork,
    /// Original deciding failure.
    pub error: InstallError,
    /// A failed local socket fence, separately retained from the original cause.
    pub fence_error: Option<ChannelError>,
}
impl fmt::Display for InstallFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Store install at {:?}: {:?}", self.phase, self.error)
    }
}
impl std::error::Error for InstallFailure {}
