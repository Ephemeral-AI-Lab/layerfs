//! The call table and its one dispatcher (#533 step 2, H §7.10): 57 calls
//! in nine families, the same from the `layerfs` binary, Node and Python.
//! See [`table::CALLS`] and [`call`].
mod check;
mod dispatch;
pub mod json;
pub mod keys;
mod route;
pub mod scope;
pub mod table;

pub use check::size;
pub use dispatch::{call, call_scoped, home, Context};
pub use scope::Scope;
pub use table::table_json;
pub use table::{find, Arg, Call, Kind, CALLS, FAMILIES};
