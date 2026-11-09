//! #547 regression coverage for the pure control-slot state machine.
//!
//! #543 deleted the inline `#[cfg(test)]` modules that proved the #540
//! invariants: a retained failure never owns a slot, a stale or duplicate
//! finish changes nothing, and the retained ring stays bounded and timed.
//! These tests live outside product `src/`; they reach only the pure
//! `Slots`/`RetainedFailures` values, no socket and no daemon process.

use layerfs_bridge::control::{Call, Reply, Request};
use layerfs_bridge::duplex::{ChannelError, ChannelWork, PeerEndFailure, PeerEndPhase};
use layerfs_daemon::application::internals::{
    disposition, AdmissionClass, Finished, RetainedFailure, RetainedFailures, SlotClass, Slots,
    LIMIT, RESERVED_SLOTS, TTL,
};
use layerfs_daemon::application::{ApplicationError, ConnectionFailure};
use layerfs_daemon::control::{Served, Success};
use std::time::{Duration, Instant};

const OWNER: &str = "daemon:pid:layerfs-control-0";

/// A failure after a decoded request: retained, never released.
fn failure(slot: usize) -> ConnectionFailure {
    ConnectionFailure {
        slot,
        received: None,
        original: None,
        cause: ApplicationError::Poisoned,
        fence_error: None,
        diagnostic_error: None,
    }
}

fn describe(slot: usize, class: SlotClass, generation: u64) -> RetainedFailure {
    RetainedFailure::describe(&failure(slot), class, generation, OWNER)
}

/// A pool admission never takes the reserved slot and vice versa.
#[test]
fn admit_uses_only_its_class() {
    let mut slots = Slots::new(8 + RESERVED_SLOTS);
    for _ in 0..8 {
        slots.admit(AdmissionClass::ControlPool).unwrap();
    }
    assert_eq!(
        slots.admit(AdmissionClass::ControlPool),
        None,
        "a pool admission took the reserved slot"
    );
    let reserved = slots.admit(AdmissionClass::Install).unwrap();
    assert_eq!(reserved.index, 8);
    assert_eq!(reserved.class, SlotClass::Reserved);
    let counts = slots.counts();
    assert_eq!(counts.pool_vacant, 0);
    assert_eq!(counts.pool_running, 8);
    assert_eq!(counts.reserved_vacant, 0);
    assert_eq!(counts.reserved_running, 1);
}

/// A retained failure is a record, not a slot: finishing the worker frees the
/// slot, and the bounded record is stored beside the slots (#540).
#[test]
fn finish_frees_even_for_a_retained_failure() {
    let mut slots = Slots::new(8 + RESERVED_SLOTS);
    let mut store = RetainedFailures::default();
    let now = Instant::now();
    let ticket = slots.admit(AdmissionClass::ControlPool).unwrap();
    let classified = disposition(&failure(ticket.index), ticket, OWNER, now);
    match slots.finish(ticket, Err(failure(ticket.index)), Some(classified)) {
        Finished::Retained(record) => {
            assert_eq!(record.class, "pool");
            assert_eq!(record.generation, ticket.generation);
            store.record(*record, now);
        }
        _ => panic!("a decoded failure was not retained"),
    }
    assert_eq!(slots.counts().pool_vacant, 8);
    assert_eq!(store.snapshot(now).failures.len(), 1);
}

/// Every running slot ends exactly once; a duplicate or stale finish releases
/// nothing and cannot free a newer worker (invariant 1).
#[test]
fn stale_or_duplicate_finish_changes_nothing() {
    let mut slots = Slots::new(8 + RESERVED_SLOTS);
    let now = Instant::now();
    let first = slots.admit(AdmissionClass::ControlPool).unwrap();
    let classified = disposition(&failure(first.index), first, OWNER, now);
    slots.finish(first, Err(failure(first.index)), Some(classified));
    // A duplicate finish of the same ticket changes nothing.
    assert!(matches!(slots.finish(first, Ok(()), None), Finished::Stale));
    let second = slots.admit(AdmissionClass::ControlPool).unwrap();
    assert_eq!(second.index, first.index);
    assert!(second.generation > first.generation);
    // The old ticket cannot free the newer worker.
    assert!(matches!(slots.finish(first, Ok(()), None), Finished::Stale));
    assert_eq!(slots.counts().pool_running, 1);
    slots.finish(second, Ok(()), None);
    assert_eq!(slots.counts().pool_vacant, 8);
}

/// The retained ring is bounded: the 65th record evicts the oldest and each
/// eviction increments `dropped` (invariant 3).
#[test]
fn ring_limit_evicts_oldest_and_counts_drops() {
    let mut store = RetainedFailures::new(3, TTL);
    let now = Instant::now();
    for slot in 0..5 {
        store.record(describe(slot, SlotClass::Pool, slot as u64 + 1), now);
    }
    let snapshot = store.snapshot(now);
    assert_eq!(snapshot.failures.len(), 3);
    assert_eq!(snapshot.dropped, 2);
    assert_eq!(snapshot.recorded, 5);
    assert_eq!(
        snapshot.failures.iter().map(|f| f.slot).collect::<Vec<_>>(),
        vec![2, 3, 4]
    );
    assert_eq!(snapshot.limit, 3);
    assert_eq!(snapshot.ttl, TTL);

    let mut full = RetainedFailures::new(LIMIT, TTL);
    for slot in 0..LIMIT + 1 {
        full.record(describe(slot % 8, SlotClass::Pool, slot as u64 + 1), now);
    }
    let snapshot = full.snapshot(now);
    assert_eq!(snapshot.failures.len(), LIMIT);
    assert_eq!(snapshot.dropped, 1);
    assert_eq!(snapshot.recorded as usize, LIMIT + 1);
}

/// Records past their TTL are never returned and count as dropped.
#[test]
fn ttl_prunes_expired_records() {
    let mut store = RetainedFailures::new(8, Duration::from_secs(10));
    let now = Instant::now();
    store.record(describe(1, SlotClass::Pool, 1), now);
    store.record(
        describe(2, SlotClass::Pool, 2),
        now + Duration::from_secs(5),
    );
    let snapshot = store.snapshot(now + Duration::from_secs(9));
    assert_eq!(snapshot.failures.len(), 2);
    assert_eq!(snapshot.failures[0].expires_in, Duration::from_secs(1));
    let snapshot = store.snapshot(now + Duration::from_secs(16));
    assert_eq!(snapshot.failures.len(), 0);
    assert_eq!(snapshot.dropped, 2);
}

/// The accept loop's lazy reaper frees expired records before it refuses.
#[test]
fn retained_store_reaps_expired_records() {
    let mut slots = Slots::new(1 + RESERVED_SLOTS);
    let mut store = RetainedFailures::default();
    let now = Instant::now();
    let ticket = slots.admit(AdmissionClass::ControlPool).unwrap();
    let classified = disposition(&failure(ticket.index), ticket, OWNER, now);
    if let Finished::Retained(record) =
        slots.finish(ticket, Err(failure(ticket.index)), Some(classified))
    {
        store.record(*record, now);
    }
    store.reap(now + TTL + Duration::from_secs(1));
    assert_eq!(
        store
            .snapshot(now + TTL + Duration::from_secs(1))
            .failures
            .len(),
        0
    );
}

/// The textual cause is bounded and cut on a character boundary.
#[test]
fn cause_is_bounded_and_char_safe() {
    let mut failure = failure(0);
    failure.cause = ApplicationError::Worker("é".repeat(4096));
    let record = RetainedFailure::describe(&failure, SlotClass::Pool, 1, "owner");
    assert!(record.cause.len() <= 512 + "…".len());
    assert!(record.cause.ends_with('…'));
    assert!(record.cause.is_char_boundary(record.cause.len()));
}

/// A record names the slot, class, generation, owner, request, reply state and
/// cause the operator needs — not just a count.
#[test]
fn record_names_the_cause_request_and_reply_state() {
    let served = Served {
        call: Call {
            id: 1,
            request: Request::EndSession,
        },
        outcome: Ok(Success {
            reply: Reply::SessionEnded,
            commit: None,
            observation_failure: None,
            kernel: None,
        }),
    };
    let failure = ConnectionFailure {
        slot: 3,
        received: Some(vec![0xde, 0xad, 0xbe, 0xef]),
        original: None,
        cause: ApplicationError::SessionEnd {
            served: Box::new(served),
            ending: Box::new(PeerEndFailure {
                cause: ChannelError::Invalid("peer reset after final reply"),
                phase: PeerEndPhase::Read,
                fence_attempted: false,
                received: None,
                work: ChannelWork::default(),
            }),
        },
        fence_error: None,
        diagnostic_error: None,
    };
    let record = RetainedFailure::describe(
        &failure,
        SlotClass::Pool,
        9,
        "a1b2c3d4:812:layerfs-control-3",
    );
    assert_eq!(record.slot, 3);
    assert_eq!(record.class, "pool");
    assert_eq!(record.generation, 9);
    assert_eq!(record.received_bytes, Some(4));
    assert_eq!(record.request, Some("end-session"));
    assert!(record.reply_sent);
    assert!(record.cause.contains("session"), "{}", record.cause);
    let line = record.to_string();
    assert!(line.starts_with("ConnectionFailure {"), "{line}");
    assert!(
        line.contains("original_kind: Some(\"end-session\")"),
        "{line}"
    );
    assert!(line.contains("reply: \"sent\""), "{line}");
    assert!(line.contains("class: \"pool\""), "{line}");
    assert!(line.contains("owner:"), "{line}");
    assert!(line.contains("expires_in_ms:"), "{line}");
}

/// A record names the slot, class, generation and owner that a retained
/// record reports, with its deadline visible.
#[test]
fn owner_and_deadline_are_visible() {
    let mut store = RetainedFailures::new(8, TTL);
    let now = Instant::now();
    let record = store.record(describe(3, SlotClass::Pool, 9), now);
    assert_eq!(record.class, "pool");
    assert_eq!(record.generation, 9);
    assert!(
        record.owner.contains("layerfs-control-"),
        "{}",
        record.owner
    );
    assert_eq!(record.retained, 1);
    let snapshot = store.snapshot(now + Duration::from_secs(1));
    assert_eq!(
        snapshot.failures[0].expires_in,
        TTL - Duration::from_secs(1)
    );
}

/// A worker whose guard is dropped without an explicit finish releases its
/// slot and records the abandonment exactly once, like any other retained
/// failure (invariant 1).
#[test]
fn guard_drop_releases_the_slot_and_records_once() {
    let mut slots = Slots::new(1 + RESERVED_SLOTS);
    let mut store = RetainedFailures::default();
    let now = Instant::now();
    let ticket = slots.admit(AdmissionClass::ControlPool).unwrap();
    let failure = ConnectionFailure {
        slot: ticket.index,
        received: None,
        original: None,
        cause: ApplicationError::Worker("dropped before completion".to_owned()),
        fence_error: None,
        diagnostic_error: None,
    };
    let classified = disposition(&failure, ticket, OWNER, now);
    match slots.finish(ticket, Err(failure), Some(classified)) {
        Finished::Retained(record) => {
            assert_eq!(record.slot, ticket.index);
            assert_eq!(record.class, "pool");
            assert!(!record.reply_sent);
            assert!(
                record.cause.contains("dropped before completion"),
                "{}",
                record.cause
            );
            store.record(*record, now);
        }
        _ => panic!("an abandoned slot was not recorded"),
    }
    let counts = slots.counts();
    assert_eq!(counts.pool_running, 0);
    assert_eq!(counts.pool_vacant, 1);
    // A second finish of the same ticket is stale: the record is never doubled.
    assert!(matches!(
        slots.finish(ticket, Ok(()), None),
        Finished::Stale
    ));
    assert_eq!(store.snapshot(now).failures.len(), 1);
}
