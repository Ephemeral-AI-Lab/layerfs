//! Bounded physical persistence contract.
pub mod acquisition;
pub(crate) mod object_key;
pub(crate) mod pack_read;
mod persistence;
pub use object_key::ObjectKey;
pub use pack_read::persisted::PersistedPack;
pub use persistence::{
    GroupRead, GroupReadAnswer, PackPersistence, PersistenceError, Publication, Published,
    PublishedPack, Reserve, Reserved,
};

pub use pack_read::selection::{
    PackRange, PackRangeBodies, PackReadChoice, PackReadPlan, PersistedPackRanges,
    PersistedPackRead, PACK_READ_PREFIX_BYTES, PACK_SCAN_BYTES,
};

pub use pack_read::scoped::{AcquiredPackRead, AcquiredPackUnits};
pub(crate) use pack_read::selection::check_ranges;
