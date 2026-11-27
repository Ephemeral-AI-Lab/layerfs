//! Shared provisioning facts for a sealed file and its one installed destination.
use std::fmt;

/// Provider selected by provisioning; consumers never guess a backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderKind {
    /// A SQLite Store on one kernel's local filesystem or shared volume.
    Sqlite,
    /// A PostgreSQL Store (#506). It is opened in its server schema, never
    /// installed from a sealed file, so an install manifest refuses it.
    Postgres,
}
/// Global Store completion profile, independent of local overlay configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoreProfile {
    /// WAL with FULL synchronization.
    Durable,
    /// The WAL Store opened with NORMAL synchronization (owner, 2026-11-17).
    /// Its wire byte (2) is the one the former `Disposable` label used.
    Normal,
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
        if self.provider == ProviderKind::Postgres {
            return Err("a PostgreSQL Store has no sealed file to install");
        }
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

// One authenticated control record for sealed Store provisioning.
use crate::{
    provision::install::ControlError,
    wire::{Reader, Writer},
};
pub(crate) const MAGIC: &[u8] = b"LFSI\x01";
impl StoreManifest {
    /// Encodes checked metadata; sealed file bytes are separate duplex records.
    pub fn encode(&self) -> Result<Vec<u8>, ControlError> {
        self.check().map_err(ControlError)?;
        let mut out = Writer::new(MAGIC);
        out.byte(match self.provider {
            ProviderKind::Sqlite => 1,
            ProviderKind::Postgres => 2,
        })?;
        out.byte(match self.profile {
            StoreProfile::Durable => 1,
            StoreProfile::Normal => 2,
        })?;
        out.blob(self.locator.as_bytes())?;
        out.blob(&self.binding)?;
        out.put(&self.cursor_key)?;
        out.put(&self.stack)?;
        out.put(&self.branch)?;
        out.put(&self.root)?;
        out.put(&self.bytes.to_be_bytes())?;
        out.blob(self.host_sqlite.as_bytes())?;
        out.blob(self.daemon_sqlite.as_deref().unwrap_or("").as_bytes())?;
        Ok(out.0)
    }
    /// Decodes exactly one bounded record, refusing unknown versions and trailing bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, ControlError> {
        let mut input = Reader::new(bytes, MAGIC)?;
        let provider = match input.byte()? {
            1 => ProviderKind::Sqlite,
            2 => ProviderKind::Postgres,
            _ => return Err(ControlError("provider kind")),
        };
        let profile = match input.byte()? {
            1 => StoreProfile::Durable,
            2 => StoreProfile::Normal,
            _ => return Err(ControlError("Store profile")),
        };
        let locator = input.text(4096)?;
        let binding = input.blob(128)?.to_vec();
        let cursor_key = input.array()?;
        let stack = input.array()?;
        let branch = input.array()?;
        let root = input.array()?;
        let bytes = u64::from_be_bytes(input.array()?);
        let host_sqlite = input.text(64)?;
        let daemon = input.text(64)?;
        input.finish()?;
        let value = Self {
            provider,
            profile,
            locator,
            binding,
            cursor_key,
            stack,
            branch,
            root,
            bytes,
            host_sqlite,
            daemon_sqlite: (!daemon.is_empty()).then_some(daemon),
        };
        value.check().map_err(ControlError)?;
        Ok(value)
    }
}
