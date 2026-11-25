//! #810 F5: a dropped connection at every server
//! message of a Commit whose Workspace reader opens its changed base files
//! in one batched window (`support/base_file_sweep.rs` states the
//! invariants). Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "support/base_file_sweep.rs"]
mod base_file_sweep;
#[path = "support/fault_proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;

#[test]
fn a_connection_dropped_at_any_message_of_a_batched_base_file_commit() {
    base_file_sweep::sweep("f5_drop", fault_proxy::Fault::Drop);
}
