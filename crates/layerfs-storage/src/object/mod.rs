//! Canonical typed-object model, codec, and bounded traversal laws.
//!
//! Object identity and canonical bytes remain backend neutral. Filesystem,
//! pack, CAS, lifecycle, content, and read layers consume these semantic
//! definitions without acquiring ownership of the frozen object format.

mod decode;
mod encode;
mod model;
mod port_decode;
mod traversal;

pub use decode::*;
pub(crate) use encode::encode_physical_object_header_v1;
pub use encode::{OBJECT_HEADER_BYTES, VERSION_RECORD_PAYLOAD_BYTES};
pub use model::*;
pub use port_decode::*;
#[cfg(feature = "operation-polymorphism")]
pub(crate) use traversal::MAX_CANONICAL_TRAVERSAL_FRAMES_V1;
pub(crate) use traversal::{require_canonical_traversal_depth_v1, CanonicalTraversalBudgetV1};
