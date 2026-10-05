//! Responsibility-scoped implementation modules and reexports.
pub(crate) mod install;
pub(crate) mod serials;
pub(crate) mod state;
pub(crate) mod view;
pub use state::{Workspace, WorkspaceError, WorkspaceResult};
