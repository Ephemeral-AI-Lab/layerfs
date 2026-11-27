//! #818 lane RH (H4): a connection dropped at every server message of one
//! cold `fs.blame` whose walk reads first-parent windows
//! (`lineage_chain_window`) on PostgreSQL (`support/read_sweep_rh.rs` states
//! the invariants). Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
use crate::fault_proxy;
use crate::read_sweep_rh;

#[test]
fn a_connection_dropped_at_any_message_of_a_blame() {
    read_sweep_rh::sweep_blame("history_rh_drop", fault_proxy::Fault::Drop);
}
