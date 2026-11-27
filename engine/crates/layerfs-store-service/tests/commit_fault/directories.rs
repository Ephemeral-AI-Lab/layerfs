//! #810 F3: a dropped connection, and the backend killed while its replies
//! are held, at every server message of a warm Workspace Commit whose change
//! touches two base directories nothing read before (`/one`, `/two`), so
//! their delta-encoded root pages are one grouped read
//! (`support/fault_sweep.rs` states the invariants). Needs
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
fn a_connection_dropped_at_any_message_of_a_spread_commit() {
    fault_sweep::sweep("drop_spread", Fault::Drop, Flight::Spread, Positions::All);
}

#[test]
fn the_backend_killed_at_any_message_of_a_spread_commit() {
    fault_sweep::sweep(
        "kill_spread",
        Fault::HoldKill,
        Flight::Spread,
        Positions::All,
    );
}
