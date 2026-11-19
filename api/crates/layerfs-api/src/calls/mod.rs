//! The call machinery under the call table (#533 step 2, H §7.10): the
//! table's shape, argument checks, folder scope, result keys and the one
//! dispatcher with its call log line (`render`, #659); moved back from
//! `layerfs-control` by layerfs-project#107. The call surface (2.0) adds
//! what an entry declares of its answer (`form`), the one place that makes
//! the short and the long form (`shape`) and the refusal every front end
//! answers (`refusal`).
//! The 83 entries in ten families live beside it, one folder per family and
//! one file per call; each function here that needs them takes that list.
mod check;
mod context;
mod dispatch;
pub(crate) mod doctor;
pub mod form;
pub mod guide;
pub(crate) mod host_only;
pub mod keys;
mod platform_policy;
pub mod refusal;
mod render;
pub mod scope;
pub mod shape;
pub mod table;
mod text;

pub use check::size;
pub use context::Context;
pub use dispatch::{asks_detail, call, call_scoped, close, home, Client, DETAIL, WITH};
pub use form::{Asked, Form, Paging, Part, Place, Profile, Section, THING};
pub use layerfs_home::json;
pub use platform_policy::Policy;
pub use scope::Scope;
pub use table::{find, table_json, Arg, Call, CallRef, Calls, Families, Family, Kind, Redact, Run};
pub use text::call_json;
