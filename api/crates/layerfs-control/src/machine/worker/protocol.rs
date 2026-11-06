//! The worker protocol's one set of waits and cadences.
//!
//! Both sides read them here: the caller's waits ([`CONNECT`], [`HELLO_WAIT`],
//! [`CLAIM_WAIT`], [`PROGRESS_WAIT`], [`PING_WAIT`], [`SEND_WAIT`]) and the
//! worker's own ([`HEARTBEAT`], [`REQUEST_WAIT`]), plus the cadence at which a
//! command's live child is reported ([`RUNNING`]). None of them is an
//! operation's duration: a command that keeps reporting progress keeps its
//! whole duration.
use std::time::Duration;

/// How long the caller waits for the TCP connection.
pub const CONNECT: Duration = Duration::from_secs(10);

/// How long the caller waits for the worker to answer its secret: the whole
/// hello frame, not each socket read (a peer that trickles bytes cannot
/// stretch it).
pub const HELLO_WAIT: Duration = Duration::from_secs(10);

/// How long the caller waits for a worker that read its request to claim it
/// (`{ claimed: true }`). A worker that stops before claiming is dead: the
/// call is `Unknown` within this wait.
pub const CLAIM_WAIT: Duration = Duration::from_secs(10);

/// How long the caller waits for the next sign that a claimed operation is
/// advancing: a `progress` or an `event` frame refreshes it. A liveness
/// `beat` does not, so a worker that keeps beating while its operation never
/// progresses is `Unknown` within this wait. An operation that keeps
/// reporting progress (a phase, output, or a live command's child) keeps its
/// whole duration.
pub const PROGRESS_WAIT: Duration = Duration::from_secs(30);

/// How often a worker beats while it runs an operation.
pub const HEARTBEAT: Duration = Duration::from_secs(2);

/// How often a live command's child is reported as operation progress. It is
/// well inside [`PROGRESS_WAIT`], so a quiet command (`sleep`, a build with no
/// output) keeps its caller's wait fresh while its process is alive.
pub const RUNNING: Duration = Duration::from_secs(5);

/// How long a ping waits for its answer, each frame of it.
pub const PING_WAIT: Duration = Duration::from_secs(10);

/// How long a socket write may make no progress. It bounds the connection,
/// never the operation: a worker that stopped reading is `Unavailable` or
/// `Unknown` within this wait, and a slow but live one keeps going.
pub const SEND_WAIT: Duration = Duration::from_secs(60);

/// The longest the worker waits for the next byte of a request.
pub const REQUEST_WAIT: Duration = Duration::from_secs(30);
