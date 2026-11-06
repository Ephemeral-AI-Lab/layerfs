//! The call machinery under the call table (#533 step 2, H §7.10): the
//! table's shape, argument checks, folder scope, result keys and the one
//! dispatcher. The 61 entries in nine families live in `layerfs-api`, one
//! file per call; each function here that needs them takes that list.
mod check;
mod dispatch;
pub mod keys;
mod platform_policy;
pub mod scope;
pub mod table;
mod text;

pub use check::size;
pub use dispatch::{call, call_scoped, home, Context};
pub use layerfs_home::json;
pub use scope::Scope;
pub use table::{find, table_json, Arg, Call, CallRef, Calls, Families, Family, Kind, Run};
pub use text::call_json;
