//! Private backend-neutral LayerFS storage engine.
//!
//! L0 owns the checked canonical format/error/path surface and custody tests.
//! L1 incrementally adds the private BLAKE3 identity, FastCDC, immutable
//! admission, dense-pack, and structural COW runtime without exposing the
//! later public SDK, workspace, authority, or publication contracts.

#![forbid(unsafe_code)]

mod error;

#[allow(dead_code)]
pub(crate) mod cas;
pub mod cdc;
#[allow(dead_code)]
pub mod content;
#[allow(dead_code)]
pub mod cow;
pub mod format;
pub mod identity;
#[cfg(feature = "operation-polymorphism")]
#[allow(dead_code)]
pub(crate) mod lifecycle;
#[allow(dead_code)]
pub(crate) mod limits;
pub mod object;
/// Bounded resource observations for the default integration owners.
pub mod resources {
    pub use crate::limits::resources::{
        base_ledger_bytes_v1, observe_forbidden_work_v1, observe_memory_plan_v1,
        observe_memory_profile_v1, operation_slot_bytes_v1, ForbiddenWorkObservationV1,
        MemoryBudgetV1, MemoryPlanObservationV1, MemoryProfileObservationV1, MemoryResourceKindV1,
    };
}
#[allow(dead_code)]
pub(crate) mod pack;
pub mod profile;
#[cfg(feature = "operation-polymorphism")]
#[allow(dead_code)]
pub(crate) mod read;

pub use error::{CoreError, CoreResult, OutcomeCode};

/// The single doc-hidden semantic operation surface used by integration
/// owners.  It contains bounded requests and immutable observations only;
/// production module families remain private behind this facade.
#[cfg(feature = "operation-polymorphism")]
#[doc(hidden)]
pub mod qualification {
    pub mod cas {
        #[cfg(feature = "operation-polymorphism")]
        pub mod semantic {
            pub use crate::cas::semantic::*;
        }
    }
    pub mod content {
        pub mod semantic {
            pub use crate::content::semantic::*;
        }
        pub mod update {
            pub mod semantic {
                pub use crate::content::update::semantic::*;
            }
        }
    }
    pub mod cow {
        pub mod semantic {
            pub use crate::cow::semantic::*;
        }
    }
    pub mod lifecycle {
        #[cfg(feature = "operation-polymorphism")]
        pub mod semantic {
            pub use crate::lifecycle::semantic::*;
        }
    }
    pub mod object {
        pub use crate::object::semantic::*;
    }
    pub mod pack {
        #[cfg(feature = "operation-polymorphism")]
        pub mod semantic {
            pub use crate::pack::semantic::*;
        }
    }
    pub mod resources {
        pub use crate::limits::resources::*;
    }
}
