//! #810 F5: a killed backend (the message held, the backend terminated) at every server
//! message of a Commit whose Workspace reader opens its changed base files
//! in one batched window (`support/base_file_sweep.rs` states the
//! invariants). Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "../support/verify/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "../support/fault/base_file_sweep.rs"]
mod base_file_sweep;
#[path = "../support/fault/proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;

#[test]
fn a_backend_killed_at_any_message_of_a_batched_base_file_commit() {
    base_file_sweep::sweep(
        "f5_kill",
        fault_proxy::Fault::HoldKill,
        base_file_sweep::Lengths::Stated,
    );
}

#[test]
fn a_backend_killed_at_any_message_of_a_commit_whose_lengths_are_not_located() {
    base_file_sweep::sweep(
        "f5_kill_cold",
        fault_proxy::Fault::HoldKill,
        base_file_sweep::Lengths::Elsewhere,
    );
}
