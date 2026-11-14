//! Opaque immutable bodies addressed by their pack digest.

use sha2::{Digest, Sha256};

/// Prototype (FUSE profile probe `verify-once`, not a product contract):
/// read-path authentication switches from `LAYERFS_PROBE_READ`, a comma list.
/// `nopacksha` skips the SHA-256 comparison of a pack read against its
/// descriptor key (length, framing and every other check stay; publication
/// still computes the key). `noresolve` skips the BLAKE3 comparison of each
/// object the Store resolver reconstructs against its locator identity.
/// Unset, none applies.
#[derive(Clone, Copy, Default)]
pub(crate) struct ReadProbe {
    pub(crate) nopacksha: bool,
    pub(crate) noresolve: bool,
}
/// Prototype: the [`ReadProbe`] switches, read once per process.
pub(crate) fn read_probe() -> ReadProbe {
    static PROBE: std::sync::OnceLock<ReadProbe> = std::sync::OnceLock::new();
    *PROBE.get_or_init(|| {
        let value = std::env::var("LAYERFS_PROBE_READ").unwrap_or_default();
        let has = |name: &str| value.split(',').any(|part| part.trim() == name);
        ReadProbe {
            nopacksha: has("nopacksha"),
            noresolve: has("noresolve"),
        }
    })
}

/// SHA-256 identity of a sealed pack, computed and checked by C2.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ObjectKey([u8; 32]);

impl ObjectKey {
    /// Hashes the complete sealed bytes.
    pub fn for_bytes(bytes: &[u8]) -> Self {
        layerfs_telemetry::timer::stages::time(
            layerfs_telemetry::timer::stages::Stage::PackHash,
            || Self(Sha256::digest(bytes).into()),
        )
    }
    /// Imports a persisted digest.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    /// Digest bytes for an engine's opaque key.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
