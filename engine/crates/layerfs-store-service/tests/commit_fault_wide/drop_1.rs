//! #810: a dropped connection at every server message of one wide Workspace
//! Commit on PostgreSQL (`Flight::Wide`: its reads before the lane go in
//! batched windows; `support/fault_sweep.rs` states the invariants). The
//! flight is swept in eight shares, two per binary
//! (`commit_fault_wide_drop_0` to `_3`), so each binary, with its final
//! full Store audit, stays within one test command's time. This binary:
//! shares 2 and 3. Needs `LAYERFS_TEST_POSTGRES_URL`;
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
fn a_connection_dropped_at_a_wide_commit_message_share_2() {
    fault_sweep::sweep(
        "drop_wide_2",
        Fault::Drop,
        Flight::Wide,
        Positions::Share(2, 8),
    );
}

#[test]
fn a_connection_dropped_at_a_wide_commit_message_share_3() {
    fault_sweep::sweep(
        "drop_wide_3",
        Fault::Drop,
        Flight::Wide,
        Positions::Share(3, 8),
    );
}
