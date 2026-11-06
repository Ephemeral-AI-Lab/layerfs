//! The backend capability a machine declares (#551 B0-145, B0-152): its
//! isolation class and kernel, its mount driver, its notes, how an idle
//! Sandbox stops, whether it can snapshot and whether it reaches a Store's
//! file directly. Built from the machine's record — its platform and the
//! backend the record names — never from a provider-name branch in a read
//! model; a build without the machine feature reports the contract fields
//! without the adapters.
//!
//! `direct_store_access` is the one owner of "can a sqlite Store live on
//! this machine" (#552): it is the typed `ProviderCapabilities` field,
//! written into the record when the machine is made, exposed here and read
//! by placement. A record written before the field has none, and placement
//! refuses rather than defaulting.
use super::record::MachineRecord;
use crate::common::contract;
use layerfs_bridge::control::SdkValue;
use layerfs_machine::HostOs;
use layerfs_sandbox::{Backend, IdleStop, CAPABILITY_SCHEMA_VERSION};

/// Machine `record`'s declared capability.
pub fn of(record: &MachineRecord) -> SdkValue {
    let class = record.isolation_class();
    let kernel = record.kernel_scope();
    let kind = record.platform().ok();
    // The backend the record names is the fact; parse it by its stored name
    // so the capability below is the same on every host. Only a record
    // written before `backend` falls back to this host's platform row, and
    // only for display: placement reads the record's own
    // `direct_store_access` and never this fallback.
    let backend = match record.backend_name() {
        Some(name) => Backend::parse(name),
        None => kind.and_then(|kind| kind.backend(HostOs::current())),
    };
    let declared = backend.map(Backend::capabilities);
    let strength = class.map(|class| class.strength()).unwrap_or("unknown");
    let notes = kind.map(|kind| kind.profile().description).unwrap_or("");
    let mount_driver = declared
        .as_ref()
        .map(|cap| cap.mount_driver.as_str())
        .unwrap_or("none");
    let idle_stop = declared
        .as_ref()
        .map(|cap| cap.idle_stop)
        .unwrap_or(IdleStop::None);
    let snapshot = declared.as_ref().and_then(|cap| cap.snapshot);
    SdkValue::map([
        (
            "schema_version",
            SdkValue::Int(i64::from(CAPABILITY_SCHEMA_VERSION)),
        ),
        (
            "isolation",
            SdkValue::opt_text(class.map(|class| class.as_str())),
        ),
        (
            "kernel",
            SdkValue::opt_text(kernel.map(|scope| scope.as_str())),
        ),
        ("strength", SdkValue::text(strength)),
        ("provider", SdkValue::opt_text(kind.map(|kind| kind.name()))),
        (
            "backend",
            SdkValue::opt_text(
                record
                    .backend_name()
                    .or_else(|| backend.map(|backend| backend.stored_name())),
            ),
        ),
        ("mount_driver", SdkValue::text(mount_driver)),
        ("idle_stop", SdkValue::text(idle_stop.as_str())),
        (
            "direct_store_access",
            match record.direct_store_access() {
                Some(direct) => SdkValue::Bool(direct),
                None => SdkValue::Null,
            },
        ),
        (
            "snapshot",
            match snapshot {
                Some(snapshot) => SdkValue::map([
                    (
                        "max_bytes",
                        SdkValue::Int(i64::try_from(snapshot.max_bytes).unwrap_or(i64::MAX)),
                    ),
                    (
                        "ttl_secs",
                        SdkValue::Int(i64::try_from(snapshot.ttl_secs).unwrap_or(i64::MAX)),
                    ),
                    ("beta", SdkValue::Bool(snapshot.beta)),
                ]),
                None => SdkValue::Null,
            },
        ),
        ("notes", SdkValue::text(notes)),
        (
            "record_schema_version",
            SdkValue::Int(contract::SCHEMA_VERSION),
        ),
    ])
}
