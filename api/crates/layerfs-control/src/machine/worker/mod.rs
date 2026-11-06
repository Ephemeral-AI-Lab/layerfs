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
//! never resent. Once the request is read, the worker claims it, reports
//! operation progress while it runs and beats as a liveness signal, so
//! every wait after the request is bounded (connect, hello, claim, progress)
//! while the operation itself is not: a dead worker is `Unknown` in bounded
//! time, a wedged one is `Unknown` once it stops progressing, and a long
//! command that reports output keeps its whole duration.
//!
//! Progress and liveness are separate frames. `{ progress: ... }` says the
//! operation advanced and refreshes the caller's wait; `{ beat: true }` says
//! the worker process is alive and never refreshes it, so a worker that
//! keeps beating while its operation never returns is `Unknown` within
//! `PROGRESS_WAIT` instead of holding the call open. A running command adds
//! one progress source of its own: the worker reports its child while that
//! process is alive (`phase: "running"`, every [`RUNNING`]), so a quiet
//! `sleep` or build keeps its caller's wait while its process lives, and an
//! operation whose child is gone (or never started) stops reporting and is
//! bounded by `PROGRESS_WAIT` again. Liveness is not progress: a process
//! that is alive but wedged is not distinguishable from a slow one. A frame
//! is bounded as a whole: its length and every read within the wait, and its
//! body within the
//! wait or its length at a minimum rate, so a peer that trickles a frame
//! cannot stretch a wait. A wait that expires writes one `layerfs worker:
//! liveness timeout:` line to stderr.
mod client;
mod frame;
mod hello;
mod protocol;
mod server;

pub use client::{
    call, call_with, hello, ping, request, send, send_with, Endpoint, Unreached, Windows,
};
pub use frame::{
    frame_bytes, read_frame, read_frame_within, read_frame_within_wait, write_frame, MAX_FRAME,
    MAX_HELLO,
};
#[cfg(feature = "machine")]
pub(crate) use hello::sealed_hash;
pub use hello::{is_mismatch, replace_with, Hello, PROTOCOL};
pub use protocol::{
    CLAIM_WAIT, CONNECT, HEARTBEAT, HELLO_WAIT, PING_WAIT, PROGRESS_WAIT, REQUEST_WAIT, RUNNING,
    SEND_WAIT,
};
pub use server::{serve, Served};
