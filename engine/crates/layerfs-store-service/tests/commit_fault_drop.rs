//! #768: a dropped connection at every server message of one Workspace
//! Commit on PostgreSQL (`support/fault_sweep.rs` states the invariants).
//! The proxy discards message `N` and everything after it and shuts both
//! sockets of that connection. Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN
//! without it.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "support/fault_proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;
#[path = "support/fault_sweep.rs"]
#[allow(dead_code)]
mod fault_sweep;

use fault_proxy::Fault;
use fault_sweep::Positions;

#[test]
fn a_connection_dropped_at_any_message_of_a_warm_commit() {
    fault_sweep::sweep("drop_warm", Fault::Drop, true, Positions::All);
}
