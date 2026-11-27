//! #768: the backend killed (`pg_terminate_backend`) at every server message
//! of a Workspace's first Commit on a newly opened Store service on
//! PostgreSQL, while its replies are held: the cold flight, with the id
//! block reservation, the statement prepares and the lane connection's
//! connect (a hold inside a connection's startup kills that backend as soon
//! as its key data arrives). The held replies and the server's last words
//! are delivered once the backend is gone (`support/fault_sweep.rs` states
//! the invariants). Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
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
fn the_backend_killed_at_any_message_of_a_first_commit() {
    fault_sweep::sweep("kill_first", Fault::HoldKill, Flight::First, Positions::All);
}
