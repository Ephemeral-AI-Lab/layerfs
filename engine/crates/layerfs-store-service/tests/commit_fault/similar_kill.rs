//! #810: the backend killed (`pg_terminate_backend`) at every server message
//! of a warm Workspace Commit that adds families of similar files, on
//! PostgreSQL, while its replies are held. The held replies and the server's
//! last words are delivered once the backend is gone. Its Save holds each
//! family's later members until one publication of the families' first
//! members, which precedes the fused message (`support/fault_sweep.rs`
//! states the invariants). Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN
//! without it.
#[path = "../support/verify/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "../support/fault/proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;
#[path = "../support/fault/sweep.rs"]
#[allow(dead_code)]
mod fault_sweep;

use fault_proxy::Fault;
use fault_sweep::{Flight, Positions};

#[test]
fn a_backend_killed_at_any_message_of_a_similar_files_commit() {
    fault_sweep::sweep(
        "kill_similar",
        Fault::HoldKill,
        Flight::Similar,
        Positions::All,
    );
}
