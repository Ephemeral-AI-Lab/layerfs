//! Validated full Engine IDs; never a prefix, name, PID or guessed path.
use super::RuntimeError;
use std::fmt;
#[derive(Clone, Copy, Eq, PartialEq, Hash)]
struct EngineId([u8; 64]);
impl EngineId {
    fn parse(value: &str) -> Result<Self, RuntimeError> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            || value.bytes().all(|c| c == b'0')
        {
            return Err(RuntimeError::Protocol("full Engine identity"));
        }
        let mut bytes = [0; 64];
        bytes.copy_from_slice(value.as_bytes());
        Ok(Self(bytes))
    }
    fn text(&self) -> &str {
        std::str::from_utf8(&self.0).expect("validated ASCII identity")
    }
}
/// Exact acknowledged container identity, never a name/prefix selector.
#[derive(Clone, Copy, Eq, PartialEq, Hash)]
pub struct ContainerId(EngineId);
impl ContainerId {
    /// Validates a full acknowledged Engine identity.
    pub fn parse(value: &str) -> Result<Self, RuntimeError> {
        EngineId::parse(value).map(Self)
    }
    /// Exact Engine path component.
    pub fn as_str(&self) -> &str {
        self.0.text()
    }
    /// A fresh random identity in the Engine's form (64 lowercase hex, not
    /// all zero), so records, names and the cgroup directory treat a local
    /// Sandbox like a container.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(crate) fn random() -> Result<Self, RuntimeError> {
        use std::{fs::File, io::Read};
        let mut bytes = [0u8; 32];
        File::open("/dev/urandom")?.read_exact(&mut bytes)?;
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let text: String = (bytes.iter())
            .flat_map(|b| [HEX[usize::from(b >> 4)], HEX[usize::from(b & 15)]])
            .map(char::from)
            .collect();
        Self::parse(&text)
    }
}
impl fmt::Debug for ContainerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ContainerId").field(&self.as_str()).finish()
    }
}
impl fmt::Display for ContainerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
