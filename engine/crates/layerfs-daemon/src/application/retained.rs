//! Bounded operator-visible records of connection failures that used to retain a slot.
//!
//! A failed connection worker used to keep its admission slot `Retained` until
//! an explicit transfer that no product path ever made. That cost one of the
//! daemon's fixed control slots per failure until every new connection was
//! refused (#540). The slot is now released at once; what survives here is a
//! compact, bounded, owned and timed record with its exact deciding cause.
use super::{ApplicationError, ConnectionFailure};
use std::{
    collections::VecDeque,
    fmt,
    time::{Duration, Instant},
};

/// Most retained records kept at once. The oldest is dropped first.
pub(super) const LIMIT: usize = 64;
/// How long a retained record stays operator-visible.
pub(super) const TTL: Duration = Duration::from_secs(60 * 60);
/// Most bytes a retained textual cause keeps.
const CAUSE_BYTES: usize = 512;

/// One connection failure that used to retain its slot, as an operator sees it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedFailure {
    /// Slot the failed worker occupied; it is already free for a new connection.
    pub slot: usize,
    /// `pool` or `reserved`: the class of the slot that was released.
    pub class: &'static str,
    /// Admission generation of the released worker; a stale finish never frees
    /// a newer one.
    pub generation: u64,
    /// The worker that failed: daemon instance, pid and thread.
    pub owner: String,
    /// Bytes of a request that failed decoding, when one was received.
    pub received_bytes: Option<usize>,
    /// `control` or `install` when a request was decoded, else `None`.
    pub request: Option<&'static str>,
    /// Whether the original reply was sent before the failure.
    pub reply_sent: bool,
    /// Bounded textual deciding cause.
    pub cause: String,
    /// Original socket fence failure, when one was observed.
    pub fence_error: Option<String>,
    /// Distinct failed diagnostic transfer, when one happened.
    pub diagnostic_error: Option<String>,
    /// Age at the observation that returned this record.
    pub age: Duration,
    /// Remaining lifetime at the observation that returned this record.
    pub expires_in: Duration,
    /// Visible records at the moment this one was recorded.
    pub retained: usize,
    /// Configured record limit at the moment this one was recorded.
    pub limit: usize,
}

impl RetainedFailure {
    /// The bounded record for `failure`, built outside the state lock.
    pub(super) fn describe(
        failure: &ConnectionFailure,
        class: super::owner::SlotClass,
        generation: u64,
        owner: &str,
    ) -> Self {
        Self {
            slot: failure.slot,
            class: class.as_str(),
            generation,
            owner: owner.to_owned(),
            received_bytes: failure.received.as_ref().map(Vec::len),
            request: request_kind(failure),
            reply_sent: reply_sent(failure),
            cause: bounded(&failure.cause.to_string()),
            fence_error: failure
                .fence_error
                .as_ref()
                .map(|error| bounded(&error.to_string())),
            diagnostic_error: failure
                .diagnostic_error
                .as_ref()
                .map(|error| bounded(&error.to_string())),
            age: Duration::ZERO,
            expires_in: TTL,
            retained: 0,
            limit: LIMIT,
        }
    }
}

impl fmt::Display for RetainedFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnectionFailure")
            .field("slot", &self.slot)
            .field("class", &self.class)
            .field("generation", &self.generation)
            .field("received_bytes", &self.received_bytes)
            .field("original_kind", &self.request)
            .field("reply", &if self.reply_sent { "sent" } else { "not_sent" })
            .field("cause", &self.cause)
            .field("fence_error", &self.fence_error)
            .field("diagnostic_error", &self.diagnostic_error)
            .field("retained", &format!("{}/{}", self.retained, self.limit))
            .field("owner", &self.owner)
            .field("expires_in_ms", &self.expires_in.as_millis())
            .finish()
    }
}

/// A bounded observation of the retained-failure store.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedSnapshot {
    /// Records, oldest first.
    pub failures: Vec<RetainedFailure>,
    /// Records evicted by the limit or the TTL since startup.
    pub dropped: u64,
    /// Total records ever recorded since startup.
    pub recorded: u64,
    /// Configured record limit.
    pub limit: usize,
    /// Configured record TTL.
    pub ttl: Duration,
}

struct Entry {
    record: RetainedFailure,
    at: Instant,
    deadline: Instant,
}

/// A fixed-capacity, TTL-pruned ring of retained connection failures. Its
/// capacity is reserved when it is built, so recording under the state lock
/// never allocates.
pub(super) struct RetainedFailures {
    entries: VecDeque<Entry>,
    dropped: u64,
    recorded: u64,
    limit: usize,
    ttl: Duration,
}

impl Default for RetainedFailures {
    fn default() -> Self {
        Self::new(LIMIT, TTL)
    }
}

impl RetainedFailures {
    pub(super) fn new(limit: usize, ttl: Duration) -> Self {
        let limit = limit.max(1);
        Self {
            entries: VecDeque::with_capacity(limit),
            dropped: 0,
            recorded: 0,
            limit,
            ttl,
        }
    }

    /// Records one already-described failure, prunes expired records and
    /// evicts the oldest beyond the limit. Returns the exact record that now
    /// is visible, with its store position filled in.
    pub(super) fn record(&mut self, mut record: RetainedFailure, now: Instant) -> RetainedFailure {
        self.prune(now);
        while self.entries.len() >= self.limit {
            self.entries.pop_front();
            self.dropped = self.dropped.saturating_add(1);
        }
        record.retained = self.entries.len() + 1;
        record.limit = self.limit;
        record.expires_in = self.ttl;
        self.entries.push_back(Entry {
            record: record.clone(),
            at: now,
            deadline: now + self.ttl,
        });
        self.recorded = self.recorded.saturating_add(1);
        record
    }

    /// Drops records whose deadline passed; a stale record is never returned.
    pub(super) fn prune(&mut self, now: Instant) {
        while let Some(front) = self.entries.front() {
            if now < front.deadline {
                break;
            }
            self.entries.pop_front();
            self.dropped = self.dropped.saturating_add(1);
        }
    }

    /// The lazy reaper: the accept loop calls it when it refuses admission.
    pub(super) fn reap(&mut self, now: Instant) {
        self.prune(now);
    }

    /// A copy of the visible records, oldest first, with their ages.
    pub(super) fn snapshot(&mut self, now: Instant) -> RetainedSnapshot {
        self.prune(now);
        RetainedSnapshot {
            failures: self
                .entries
                .iter()
                .map(|entry| {
                    let mut record = entry.record.clone();
                    record.age = now.saturating_duration_since(entry.at);
                    record.expires_in = self.ttl.saturating_sub(record.age);
                    record
                })
                .collect(),
            dropped: self.dropped,
            recorded: self.recorded,
            limit: self.limit,
            ttl: self.ttl,
        }
    }

    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Whether a request was decoded, from the direct field or the cause that
/// owns the original receipt.
fn request_kind(failure: &ConnectionFailure) -> Option<&'static str> {
    if let Some(original) = &failure.original {
        return Some(match original {
            layerfs_bridge::initial_record::InitialRecord::Control(_) => "control",
            layerfs_bridge::initial_record::InitialRecord::Install(_) => "install",
        });
    }
    match &failure.cause {
        ApplicationError::Control(serve) => serve
            .original
            .as_ref()
            .map(|served| kind(&served.call.request)),
        ApplicationError::RetainedControl(served) => Some(kind(&served.call.request)),
        ApplicationError::SessionEnd { served, .. } => Some(kind(&served.call.request)),
        _ => None,
    }
}

fn kind(request: &layerfs_bridge::control::Request) -> &'static str {
    use layerfs_bridge::control::Request;
    match request {
        Request::Mount { .. } => "mount",
        Request::Commit(_) => "commit",
        Request::Status(_) => "status",
        Request::Unmount(_) => "unmount",
        Request::ForceUnmount { .. } => "force-unmount",
        Request::Attach(_) => "attach",
        Request::Locate(_) => "locate",
        Request::Cleanup(_) => "cleanup",
        Request::Fork(_) => "fork",
        Request::Branches { .. } => "branches",
        Request::History(_) => "history",
        Request::Sdk(_) => "sdk",
        Request::Hello(_) => "hello",
        Request::EndSession => "end-session",
        Request::Observed { .. } => "observed",
    }
}

/// Whether the original reply reached the caller. A send/fence failure did
/// not; a retained receipt or a session-end observation follows a sent reply.
fn reply_sent(failure: &ConnectionFailure) -> bool {
    matches!(
        failure.cause,
        ApplicationError::RetainedControl(_) | ApplicationError::SessionEnd { .. }
    )
}

fn bounded(text: &str) -> String {
    if text.len() <= CAUSE_BYTES {
        return text.to_owned();
    }
    let mut end = CAUSE_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::owner::SlotClass;
    use crate::control::{Served, Success};
    use layerfs_bridge::control::{Call, Reply, Request};
    use layerfs_bridge::duplex::{ChannelError, ChannelWork, PeerEndFailure, PeerEndPhase};

    /// A failure after the daemon sent EndSession's reply: what a client that
    /// dies before reading it leaves behind.
    fn retained_after_reply(slot: usize, _generation: u64) -> ConnectionFailure {
        ConnectionFailure {
            slot,
            received: None,
            original: None,
            cause: ApplicationError::SessionEnd {
                served: Box::new(Served {
                    call: Call {
                        id: 1,
                        request: Request::EndSession,
                    },
                    outcome: Ok(Success::reply(Reply::SessionEnded)),
                }),
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
        }
    }

    fn describe(slot: usize, generation: u64) -> RetainedFailure {
        RetainedFailure::describe(
            &retained_after_reply(slot, generation),
            SlotClass::Pool,
            generation,
            "a1b2c3d4:812:layerfs-control-0",
        )
    }

    #[test]
    fn limit_evicts_oldest_and_counts_drops() {
        let mut store = RetainedFailures::new(3, TTL);
        let now = Instant::now();
        for slot in 0..5 {
            store.record(describe(slot, slot as u64), now);
        }
        assert_eq!(store.len(), 3);
        let snapshot = store.snapshot(now);
        assert_eq!(snapshot.dropped, 2);
        assert_eq!(snapshot.recorded, 5);
        assert_eq!(
            snapshot.failures.iter().map(|f| f.slot).collect::<Vec<_>>(),
            vec![2, 3, 4]
        );
        assert_eq!(snapshot.limit, 3);
        assert_eq!(snapshot.ttl, TTL);
    }

    #[test]
    fn ttl_prunes_expired_records() {
        let mut store = RetainedFailures::new(8, Duration::from_secs(10));
        let now = Instant::now();
        store.record(describe(1, 1), now);
        store.record(describe(2, 2), now + Duration::from_secs(5));
        let snapshot = store.snapshot(now + Duration::from_secs(9));
        assert_eq!(snapshot.failures.len(), 2);
        assert_eq!(snapshot.failures[0].expires_in, Duration::from_secs(1));
        let snapshot = store.snapshot(now + Duration::from_secs(16));
        assert_eq!(snapshot.failures.len(), 0);
        assert_eq!(snapshot.dropped, 2);
    }

    #[test]
    fn cause_is_bounded_and_char_safe() {
        let long = "é".repeat(4096);
        let bounded = bounded(&long);
        assert!(bounded.len() <= CAUSE_BYTES + "…".len());
        assert!(bounded.ends_with('…'));
        assert!(bounded.is_char_boundary(bounded.len()));
    }

    #[test]
    fn owner_and_deadline_are_visible() {
        let mut store = RetainedFailures::new(8, TTL);
        let now = Instant::now();
        let record = store.record(describe(3, 9), now);
        assert_eq!(record.class, "pool");
        assert_eq!(record.generation, 9);
        assert!(record.owner.contains("812"), "{}", record.owner);
        assert_eq!(record.retained, 1);
        let snapshot = store.snapshot(now + Duration::from_secs(1));
        assert_eq!(
            snapshot.failures[0].expires_in,
            TTL - Duration::from_secs(1)
        );
    }

    #[test]
    fn record_names_the_cause_request_and_reply_state() {
        let mut store = RetainedFailures::new(8, TTL);
        let now = Instant::now();
        let record = store.record(describe(3, 1), now);
        assert_eq!(record.slot, 3);
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
}
