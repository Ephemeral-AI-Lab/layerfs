//! The call machinery under the call table (#533 step 2, H §7.10): the
//! table's shape, argument checks, folder scope, result keys and the one
//! dispatcher with its call log line (`render`, #659); moved back from
//! `layerfs-control` by layerfs-project#107.
//! The 70 entries in ten families live beside it, one folder per family and
//! one file per call; each function here that needs them takes that list.
mod check;
mod context;
mod dispatch;
pub mod keys;
mod platform_policy;
mod render;
pub mod scope;
pub mod table;
mod text;

pub use check::size;
pub use context::Context;
pub use dispatch::{call, call_scoped, home, Client};
pub use layerfs_home::json;
pub use scope::Scope;
pub use table::{find, table_json, Arg, Call, CallRef, Calls, Families, Family, Kind, Run};
pub use text::call_json;
