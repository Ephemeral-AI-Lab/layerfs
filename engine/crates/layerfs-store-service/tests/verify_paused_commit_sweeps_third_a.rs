//! #768 independent verification on PostgreSQL: a Workspace Commit whose
//! session is paused at every server message of its flight (the proxy holds
//! the replies from message `N`; the client is stopped there), and
//! disturbed from elsewhere while it waits:
//!
//! * `Swap`: what a compaction swap does, faithfully: one transaction locks
//!   the base tables `ACCESS EXCLUSIVE NOWAIT` and bumps
//!   `history_meta.catalog_incarnation`, returning the history serial it
//!   saw. Either the lock is refused (the paused Commit holds a lock: a real
//!   swap is `Busy` there) and the Commit must land, or the generation moved
//!   and the Commit lands only if its Commit row's history serial is below
//!   the bump's (published before the swap), else it is a definite `Stale`
//!   refusal with the Branch at its old head. A Commit published at the old
//!   generation after the swap fails the test.
//! * `Third`: a third writer (another service, short lane wait) commits its
//!   own file on the same Branch while the Commit is paused. Every answer is
//!   Committed or a definite refusal; the Branch's tree is exactly the seed,
//!   the head mover's file, and the file of each Commit answered Committed:
//!   no lost update and no publication nobody answered for.
//!
//! Each runs on a warm fused Commit and on a raced one (another Workspace
//! moved the head first: the fused attempt's race refusal and the fallback
//! on the separate path). Shares of each flight run as separate tests. At
//! the end every Commit row is on a chain, the audit and amcheck pass.
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
//!
//! Split into three binaries (this one, `verify_paused_commit_sweeps_third_a`
//! and `_third_b`) so each stays well under the 120 s test ceiling; a
//! process runs at most `verify768::SWEEPS` sweeps at once (see
//! `verify_unknown_allocators`).
// Each binary runs part of the shared body.
#![allow(dead_code)]
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "support/fault_proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;
#[path = "support/verify768.rs"]
#[allow(dead_code)]
mod verify768;
include!("support/verify_paused_body.rs");

#[test]
fn third_writer_at_every_pause_of_a_raced_commit_part_0() {
    sweep("v768p_th_r0", Flight::Raced, Disturb::Third, 0);
}

#[test]
fn third_writer_at_every_pause_of_a_raced_commit_part_1() {
    sweep("v768p_th_r1", Flight::Raced, Disturb::Third, 1);
}

#[test]
fn third_writer_at_every_pause_of_a_raced_commit_part_2() {
    sweep("v768p_th_r2", Flight::Raced, Disturb::Third, 2);
}
