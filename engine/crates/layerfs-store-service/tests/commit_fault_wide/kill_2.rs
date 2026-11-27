//! #810: the backend killed (its replies held, then terminated) at every server message of one wide Workspace
//! Commit on PostgreSQL (`Flight::Wide`: its reads before the lane go in
//! batched windows; `support/fault_sweep.rs` states the invariants). The
//! flight is swept in eight shares, two per binary
//! (`commit_fault_wide_kill_0` to `_3`), so each binary, with its final
//! full Store audit, stays within one test command's time. This binary:
//! shares 4 and 5. Needs `LAYERFS_TEST_POSTGRES_URL`;
//! NOT_RUN without it.
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
fn the_backend_killed_at_a_wide_commit_message_share_4() {
    fault_sweep::sweep(
        "kill_wide_4",
        Fault::HoldKill,
        Flight::Wide,
        Positions::Share(4, 8),
    );
}

#[test]
fn the_backend_killed_at_a_wide_commit_message_share_5() {
    fault_sweep::sweep(
        "kill_wide_5",
        Fault::HoldKill,
        Flight::Wide,
        Positions::Share(5, 8),
    );
}
