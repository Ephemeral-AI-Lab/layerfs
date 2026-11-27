//! #810: the backend killed (its replies held, then terminated) at every server message of one wide Workspace
//! Commit on PostgreSQL (`Flight::Wide`: its reads before the lane go in
//! batched windows; `support/fault_sweep.rs` states the invariants). The
//! flight is swept in eight shares, two per binary
//! (`commit_fault_wide_kill_0` to `_3`), so each binary, with its final
//! full Store audit, stays within one test command's time. This binary:
//! shares 0 and 1. Needs `LAYERFS_TEST_POSTGRES_URL`;
//! NOT_RUN without it.
use crate::fault_proxy;
use crate::fault_sweep;

use fault_proxy::Fault;
use fault_sweep::{Flight, Positions};

#[test]
fn the_backend_killed_at_a_wide_commit_message_share_0() {
    fault_sweep::sweep(
        "kill_wide_0",
        Fault::HoldKill,
        Flight::Wide,
        Positions::Share(0, 8),
    );
}

#[test]
fn the_backend_killed_at_a_wide_commit_message_share_1() {
    fault_sweep::sweep(
        "kill_wide_1",
        Fault::HoldKill,
        Flight::Wide,
        Positions::Share(1, 8),
    );
}
