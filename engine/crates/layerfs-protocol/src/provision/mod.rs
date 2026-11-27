//! Store provisioning: what a daemon is set up with (`daemon_setup`), the
//! install records and their codec (`install`), the Store manifest
//! (`manifest`) and the Store a daemon opens (`store`).
pub mod daemon_setup;
pub mod install;
pub mod manifest;
pub mod store;
