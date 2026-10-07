//! Production direct-Store daemon startup/control, with no command supervisor.
mod cli;
mod config;
mod connection;
mod failure;
mod owner;
mod serve;
pub use cli::main;
pub use failure::{ApplicationError, ConnectionFailure};
pub use owner::Application;
