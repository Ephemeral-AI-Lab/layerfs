//! The LayerFS session layer, shared by the JavaScript binding and the Rust
//! rutis plugin.
//!
//! One process owns its project discovery, Sandboxes, control connections and
//! mounted Workspaces; Store calls open the installed Store in this process.
//! There is no Node dependency and no second implementation of placement,
//! Sandbox creation, exec spooling or Workspace records.
mod call;
mod direct;
mod discovery;
mod exec;
mod outcome;
mod output;
mod pool;
mod project;
mod registry;
mod sandbox;
mod trace;
mod workspace;

pub use call::{dispatch, exec_output, operator_call};
pub use direct::{acquire, branch_id_in, head_in, release};
pub use outcome::{failed, refused, Outcome};
pub use trace::{run, take_scope, take_waited};
