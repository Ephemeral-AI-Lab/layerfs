//! #768 independent verification on PostgreSQL: the Store's allocators after
//! a Commit whose reply is lost at every server message.
//!
//! Each run Commits a change that takes many inode ordinals (a new
//! directory of 150 small files, 40 on the raced flight) and new pack ids (a 140 KiB file) through a
//! proxy that faults message `N`: the connection dropped (`Drop`) or the
//! backend killed while its replies are held (`HoldKill`). Flights: a warm
//! Commit (the handle's kept id block), the first Commit of a newly opened
//! service (a fresh reservation and the ordinal tail released in the same
//! message), and a raced one (the fallback path). The service is abandoned
//! after the follow-up Commit, so its kept block is leaked, never released.
//!
//! After every run, besides the Branch being old or exactly one Commit on
//! (`adversary::check_attempt`), the allocator never stands behind a stored
//! row: no pack id or inode ordinal at or past `store_policy`'s counters, no
//! two value blocks overlapping, no locator into a missing pack, no object
//! located twice. Every tenth run a direct service Commits another 150-file
//! change, so ids reserved after a rewind are used. At the end the audit
//! and amcheck pass. Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
//!
//! Split into nine binaries so each stays well under the 120 s test ceiling
//! on a loaded host: `verify_unknown_allocators` and `_warm_b` hold the
//! halves of the warm flights, `_cold_a` to `_cold_d` the quarters of the
//! cold flights, and `_raced_a` to `_raced_c` the ninths of the raced
//! flights. The final audit has its own 30 s deadline, which a host loaded
//! well past its cores misses whatever the sweep's size (`Cancelled`). A
//! process runs at most `verify768::SWEEPS` sweeps at once. More at once exhausts macOS's ephemeral ports (about 16k,
//! TIME_WAIT about 30 s) and later dials fail `Refused { status: "Closed" }`.
// Each binary runs part of the shared body.
#![allow(dead_code)]
use crate::adversary;
use crate::fault_proxy;
use crate::verify768;
include!("../support/verify/unknown_body.rs");

#[test]
fn drop_raced_part_3() {
    sweep("v768u_dr3", Flight::Raced, Fault::Drop, 3, 9);
}
#[test]
fn drop_raced_part_4() {
    sweep("v768u_dr4", Flight::Raced, Fault::Drop, 4, 9);
}
#[test]
fn drop_raced_part_5() {
    sweep("v768u_dr5", Flight::Raced, Fault::Drop, 5, 9);
}
