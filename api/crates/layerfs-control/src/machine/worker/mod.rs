//! The worker: the one process on a machine whose kernel the caller cannot
//! open a sqlite Store on (`docker` on macOS and Windows, every VM and ssh
//! host). It runs the library's host operations for the calls the caller
//! sends it, behind a secret, on a loopback port (H §7.2, §7.7). It serves
//! until `machine.stop` ends it: there is no idle stop (#535 D11).
//!
//! Guests never read the home: every request carries what its operation
//! needs (the Store's identity and path on the machine, the project's
//! record, the Sandbox settings). One request per connection, sent once,
//! after a small first frame with the secret that the worker answers before
//! it reads the request, with its protocol and bundle (`hello`): a worker of
//! another build is sent nothing. A refusal or a lost connection before the
//! request is sent is a plain refusal; a lost answer after it is `Unknown`,
//! never resent. Once the request is read, the worker claims it and then
//! beats while it runs, so every wait after the request is bounded (connect,
//! hello, claim, heartbeat) while the operation itself is not: a dead worker
//! is `Unknown` in bounded time, and a long command keeps running.
mod client;
mod frame;
mod hello;
mod server;

pub use client::{
    call, hello, ping, request, send, Endpoint, Unreached, CLAIM_WAIT, HEARTBEAT, HEARTBEAT_WAIT,
    HELLO_WAIT, PING_WAIT, SEND_WAIT,
};
pub use frame::{
    frame_bytes, read_frame, read_frame_within, read_frame_within_wait, write_frame, MAX_FRAME,
    MAX_HELLO,
};
#[cfg(feature = "machine")]
pub(crate) use hello::sealed_hash;
pub use hello::{is_mismatch, replace_with, Hello, PROTOCOL};
pub use server::{serve, Served, REQUEST_WAIT};
