//! #818 lane RH: the backend killed at every server message of one cold SDK
//! read (`fs.stat` on a Branch with a marker present) on PostgreSQL, its
//! replies held until it is gone (`support/read_sweep_rh.rs` states the
//! invariants). Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "support/fault_proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;
#[path = "support/read_sweep_rh.rs"]
mod read_sweep_rh;

#[test]
fn a_backend_killed_at_any_message_of_a_read() {
    read_sweep_rh::sweep("read_rh_kill", fault_proxy::Fault::HoldKill);
}
