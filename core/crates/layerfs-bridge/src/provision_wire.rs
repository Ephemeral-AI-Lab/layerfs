//! One authenticated control record for sealed Store provisioning.
use crate::{
    control::ControlError,
    provision::{ProviderKind, StoreManifest, StoreProfile},
    wire::{Reader, Writer},
};
pub(crate) const MAGIC: &[u8] = b"LFSI\x01";
impl StoreManifest {
    /// Encodes checked metadata; sealed file bytes are separate native records.
    pub fn encode(&self) -> Result<Vec<u8>, ControlError> {
        self.check().map_err(ControlError)?;
        let mut out = Writer::new(MAGIC);
        out.byte(match self.provider {
            ProviderKind::Sqlite => 1,
        })?;
        out.byte(match self.profile {
            StoreProfile::Durable => 1,
            StoreProfile::Disposable => 2,
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
            _ => return Err(ControlError("provider kind")),
        };
        let profile = match input.byte()? {
            1 => StoreProfile::Durable,
            2 => StoreProfile::Disposable,
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
