//! The #551 contract the machine, Sandbox and backend read models share:
//! one `schema_version`, one set of capacity bounds, and one field-naming
//! convention (snake_case). Machine and Sandbox records carry the version;
//! an old record is read through the field fallbacks its type documents and
//! is reported by `doctor`, never rewritten by a read.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::time::Duration;

/// The record and capability schema this build writes (#551 B0-152).
pub const SCHEMA_VERSION: i64 = 1;

/// Stores one machine worker serves at once (#551 B0-151).
pub const PER_STORE_QUOTA: u32 = 4;

/// Calls one Store's daemon admits in flight before callers queue (#551 B0-151).
pub const IN_FLIGHT: u32 = 8;

/// Calls one Store's daemon queues beyond [`IN_FLIGHT`] before it refuses
/// `Capacity`; queue time counts inside [`READ_BUDGET_MS`] (#551 B0-151,
/// B8-160, B8-161).
pub const QUEUE: u32 = 16;

/// How long a read model waits for a local machine probe (#551 B8-159).
pub const PROBE_BUDGET_MS: u64 = 2_000;

/// How long a read model waits for a Store query, queue time included
/// (#551 B8-159, B8-161).
pub const READ_BUDGET_MS: u64 = 5_000;

/// The capacity contract a Sandbox records and its read model reports
/// (#551 B0-151). The same numbers are the refusal detail when the queue is
/// full, so a caller can size the next call from the answer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Capacity {
    /// Stores one machine worker serves at once.
    pub per_store_quota: u32,
    /// Calls one Store admits in flight.
    pub in_flight: u32,
    /// Calls one Store queues beyond its in-flight limit.
    pub queue: u32,
}

impl Capacity {
    /// What this build writes into a new Sandbox record.
    pub const CONTRACT: Self = Self {
        per_store_quota: PER_STORE_QUOTA,
        in_flight: IN_FLIGHT,
        queue: QUEUE,
    };

    /// The contract as a record or read-model value.
    pub fn to_sdk(self) -> SdkValue {
        SdkValue::map([
            ("schema_version", SdkValue::Int(SCHEMA_VERSION)),
            (
                "per_store_quota",
                SdkValue::Int(i64::from(self.per_store_quota)),
            ),
            ("in_flight_limit", SdkValue::Int(i64::from(self.in_flight))),
            ("queue_limit", SdkValue::Int(i64::from(self.queue))),
        ])
    }

    /// The same contract with what one Sandbox measures now: its in-flight
    /// count and how many callers wait.
    pub fn measured(self, in_flight: u32, queued: u32) -> SdkValue {
        let value = self.to_sdk();
        let value = crate::common::home::with_field(
            value,
            "in_flight",
            SdkValue::Int(i64::from(in_flight)),
        );
        crate::common::home::with_field(value, "queued", SdkValue::Int(i64::from(queued)))
    }

    /// A full queue's refusal (#551 B8-160): `Capacity` carrying the bound,
    /// the measured counts and how long the caller waited, which is inside
    /// the read budget it was given.
    pub fn refusal(self, store: &str, in_flight: u32, queued: u32, waited: Duration) -> SdkRefusal {
        SdkRefusal {
            code: "Capacity".to_owned(),
            message: format!(
                "Store {store}'s daemon already has {in_flight} calls in flight and {queued} queued (queue bound {}); nothing was attempted; retry after a call ends",
                self.queue
            ),
            detail: SdkValue::map([
                ("resource", SdkValue::text("store_calls")),
                ("store", SdkValue::text(store)),
                ("in_flight", SdkValue::Int(i64::from(in_flight))),
                ("queued", SdkValue::Int(i64::from(queued))),
                (
                    "queue_limit",
                    SdkValue::Int(i64::from(self.queue)),
                ),
                (
                    "in_flight_limit",
                    SdkValue::Int(i64::from(self.in_flight)),
                ),
                ("per_store_quota", SdkValue::Int(i64::from(self.per_store_quota))),
                (
                    "waited_ms",
                    SdkValue::Int(i64::try_from(waited.as_millis()).unwrap_or(i64::MAX)),
                ),
                ("retryable", SdkValue::Bool(true)),
            ]),
        }
    }
}

/// `value`'s `schema_version`, or this build's for a record written before
/// the field (#551 B0-152). Reading an old record is the migration path: its
/// missing fields are derived by its type's read fallbacks, never guessed
/// here, and nothing is written.
pub fn schema_version(value: &SdkValue) -> i64 {
    value.get_int("schema_version").unwrap_or(SCHEMA_VERSION)
}

/// `value` with `schema_version` when it has none: what a new record and an
/// explicit migration write, never a read.
pub fn with_schema_version(value: SdkValue) -> SdkValue {
    match value.get("schema_version") {
        Some(_) => value,
        None => {
            crate::common::home::with_field(value, "schema_version", SdkValue::Int(SCHEMA_VERSION))
        }
    }
}
