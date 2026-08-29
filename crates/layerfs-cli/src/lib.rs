#![forbid(unsafe_code)]

mod command;
mod completion;
mod context;
mod control;
mod event;
mod execute;
mod host;
mod output;
mod parse;
mod plan;
mod query;

pub use command::{
    BranchCommand, Command, DbCommand, LayerCommand, LayerInit, MonitorCommand, Projection,
    StackCommand, StoreRole, WorkspaceCommand,
};
pub use completion::Completion;
pub use context::default_context_location;
#[doc(hidden)]
pub use control::{invoke, runtime_location, serve};
pub use control::{CliSession, OperationHandle};
pub use event::{
    CliError, CliEvent, CliResult, CommandResult, CommandSummary, OperationPhase, ProgressValue,
};
pub use layerfs_sdk::{Fact, FactKind};
pub use plan::{CommandEffect, CommandPlan};
pub use query::{
    CommitDiffEntry, DatabaseView, DedupView, MonitorView, PlacementView, StoreFact, StoreQuery,
    StoreScope, StoreSnapshot, TopologyEntry, ViewQuery, ViewScope, ViewSnapshot,
};

pub const JSON_SCHEMA_VERSION: u32 = 1;
