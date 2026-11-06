//! The Store calls whose checks read who uses a Store (layerfs-project#105):
//! each runs `layerfs-store`'s call with the checks from
//! [`crate::inventory`]. The Store calls proper (`branch`, `commit`, `file`)
//! and the rest are `layerfs-store`'s own.
use crate::inventory;
use layerfs_store::Related;

pub use layerfs_store::{calls, close_idle};

/// Who uses a Store or a project, read from the Sandbox records.
pub const RELATED: Related = Related {
    store_idle: inventory::store_idle,
    can_remove_project: inventory::can_remove_project,
};

/// The sqlite `store` calls.
pub mod family {
    use layerfs_bridge::control::SdkValue;
    use layerfs_home::context::Context;
    use layerfs_home::outcome::Outcome;

    /// Runs one sqlite `store` call.
    pub fn run(context: &mut Context<'_>, call: &str, args: &SdkValue) -> Outcome {
        layerfs_store::sqlite::family::run(context, call, args, super::RELATED)
    }
}

/// The PostgreSQL `store.connect` and `store.disconnect` calls.
pub mod connect {
    use layerfs_bridge::control::SdkValue;
    use layerfs_home::context::Context;
    use layerfs_home::outcome::Outcome;

    /// Runs `store.connect` or `store.disconnect`.
    pub fn run(context: &mut Context<'_>, call: &str, args: &SdkValue) -> Outcome {
        layerfs_store::postgresql::connect::run(context, call, args, super::RELATED)
    }
}
