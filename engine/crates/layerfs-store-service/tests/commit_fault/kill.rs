//! #768: the backend killed (`pg_terminate_backend`) at every server message
//! of one Workspace Commit on PostgreSQL while its replies are held
//! (`support/fault_sweep.rs` states the invariants). The held replies and
//! the server's last words are delivered once the backend is gone. Needs
//! `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
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
fn the_backend_killed_at_any_message_of_a_warm_commit() {
    fault_sweep::sweep("kill_warm", Fault::HoldKill, Flight::Warm, Positions::All);
}
