//! #768: a dropped connection at every server message of a Workspace's
//! first Commit on a newly opened Store service on PostgreSQL: the cold
//! flight, with the id block reservation, the statement prepares and the
//! lane connection's connect. The proxy discards message `N` and everything
//! after it and shuts both sockets of that connection
//! (`support/fault_sweep.rs` states the invariants). Needs
//! `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
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
fn a_connection_dropped_at_any_message_of_a_first_commit() {
    fault_sweep::sweep("drop_first", Fault::Drop, false, Positions::All);
}
