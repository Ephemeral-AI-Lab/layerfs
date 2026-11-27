//! The Store calls that change where a Store exists or runs: create,
//! connect, disconnect, attach, detach, move and remove.
pub(super) mod attach;
pub(super) mod connect;
pub(super) mod create;
pub(super) mod detach;
pub(super) mod disconnect;
pub(super) mod r#move;
pub(super) mod remove;
