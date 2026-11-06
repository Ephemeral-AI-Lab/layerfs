//! What the entities here still share (#536): a running call's context,
//! placement and Store leases. The home and its records are `layerfs-home`
//! (layerfs-project#103).
pub mod context;
pub mod lease;
pub(crate) mod place;
