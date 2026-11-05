//! The backend capability a machine declares (#551 B0-145, B0-152): its
//! isolation class and kernel, its mount driver, its notes, how an idle
//! Sandbox stops and whether it snapshots. Built from the machine's
//! platform and the backend behind it, never from a provider-name branch in
//! a read model; a build without the machine feature has no adapters and
//! reports only the record's own class.
use super::record::MachineRecord;
use crate::common::contract;
use layerfs_bridge::control::SdkValue;
use layerfs_sandbox::{Backend, IdleStop, CAPABILITY_SCHEMA_VERSION};
use layerfs_machine::HostOs;

/// Machine `record`'s declared capability.
pub fn of(record: &MachineRecord) -> SdkValue {
    let class = record.isolation_class();
    let kernel = record.kernel_scope();
    let kind = record.platform().ok();
    let backend = kind.and_then(|kind| kind.backend(HostOs::current()));
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
        ("schema_version", SdkValue::Int(i64::from(CAPABILITY_SCHEMA_VERSION))),
        (
            "isolation",
            SdkValue::opt_text(class.map(|class| class.as_str())),
        ),
        (
            "kernel",
            SdkValue::opt_text(kernel.map(|scope| scope.as_str())),
        ),
        ("strength", SdkValue::text(strength)),
        (
            "provider",
            SdkValue::opt_text(kind.map(|kind| kind.name())),
        ),
        (
            "backend",
            SdkValue::opt_text(backend.map(|backend| backend.stored_name())),
        ),
        ("mount_driver", SdkValue::text(mount_driver)),
        (
            "idle_stop",
            SdkValue::text(idle_stop.as_str()),
        ),
        (
            "snapshot",
            match snapshot {
                Some(snapshot) => SdkValue::map([
                    ("max_bytes", SdkValue::Int(i64::try_from(snapshot.max_bytes).unwrap_or(i64::MAX))),
                    ("ttl_secs", SdkValue::Int(i64::try_from(snapshot.ttl_secs).unwrap_or(i64::MAX))),
                    ("beta", SdkValue::Bool(snapshot.beta)),
                ]),
                None => SdkValue::Null,
            },
        ),
        ("notes", SdkValue::text(notes)),
        ("record_schema_version", SdkValue::Int(contract::SCHEMA_VERSION)),
    ])
}
