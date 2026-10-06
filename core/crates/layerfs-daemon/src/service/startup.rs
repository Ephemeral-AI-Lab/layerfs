//! One daemon initialization attempt, with original creation custody retained.
use crate::{Owner, OwnerError};
use std::sync::Arc;

/// Result of an observed daemon start. The receipt survives a definite startup
/// failure and includes allocation observation errors without replacing them.
pub struct OwnerStart {
    pub result: Result<Owner, OwnerError>,
    pub startup: Arc<layerfs_overlay::CreationWork>,
    /// Complete start-to-readiness/failure wall, overlapping creation work.
    pub elapsed_ns: u64,
}
