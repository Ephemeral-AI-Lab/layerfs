//! Shared provisioning facts for a sealed file and its one installed destination.
use std::fmt;

/// Provider selected by provisioning; consumers never guess a backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderKind {
    /// A SQLite Store on one kernel's local filesystem or shared volume.
    Sqlite,
}
/// Global Store completion profile, independent of local overlay configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoreProfile {
    /// WAL with FULL synchronization.
    Durable,
    /// WAL with synchronization disabled; process-crash survival only.
    Disposable,
}
/// Bounded control metadata. File bytes are streamed separately without a size cap.
#[derive(Clone, Eq, PartialEq)]
pub struct StoreManifest {
    /// Explicit provider kind.
    pub provider: ProviderKind,
    /// Absolute Store path in the daemon's filesystem, never the host source path.
    pub locator: String,
    /// Provisioned completion profile.
    pub profile: StoreProfile,
    /// Catalog binding key; validated again by the actual provider at open.
    pub binding: Vec<u8>,
    /// Catalog cursor authority, sent only over the authenticated channel.
    pub cursor_key: [u8; 32],
    /// Versioned LayerStack identity.
    pub stack: [u8; 17],
    /// Versioned initial Branch identity.
    pub branch: [u8; 17],
    /// Complete canonical root selected by Init.
    pub root: [u8; 32],
    /// Exact sealed main-file length.
    pub bytes: u64,
    /// SQLite version that created and sealed the file.
    pub host_sqlite: String,
    /// Actual daemon SQLite version, populated only by successful installation.
    pub daemon_sqlite: Option<String>,
}
impl StoreManifest {
    /// Checks protocol metadata bounds before a file or channel attempt.
    /// These are control/OS path bounds, not file or namespace size limits.
    pub fn check(&self) -> Result<(), &'static str> {
        check_destination(&self.locator, &self.binding, self.cursor_key)?;
        if self.bytes == 0 || !version(&self.host_sqlite) {
            return Err("sealed Store identity");
        }
        if self.daemon_sqlite.as_deref().is_some_and(|v| !version(v)) {
            return Err("daemon SQLite version");
        }
        Ok(())
    }
}
/// Validates the destination and authority before Init creates any file.
pub fn check_destination(
    locator: &str,
    binding: &[u8],
    cursor_key: [u8; 32],
) -> Result<(), &'static str> {
    if !locator.starts_with('/')
        || locator.len() > 4096
        || locator.as_bytes().contains(&0)
        || locator.ends_with('/')
    {
        return Err("Store locator");
    }
    if binding.is_empty() || binding.len() > 128 || cursor_key == [0; 32] {
        return Err("catalog authority");
    }
    Ok(())
}
fn version(value: &str) -> bool {
    !value.is_empty() && value.len() <= 64 && value.bytes().all(|b| b.is_ascii_graphic())
}
impl fmt::Debug for StoreManifest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoreManifest")
            .field("provider", &self.provider)
            .field("locator", &self.locator)
            .field("profile", &self.profile)
            .field("binding", &self.binding)
            .field("cursor_key", &"[redacted]")
            .field("stack", &self.stack)
            .field("branch", &self.branch)
            .field("root", &self.root)
            .field("bytes", &self.bytes)
            .field("host_sqlite", &self.host_sqlite)
            .field("daemon_sqlite", &self.daemon_sqlite)
            .finish()
    }
}
