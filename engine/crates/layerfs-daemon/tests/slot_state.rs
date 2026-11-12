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
    disposition, recovery_request, AdmissionClass, Finished, RetainedFailure, RetainedFailures,
    SlotClass, SlotTicket, Slots, LIMIT, RESERVED_SLOTS, TTL,
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

/// A recovery-class admission takes the reserved slot, never a pool slot.
#[test]
fn recovery_admission_takes_the_reserved_slot() {
    let mut slots = Slots::new(8 + RESERVED_SLOTS);
    let reserved = slots.admit(AdmissionClass::Recovery).unwrap();
    assert_eq!(reserved.class, SlotClass::Reserved);
    assert_eq!(reserved.index, 8);
    assert_eq!(slots.counts().reserved_running, 1);
}

/// An admitted slot starts Busy (handshake), becomes Idle before blocking in
/// `receive()`, and returns to Busy when a request arrives.
#[test]
fn idle_and_busy_track_the_reapable_phase() {
    let mut slots = Slots::new(1 + RESERVED_SLOTS);
    let now = Instant::now();
    let ttl = Duration::from_secs(1);
    let ticket = slots.admit(AdmissionClass::ControlPool).unwrap();
    // Busy: never reapable, even past the TTL.
    assert!(slots
        .reap_idle(now + ttl + Duration::from_secs(1), ttl)
        .is_empty());
    // Idle since `now`: reapable once `idle_ttl` has passed.
    assert!(slots.idle(ticket, now));
    assert_eq!(slots.reap_idle(now + ttl, ttl), vec![ticket]);
    // A fresh worker is Busy again and cannot be reaped while idle is false.
    let second = slots.admit(AdmissionClass::ControlPool).unwrap();
    assert_eq!(second.index, ticket.index);
    assert!(slots.idle(second, now));
    assert!(slots.busy(second));
    assert!(slots
        .reap_idle(now + ttl + Duration::from_secs(1), ttl)
        .is_empty());
    // A stale ticket changes nothing.
    slots.finish(second, Ok(()), None);
    assert!(!slots.busy(second));
    assert!(!slots.idle(second, now));
}

/// `reap_idle` frees only pool slots parked in `receive()` past `idle_ttl`;
/// a busy slot, a recently-idle slot and the reserved slot stay put.
#[test]
fn reap_idle_frees_only_pool_slots_parked_past_ttl() {
    let mut slots = Slots::new(3 + RESERVED_SLOTS);
    let now = Instant::now();
    let ttl = Duration::from_secs(5);
    let early = slots.admit(AdmissionClass::ControlPool).unwrap();
    // Never marked idle: it stays Busy and must survive the reap.
    let _busy = slots.admit(AdmissionClass::ControlPool).unwrap();
    let recent = slots.admit(AdmissionClass::ControlPool).unwrap();
    let reserved = slots.admit(AdmissionClass::Recovery).unwrap();
    assert!(slots.idle(early, now));
    assert!(slots.idle(recent, now + ttl - Duration::from_millis(1)));
    assert!(slots.idle(reserved, now));
    let at = now + ttl;
    assert_eq!(slots.reap_idle(at, ttl), vec![early]);
    let counts = slots.counts();
    assert_eq!(counts.pool_vacant, 1);
    assert_eq!(counts.pool_running, 2);
    // The reserved slot is never reclaimed by the idle reaper.
    assert_eq!(counts.reserved_running, 1);
}

/// A reaped slot is immediately reusable; the woken worker's stale finish
/// cannot free the newer worker (invariant 1).
#[test]
fn reaped_slot_readmits_and_the_old_ticket_finishes_stale() {
    let mut slots = Slots::new(1 + RESERVED_SLOTS);
    let now = Instant::now();
    let ttl = Duration::from_secs(1);
    let first = slots.admit(AdmissionClass::ControlPool).unwrap();
    assert!(slots.idle(first, now));
    assert_eq!(slots.reap_idle(now + ttl, ttl), vec![first]);
    let second = slots.admit(AdmissionClass::ControlPool).unwrap();
    assert_eq!(second.index, first.index);
    assert!(second.generation > first.generation);
    assert!(matches!(slots.finish(first, Ok(()), None), Finished::Stale));
    assert_eq!(slots.counts().pool_running, 1);
    slots.finish(second, Ok(()), None);
    assert_eq!(slots.counts().pool_vacant, 1);
}

/// The recovery operation set: close (`Unmount`), workspace status
/// (`Status`) and the operator read model — and nothing else (#547 slice 3).
#[test]
fn recovery_request_admits_only_the_recovery_set() {
    use layerfs_bridge::control::{SdkCall, UnmountControl};
    use layerfs_history::WorkspaceId;
    let token = layerfs_bridge::control::WorkspaceToken {
        workspace: WorkspaceId::from_authority([1; 32]).unwrap(),
        namespace: 0,
    };
    assert!(recovery_request(&Request::Status(token)));
    assert!(recovery_request(&Request::Unmount(UnmountControl {
        token,
        discard: false,
    })));
    assert!(recovery_request(&Request::Sdk(SdkCall {
        method: layerfs_bridge::control::OPERATION.to_owned(),
        args: layerfs_bridge::control::SdkValue::Null,
    })));
    // Everything else is refused.
    assert!(!recovery_request(&Request::EndSession));
    assert!(!recovery_request(&Request::Hello(
        layerfs_bridge::control::HelloRequest {
            expected_instance: None,
            wait_for_store: false,
        }
    )));
    assert!(!recovery_request(&Request::Sdk(SdkCall {
        method: "sandbox.list_workspaces".to_owned(),
        args: layerfs_bridge::control::SdkValue::Null,
    })));
    assert!(!recovery_request(&Request::MountNamed {
        workspace: WorkspaceId::from_authority([2; 32]).unwrap(),
        project: "project".to_owned(),
        branch: "branch".to_owned(),
    }));
    assert!(!recovery_request(&Request::Commit(
        layerfs_bridge::control::CommitControl {
            token,
            force: false,
            message: None,
        }
    )));
}

/// A slot marked in-flight past its deadline is expired; the reserved slot is
/// never expired automatically (install is one-time, recovery closes after
/// one request) (#547 slice 5).
#[test]
fn inflight_slots_expire_only_past_their_deadline() {
    let mut slots = Slots::new(3 + RESERVED_SLOTS);
    let now = Instant::now();
    let early = slots.admit(AdmissionClass::ControlPool).unwrap();
    let late = slots.admit(AdmissionClass::ControlPool).unwrap();
    let reserved = slots.admit(AdmissionClass::Recovery).unwrap();
    assert!(slots.inflight(early, now, now + Duration::from_secs(5)));
    assert!(slots.inflight(late, now, now + Duration::from_secs(20)));
    assert!(slots.inflight(reserved, now, now + Duration::from_secs(5)));
    // Before any deadline, nothing expires.
    assert!(slots.reap_expired(now + Duration::from_secs(1)).is_empty());
    // At 5 s only `early` expired, with its exact ticket.
    let expired = slots.reap_expired(now + Duration::from_secs(5));
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0], early);
    // At 20 s `late` expires too; the reserved slot still stays.
    let expired = slots.reap_expired(now + Duration::from_secs(20));
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0], late);
    let counts = slots.counts();
    assert_eq!(counts.pool_vacant, 3);
    assert_eq!(counts.pool_running, 0);
    assert_eq!(counts.reserved_running, 1);
}

/// A reaped in-flight slot is immediately reusable; the woken worker's stale
/// finish cannot free the newer worker (invariant 1, #547 slice 5).
#[test]
fn expired_slot_readmits_and_the_old_ticket_finishes_stale() {
    let mut slots = Slots::new(1 + RESERVED_SLOTS);
    let now = Instant::now();
    let first = slots.admit(AdmissionClass::ControlPool).unwrap();
    assert!(slots.inflight(first, now, now + Duration::from_secs(1)));
    assert_eq!(
        slots.reap_expired(now + Duration::from_secs(1)),
        vec![first]
    );
    let second = slots.admit(AdmissionClass::ControlPool).unwrap();
    assert_eq!(second.index, first.index);
    assert!(second.generation > first.generation);
    assert!(matches!(slots.finish(first, Ok(()), None), Finished::Stale));
    assert_eq!(slots.counts().pool_running, 1);
    slots.finish(second, Ok(()), None);
    assert_eq!(slots.counts().pool_vacant, 1);
}

/// Reclaim frees exactly the named generation, on any class, and is one-shot
/// (#547 slice 5).
#[test]
fn reclaim_frees_the_matching_generation_only() {
    let mut slots = Slots::new(2 + RESERVED_SLOTS);
    let now = Instant::now();
    let first = slots.admit(AdmissionClass::ControlPool).unwrap();
    assert!(slots.inflight(first, now, now + Duration::from_secs(60)));
    // A wrong generation changes nothing.
    assert!(slots.reclaim(first.index, first.generation + 1).is_none());
    assert_eq!(slots.counts().pool_running, 1);
    // The right generation frees the slot and returns its exact ticket.
    let ticket = slots.reclaim(first.index, first.generation).unwrap();
    assert_eq!(ticket, first);
    assert_eq!(slots.counts().pool_vacant, 2);
    // One-shot: the same generation no longer matches.
    assert!(slots.reclaim(first.index, first.generation).is_none());
    // The reserved slot is reclaimable too.
    let reserved = slots.admit(AdmissionClass::Recovery).unwrap();
    assert!(slots.reclaim(reserved.index, reserved.generation).is_some());
    assert_eq!(slots.counts().reserved_vacant, 1);
}

/// `Expired` and `Reclaimed` records carry their deciding cause, the slot and
/// generation, and never claim a reply was sent (#547 slice 5).
#[test]
fn expired_and_reclaimed_records_name_their_disposition() {
    let ticket = SlotTicket {
        index: 2,
        generation: 7,
        class: SlotClass::Pool,
    };
    let expired = RetainedFailure::expired(ticket, OWNER);
    assert_eq!(expired.slot, 2);
    assert_eq!(expired.class, "pool");
    assert_eq!(expired.generation, 7);
    assert!(!expired.reply_sent);
    assert_eq!(expired.cause, "Expired");
    assert!(expired.received_bytes.is_none());
    assert!(expired.request.is_none());
    let reclaimed = RetainedFailure::reclaimed(ticket, OWNER);
    assert_eq!(reclaimed.slot, 2);
    assert!(!reclaimed.reply_sent);
    assert_eq!(reclaimed.cause, "Reclaimed");
}

/// `recovery_request` admits reclaim alongside the operator read model
/// (#547 slice 5).
#[test]
fn recovery_request_admits_reclaim() {
    assert!(recovery_request(&Request::Sdk(
        layerfs_bridge::control::SdkCall {
            method: layerfs_bridge::control::RECLAIM.to_owned(),
            args: layerfs_bridge::control::SdkValue::Null,
        }
    )));
}
