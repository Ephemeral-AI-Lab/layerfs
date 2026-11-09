//! The daemon's control-capacity read model (#540).
//!
//! One typed shape is the contract between the daemon that publishes it, the
//! operator read that forwards it, and the doctor that explains it: a
//! [`CapacityModel`] with one [`AdmissionModel`] (vacant and in-use control
//! slots by class) and one [`RetainedModel`] (the bounded store of connection
//! failures that used to retain a slot). It is a plain value object: parse a
//! daemon answer once, then read typed fields instead of string keys.
use crate::control::SdkValue;

/// The daemon backend method that returns this read model.
pub const OPERATION: &str = "backend.daemon.connection_failures";
/// The daemon backend method that closes one named worker's socket and
/// records `Reclaimed` (#547 slice 5).
pub const RECLAIM: &str = "backend.daemon.reclaim";

/// Vacant and in-use control slots by admission class.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AdmissionModel {
    /// Pool slots that no connection holds.
    pub pool_vacant: i64,
    /// Pool slots that an admitted connection holds.
    pub pool_running: i64,
    /// Reserved installer slots that no connection holds.
    pub reserved_vacant: i64,
    /// Reserved installer slots that an admitted connection holds.
    pub reserved_running: i64,
}

impl AdmissionModel {
    /// The model in `value`, or all zero when the record does not carry one.
    pub fn parse(value: &SdkValue) -> Self {
        let admission = value.get("admission").unwrap_or(&SdkValue::Null);
        Self {
            pool_vacant: admission.get_int("pool_vacant").unwrap_or(0),
            pool_running: admission.get_int("pool_running").unwrap_or(0),
            reserved_vacant: admission.get_int("reserved_vacant").unwrap_or(0),
            reserved_running: admission.get_int("reserved_running").unwrap_or(0),
        }
    }

    /// The model as the read model's `admission` record.
    pub fn to_sdk(&self) -> SdkValue {
        SdkValue::map([
            ("pool_vacant", SdkValue::Int(self.pool_vacant)),
            ("pool_running", SdkValue::Int(self.pool_running)),
            ("reserved_vacant", SdkValue::Int(self.reserved_vacant)),
            ("reserved_running", SdkValue::Int(self.reserved_running)),
        ])
    }

    /// Whether the pool has no vacant slot while it is in use at all.
    pub fn exhausted(&self) -> bool {
        self.pool_vacant == 0 && self.pool_running > 0
    }
}

/// One connection failure that used to retain a control slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedFailureModel {
    /// Slot the failed worker occupied; it is already free for a new connection.
    pub slot: i64,
    /// `pool` or `reserved`: the class of the slot that was released.
    pub class: String,
    /// Admission generation of the released worker.
    pub generation: i64,
    /// The worker that failed: daemon instance, pid and thread.
    pub owner: String,
    /// Bytes of a request that failed decoding, when one was received.
    pub received_bytes: Option<i64>,
    /// `control` or `install` when a request was decoded.
    pub request: Option<String>,
    /// Whether the original reply was sent before the failure.
    pub reply_sent: bool,
    /// Bounded textual deciding cause.
    pub cause: String,
    /// Original socket fence failure, when one was observed.
    pub fence_error: Option<String>,
    /// Distinct failed diagnostic transfer, when one happened.
    pub diagnostic_error: Option<String>,
    /// Age at the observation that returned this record, in milliseconds.
    pub age_ms: i64,
    /// Remaining lifetime at the observation that returned this record, in
    /// milliseconds.
    pub expires_in_ms: i64,
}

impl RetainedFailureModel {
    /// The record in `value`, or `None` when it is not a failure record.
    pub fn parse(value: &SdkValue) -> Option<Self> {
        Some(Self {
            slot: value.get_int("slot")?,
            class: value.get_text("class")?.to_owned(),
            generation: value.get_int("generation")?,
            owner: value.get_text("owner")?.to_owned(),
            received_bytes: value.get_int("received_bytes"),
            request: value.get_text("request").map(str::to_owned),
            reply_sent: value.get_bool("reply_sent"),
            cause: value.get_text("cause")?.to_owned(),
            fence_error: value.get_text("fence_error").map(str::to_owned),
            diagnostic_error: value.get_text("diagnostic_error").map(str::to_owned),
            age_ms: value.get_int("age_ms")?,
            expires_in_ms: value.get_int("expires_in_ms")?,
        })
    }

    /// The record as the read model's `retained.failures` item.
    pub fn to_sdk(&self) -> SdkValue {
        SdkValue::map([
            ("slot", SdkValue::Int(self.slot)),
            ("class", SdkValue::text(&self.class)),
            ("generation", SdkValue::Int(self.generation)),
            ("owner", SdkValue::text(&self.owner)),
            (
                "received_bytes",
                self.received_bytes.map_or(SdkValue::Null, SdkValue::Int),
            ),
            ("request", SdkValue::opt_text(self.request.clone())),
            ("reply_sent", SdkValue::Bool(self.reply_sent)),
            ("cause", SdkValue::text(&self.cause)),
            ("fence_error", SdkValue::opt_text(self.fence_error.clone())),
            (
                "diagnostic_error",
                SdkValue::opt_text(self.diagnostic_error.clone()),
            ),
            ("age_ms", SdkValue::Int(self.age_ms)),
            ("expires_in_ms", SdkValue::Int(self.expires_in_ms)),
        ])
    }
}

/// The bounded store of connection failures that used to retain a slot.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RetainedModel {
    /// Most records the store keeps at once.
    pub limit: i64,
    /// How long a record stays operator-visible, in milliseconds.
    pub ttl_ms: i64,
    /// Visible records.
    pub count: i64,
    /// Records evicted by the limit or the TTL since startup.
    pub dropped: i64,
    /// Total records ever recorded since startup.
    pub recorded: i64,
    /// Visible records, oldest first.
    pub failures: Vec<RetainedFailureModel>,
}

impl RetainedModel {
    /// The model in `value`, or all zero when the record does not carry one.
    pub fn parse(value: &SdkValue) -> Self {
        let retained = value.get("retained").unwrap_or(&SdkValue::Null);
        let failures = match retained.get("failures") {
            Some(SdkValue::List(items)) => items
                .iter()
                .filter_map(RetainedFailureModel::parse)
                .collect(),
            _ => Vec::new(),
        };
        Self {
            limit: retained.get_int("limit").unwrap_or(0),
            ttl_ms: retained.get_int("ttl_ms").unwrap_or(0),
            count: retained.get_int("count").unwrap_or(0),
            dropped: retained.get_int("dropped").unwrap_or(0),
            recorded: retained.get_int("recorded").unwrap_or(0),
            failures,
        }
    }

    /// The model as the read model's `retained` record.
    pub fn to_sdk(&self) -> SdkValue {
        SdkValue::map([
            ("limit", SdkValue::Int(self.limit)),
            ("ttl_ms", SdkValue::Int(self.ttl_ms)),
            ("count", SdkValue::Int(self.count)),
            ("dropped", SdkValue::Int(self.dropped)),
            ("recorded", SdkValue::Int(self.recorded)),
            (
                "failures",
                SdkValue::List(
                    self.failures
                        .iter()
                        .map(RetainedFailureModel::to_sdk)
                        .collect(),
                ),
            ),
        ])
    }
}

/// The daemon's control-capacity read model (#540).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CapacityModel {
    /// Vacant and in-use control slots by class.
    pub admission: AdmissionModel,
    /// The bounded retained-failure store.
    pub retained: RetainedModel,
}

impl CapacityModel {
    /// Parse one `backend.daemon.connection_failures` answer. A field the
    /// record does not carry reads as zero, so a newer daemon stays legible.
    pub fn parse(value: &SdkValue) -> Self {
        Self {
            admission: AdmissionModel::parse(value),
            retained: RetainedModel::parse(value),
        }
    }

    /// The answer a daemon publishes for this model.
    pub fn to_sdk(&self) -> SdkValue {
        SdkValue::map([
            ("admission", self.admission.to_sdk()),
            ("retained", self.retained.to_sdk()),
        ])
    }
}
