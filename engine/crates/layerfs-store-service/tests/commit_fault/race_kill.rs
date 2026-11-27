//! #768: the backend killed (`pg_terminate_backend`) while its replies are
//! held, the held replies delivered once it is gone,
//! at every server message of a Workspace Commit whose head another
//! Workspace moved first, on PostgreSQL: the fused attempt (refused as a
//! race, nothing written) and the fallback on the separate path that merges
//! and publishes in the same call. Besides `support/fault_sweep.rs`'s
//! invariants, the call falls back at most once, and never when the fault
//! came at or before the race refusal reached the client. Needs
//! `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
use crate::fault_proxy;
use crate::fault_sweep;

use fault_proxy::Fault;
use fault_sweep::{Flight, Positions};

/// The flight is long (the fused attempt, then the separate path), so
/// four tests sweep a quarter of its positions each, in parallel, each on
/// its own Store; together they cover every position.
const PARTS: u64 = 4;

fn part(part: u64) {
    fault_sweep::sweep(
        &format!("kill_raced_{part}"),
        Fault::HoldKill,
        Flight::Raced,
        Positions::Share(part, PARTS),
    );
}

#[test]
fn the_backend_killed_at_any_message_of_a_raced_commit_part_0() {
    part(0);
}

#[test]
fn the_backend_killed_at_any_message_of_a_raced_commit_part_1() {
    part(1);
}

#[test]
fn the_backend_killed_at_any_message_of_a_raced_commit_part_2() {
    part(2);
}

#[test]
fn the_backend_killed_at_any_message_of_a_raced_commit_part_3() {
    part(3);
}
