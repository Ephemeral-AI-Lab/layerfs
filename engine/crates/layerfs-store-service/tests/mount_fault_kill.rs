//! #768: a killed backend at every server message of one repeated Workspace
//! mount, and of one unmount (the owner's release, #768 step C), on
//! PostgreSQL (`support/mount_sweep.rs` states the invariants).
//! The proxy holds message `N` and every later reply of that connection,
//! terminates its backend, then delivers what the server sent before it
//! ended. Needs `LAYERFS_TEST_POSTGRES_URL`;
//! NOT_RUN without it.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "support/fault_proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;
#[path = "support/mount_sweep.rs"]
#[allow(dead_code)]
mod mount_sweep;

use fault_proxy::Fault;
use mount_sweep::Flight;

#[test]
fn a_backend_killed_at_any_message_of_a_mount() {
    mount_sweep::sweep("mount_kill", Fault::HoldKill, Flight::Mount);
}

#[test]
fn a_backend_killed_at_any_message_of_an_unmount() {
    mount_sweep::sweep("unmount_kill", Fault::HoldKill, Flight::Unmount);
}
