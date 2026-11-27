//! #768 independent verification, findings 1 and 2: a connection lost in
//! the part of a raced Workspace Commit where the fused attempt has been
//! refused as a race and the call goes on without a Commit outstanding, on
//! PostgreSQL. Besides `support/fault_sweep.rs`'s invariants (among them:
//! the Workspace's follow-up Commit is never refused `InFlight` with
//! nothing retained to resolve):
//!
//! * dropped among the fused message's replies after its race refusal (the
//!   race in hand, the session quarantined): no fallback starts on the lost
//!   session, and the call answers a definite refusal;
//! * dropped or its backend killed while the fallback publishes the held
//!   tail and records its hint rows (and what follows on the writer before
//!   another connection opens): the call folds back as a failed `finish`
//!   does, and the Workspace is not locked out.
//!
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "../support/verify/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "../support/fault/proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;
#[path = "../support/fault/sweep.rs"]
#[allow(dead_code)]
mod fault_sweep;

use fault_proxy::Fault;
use fault_sweep::{Flight, Positions};

#[test]
fn a_connection_dropped_after_the_race_refusal_does_not_fall_back() {
    fault_sweep::sweep(
        "custody_race_replies",
        Fault::Drop,
        Flight::Raced,
        Positions::RaceReplies,
    );
}

#[test]
fn a_connection_dropped_in_the_fallback_publication_keeps_the_workspace() {
    fault_sweep::sweep(
        "custody_fallback_drop",
        Fault::Drop,
        Flight::Raced,
        Positions::Fallback,
    );
}

#[test]
fn a_backend_killed_in_the_fallback_publication_keeps_the_workspace() {
    fault_sweep::sweep(
        "custody_fallback_kill",
        Fault::HoldKill,
        Flight::Raced,
        Positions::Fallback,
    );
}
