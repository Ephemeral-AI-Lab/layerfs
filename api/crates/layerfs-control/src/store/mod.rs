//! Stores in the 2.0 home (#533): leases, the Stores this process holds
//! open, and the route each Store call takes, chosen from records.
pub(crate) mod files;
pub(crate) mod open;
pub(crate) mod route;
pub(crate) mod store_ref;
