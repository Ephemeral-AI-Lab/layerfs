//! Native process assembly: configuration, acceptor, Store and Server.
mod acceptor;
mod assembly;
mod config;
mod run;
mod store;
pub use assembly::{Server, ServerConfig, PRIMARY_STORE};
pub use config::{history, telemetry};
pub use run::run;
