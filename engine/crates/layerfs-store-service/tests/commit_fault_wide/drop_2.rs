//! #810: a dropped connection at every server message of one wide Workspace
//! Commit on PostgreSQL (`Flight::Wide`: its reads before the lane go in
//! batched windows; `support/fault_sweep.rs` states the invariants). The
//! flight is swept in eight shares, two per binary
//! (`commit_fault_wide_drop_0` to `_3`), so each binary, with its final
//! full Store audit, stays within one test command's time. This binary:
//! shares 4 and 5. Needs `LAYERFS_TEST_POSTGRES_URL`;
//! NOT_RUN without it.
use crate::fault_proxy;
use crate::fault_sweep;

use fault_proxy::Fault;
use fault_sweep::{Flight, Positions};

#[test]
fn a_connection_dropped_at_a_wide_commit_message_share_4() {
    fault_sweep::sweep(
        "drop_wide_4",
        Fault::Drop,
        Flight::Wide,
        Positions::Share(4, 8),
    );
}

#[test]
fn a_connection_dropped_at_a_wide_commit_message_share_5() {
    fault_sweep::sweep(
        "drop_wide_5",
        Fault::Drop,
        Flight::Wide,
        Positions::Share(5, 8),
    );
}
