//! #768: a dropped connection at every server message of one repeated Workspace
//! mount, and of one unmount (the owner's release, #768 step C), on
//! PostgreSQL (`support/mount_sweep.rs` states the invariants).
//! The proxy discards message `N` and everything after it and shuts both
//! sockets of that connection. Needs `LAYERFS_TEST_POSTGRES_URL`;
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
fn a_connection_dropped_at_any_message_of_a_mount() {
    mount_sweep::sweep("mount_drop", Fault::Drop, Flight::Mount);
}

#[test]
fn a_connection_dropped_at_any_message_of_an_unmount() {
    mount_sweep::sweep("unmount_drop", Fault::Drop, Flight::Unmount);
}
