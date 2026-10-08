//! Cached pages of an open file. Base install continuity belongs to the
//! mounted Commit; nothing here changes a mounted view.
use layerfs_workspace::Refusal;

/// How the kernel treats cached pages of one opened file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpenCache {
    /// Pages cached by an earlier open stay valid: every change to a mounted
    /// view arrives as a request on this connection, which updates or drops
    /// the affected pages itself.
    pub keep: bool,
    /// Reads and writes go through the page cache, never around it.
    pub direct: bool,
}
/// The same answer for read-only, writable and created descriptors.
pub const fn open_cache() -> OpenCache {
    OpenCache {
        keep: true,
        direct: false,
    }
}
/// WRITE replies the full count or an error, never a short count: a short
/// reply would make the kernel shrink its size or resend a tail the daemon
/// already published. A mapped store clipped by the engine is still answered
/// in full, because the kernel had already clipped it to its own size.
pub fn written(requested: usize) -> Result<u32, Refusal> {
    u32::try_from(requested).map_err(|_| Refusal::Invalid)
}
