//! One daemon initialization attempt, with original creation custody retained.
use crate::{Owner, OwnerError};
use std::sync::Arc;

/// Result of an observed daemon start. The receipt survives a definite startup
/// failure and includes allocation observation errors without replacing them.
pub struct OwnerStart {
    pub result: Result<Owner, OwnerError>,
    /// Original creation receipt when creation_reported is true. Otherwise this
    /// is an empty placeholder; a pre-receipt worker failure has unavailable
    /// creation costs, not observed zero work.
    pub startup: Arc<layerfs_overlay::CreationWork>,
    /// The owner received the original creation result/work from its worker.
    /// Admission/spawn or pre-receipt worker failures leave this false.
    pub creation_reported: bool,
    /// Complete start-to-readiness/failure wall, overlapping creation work.
    pub elapsed_ns: u64,
}
