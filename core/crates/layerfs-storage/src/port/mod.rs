//! Bounded physical persistence contract.
mod pack;
mod persistence;
pub use pack::ObjectKey;
pub use persistence::{
    PackPersistence, PersistedPack, PersistenceError, Publication, Published, PublishedPack,
    Reserve, Reserved, ValueGroupQuery, ValueGroups,
};
