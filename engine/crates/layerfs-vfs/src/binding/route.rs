//! Daemon routing metadata, separate from Overlay ownership and filesystem state.
use layerfs_overlay::{OverlayError, OverlayResult};

/// Identifies a daemon Workspace handle. Namespace selects the handle; engine
/// and incarnation prevent a stale route from selecting its replacement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Route {
    namespace: i64,
    engine: u64,
    incarnation: [u8; 32],
}
impl Route {
    pub fn new(namespace: i64, engine: u64, incarnation: [u8; 32]) -> OverlayResult<Self> {
        if namespace <= 0 || engine == 0 || incarnation == [0; 32] {
            return Err(OverlayError::Invalid("Workspace route identity"));
        }
        Ok(Self {
            namespace,
            engine,
            incarnation,
        })
    }
    pub const fn namespace(self) -> i64 {
        self.namespace
    }
    pub const fn engine(self) -> u64 {
        self.engine
    }
}
