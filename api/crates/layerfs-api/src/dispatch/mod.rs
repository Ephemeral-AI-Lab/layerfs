//! The call machinery under the call table (#533 step 2, H §7.10): the
//! table's shape, argument checks, folder scope, result keys and the one
//! dispatcher with its call log line (`render`, #659); moved back from
//! `layerfs-control` by layerfs-project#107. The call surface (2.0) adds
//! what an entry declares of its answer (`form`), the one place that makes
//! the short and the long form (`shape`) and the refusal every front end
//! answers (`refusal`).
//! The 83 entries in ten families live beside it, one folder per family and
//! one file per call; each function here that needs them takes that list.
pub mod answer;
mod context;
pub mod guard;
pub mod guide;
pub(crate) mod run;
pub mod table;

pub use answer::form::{Asked, Form, Paging, Part, Place, Profile, Section, THING};
pub use context::Context;
pub use guard::arguments::size;
pub use guard::platform_policy::Policy;
pub use guard::scope::Scope;
pub use layerfs_home::json;
pub use run::call::{asks_detail, call, call_scoped, close, home, Client, DETAIL, WITH};
pub(crate) use run::call::{call_cli, cli_defers};
pub use run::call_json::call_json;
pub use table::{find, table_json, Arg, Call, CallRef, Calls, Families, Family, Kind, Redact, Run};
