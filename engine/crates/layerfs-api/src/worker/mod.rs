//! The worker: the one process on a machine whose kernel the caller cannot
//! open a sqlite Store on (`docker` on macOS and Windows, every VM and ssh
//! host). It runs the library's host operations for the calls the caller
//! sends it, behind a secret, on a loopback port (H §7.2, §7.7).
//!
//! Guests never read the home: every request carries what its operation
//! needs (the Store's identity and path on the machine, the project's
//! record, the Sandbox settings). One request per connection, sent once:
//! a lost answer is `Unknown`, never resent.
mod client;
mod frame;
mod server;

pub use client::{call, ping, Endpoint};
pub use frame::{read_frame, write_frame, MAX_FRAME};
pub use server::{last_active, serve, Served};
