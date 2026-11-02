//! The worker: the one process on a machine whose kernel the caller cannot
//! open a sqlite Store on (`docker` on macOS and Windows, every VM and ssh
//! host). It runs the library's host operations for the calls the caller
//! sends it, behind a secret, on a loopback port (H §7.2, §7.7).
//!
//! Guests never read the home: every request carries what its operation
//! needs (the Store's identity and path on the machine, the project's
//! record, the Sandbox settings). One request per connection, sent once,
//! after a small first frame with the secret that the worker answers before
//! it reads the request: a refusal or a lost connection before the request
//! is sent is a plain refusal; a lost answer after it is `Unknown`, never
//! resent.
mod client;
mod frame;
mod idle;
mod server;

pub use client::{call, ping, Endpoint, HELLO_WAIT, PING_WAIT};
pub use frame::{frame_bytes, read_frame, read_frame_within, write_frame, MAX_FRAME, MAX_HELLO};
pub use idle::{idle, serve_until_idle, IDLE};
pub use server::{last_active, serve, Served, REQUEST_WAIT};
