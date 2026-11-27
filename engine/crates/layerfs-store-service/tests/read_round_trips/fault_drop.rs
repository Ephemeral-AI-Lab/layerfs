//! #818 lane RH: a connection dropped at every server message of one cold
//! SDK read (`fs.stat` on a Branch with a marker present) on PostgreSQL
//! (`support/read_sweep_rh.rs` states the invariants). Needs
//! `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "../support/verify/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "../support/fault/proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;
#[path = "../support/fault/read_sweep.rs"]
#[allow(dead_code)]
mod read_sweep_rh;

#[test]
fn a_connection_dropped_at_any_message_of_a_read() {
    read_sweep_rh::sweep("read_rh_drop", fault_proxy::Fault::Drop);
}
