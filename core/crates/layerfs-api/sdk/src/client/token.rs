//! Opaque remotely returned Save identity; authority remains with the host.
/// Exact runtime incarnation, slot and serial returned by SaveBegin.
/// Parsing this value does not establish a local or remote capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SaveToken(pub(crate) [u8; 48]);
impl SaveToken {
    /// Stores original wire identity for reconnect/receipt inspection. Every
    /// host use must revalidate peer, binding, incarnation, slot and serial.
    pub const fn from_bytes(bytes: [u8; 48]) -> Self {
        Self(bytes)
    }
    /// Exact identity without reconstructing it from a display string.
    pub const fn bytes(self) -> [u8; 48] {
        self.0
    }
}
