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
//! never resent.
//!
//! This is the protocol and its client; the worker's server, which runs
//! every entity's operations, is `layerfs-api` `worker::serve`
//! (layerfs-project#104; #828).
pub mod accept;
pub(crate) mod guest;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) mod host;
pub(crate) mod runs_here;

pub use guest::client::{
    call, hello, hello_within, ping, request, send, send_within, Budget, Endpoint, EventSink,
    Unreached, HELLO_WAIT, PING_WAIT,
};
pub use guest::diagnostic::{diagnostic_connect, DiagnosticOperation, DiagnosticWorker};
pub use guest::frame::{
    frame_bytes, read_frame, read_frame_within, write_frame, ATTACHED, MAX_FRAME, MAX_HELLO,
};
#[cfg(feature = "machine")]
pub(crate) use guest::hello::sealed_hash;
pub use guest::hello::{is_mismatch, replace_with, Hello, PROTOCOL};

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use host::client::HOST_HELD_WRITES;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use host::diagnostic::{HostObservation, HostProbe};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use host::endpoint::{host_check_peer, HostGuard, HostPaths};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use host::frame::{
    read_host_frame, value_within_budget, within_budget, write_host_frame, FRAME_LIMITS,
    HOST_MAX_REPLY, HOST_MAX_REQUEST,
};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use host::hello::{set_host_build, HostHello, HOST_PROTOCOL};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use host::start::{
    record_worker_start_failure, start_failure, LAUNCH_BACKGROUND, LAUNCH_MACHINE_START,
    START_FAILURE_LIMIT,
};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use host::stream::{
    StreamFrame, StreamReader, StreamSender, STREAM_KEEPALIVE, STREAM_PIECE, STREAM_PROGRESS,
    STREAM_WAIT,
};
