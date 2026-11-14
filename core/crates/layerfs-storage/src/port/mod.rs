//! Bounded physical persistence contract.
pub mod acquisition;
mod pack;
mod persistence;
mod read_pack;
mod read_scoped;
mod read_selection;
pub(crate) use pack::read_probe;
pub use pack::ObjectKey;
pub use persistence::{
    PackPersistence, PersistenceError, Publication, Published, PublishedPack, Reserve, Reserved,
};
pub use read_pack::PersistedPack;

pub use read_selection::{
    PackRange, PackRangeBodies, PackReadChoice, PackReadPlan, PersistedPackRanges,
    PersistedPackRead, PACK_READ_PREFIX_BYTES, PACK_SCAN_BYTES,
};

pub use read_scoped::{AcquiredPackRead, AcquiredPackUnits};
