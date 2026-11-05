//! The #551 contract's record-level tests (B0-142, B0-151, B0-152).
//!
//! The call-surface acceptance lives in `layerfs-api`'s `tests/contract.rs`;
//! this file holds the record and capability layers those calls read: the
//! capacity bounds and their `Capacity` refusal, `schema_version` on a new
//! record and the explicit migration of an old one, and each backend's
//! typed capability with its schema version and naming convention.
use layerfs_bridge::control::SdkValue;
use layerfs_control::common::contract::{Capacity, SCHEMA_VERSION};
use layerfs_control::machine::record::MachineRecord;
use layerfs_control::sandbox::record::SandboxRecord;
use layerfs_sandbox::{
    Backend, IdleStop, IsolationModel, MountDriver, CAPABILITY_SCHEMA_VERSION,
};
use std::time::Duration;

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn record(fields: Vec<(&str, SdkValue)>) -> SdkValue {
    SdkValue::Map(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

/// B0-151 / acceptance 87: a full queue is `Capacity`, and its detail is the
/// contract a caller sizes the next call from.
#[test]
fn a_full_queue_is_capacity_with_the_contract_bounds() {
    let capacity = Capacity::CONTRACT;
    assert!(capacity.queue > 0 && capacity.in_flight > 0 && capacity.per_store_quota > 0);
    let refusal = capacity.refusal(
        "shop",
        capacity.in_flight,
        capacity.queue,
        Duration::from_millis(5),
    );
    assert_eq!(refusal.code, "Capacity");
    let detail = refusal.detail;
    assert_eq!(detail.get_text("resource"), Some("store_calls"));
    assert_eq!(detail.get_text("store"), Some("shop"));
    assert_eq!(detail.get_int("in_flight"), Some(i64::from(capacity.in_flight)));
    assert_eq!(detail.get_int("queued"), Some(i64::from(capacity.queue)));
    assert_eq!(detail.get_int("queue_limit"), Some(i64::from(capacity.queue)));
    assert_eq!(
        detail.get_int("in_flight_limit"),
        Some(i64::from(capacity.in_flight))
    );
    assert_eq!(
        detail.get_int("per_store_quota"),
        Some(i64::from(capacity.per_store_quota))
    );
    assert_eq!(detail.get_int("waited_ms"), Some(5));
    assert_eq!(detail.get_bool("retryable"), true);
    assert!(refusal.message.contains("queue bound"), "{}", refusal.message);

    // the recorded contract is the same shape a read model answers
    let value = capacity.measured(2, 1);
    assert_eq!(value.get_int("schema_version"), Some(SCHEMA_VERSION));
    assert_eq!(value.get_int("in_flight"), Some(2));
    assert_eq!(value.get_int("queued"), Some(1));
}

/// B0-152 / acceptance 88: a new record carries `schema_version`; an old one
/// is migrated only by an explicit write, never by a read.
#[test]
fn old_records_have_an_explicit_migration_path() {
    let old = MachineRecord::from_record(
        "dev",
        record(vec![("name", t("dev")), ("platform", t("lima"))]),
    );
    // the read path derives what it can and writes nothing
    assert_eq!(
        old.to_record().get_int("schema_version"),
        None,
        "a read must not invent a field in the record"
    );
    assert_eq!(old.isolation_class().map(|c| c.as_str()), Some("vm"));
    let migrated = old.with_contract();
    assert_eq!(
        migrated.to_record().get_int("schema_version"),
        Some(SCHEMA_VERSION)
    );
    assert_eq!(
        migrated.to_record().get_text("isolation_class"),
        Some("vm")
    );

    let old = SandboxRecord::from_record(
        "sb",
        record(vec![
            ("name", t("sb")),
            ("project", t("pg/app")),
            ("machine", t("docker")),
            ("platform", t("docker")),
        ]),
    );
    assert_eq!(old.to_record().get_int("schema_version"), None);
    assert_eq!(old.capacity(), Capacity::CONTRACT);
    assert_eq!(old.schema_version(), SCHEMA_VERSION);
    let migrated = old.with_contract();
    assert_eq!(
        migrated.to_record().get_int("schema_version"),
        Some(SCHEMA_VERSION)
    );
    assert!(migrated.to_record().get("capacity").is_some());
    assert_eq!(
        migrated.to_record().get_text("effective_isolation"),
        Some("container")
    );
}

/// B0-145 / B0-152: every backend declares a typed capability with one
/// schema version, one isolation class and one naming convention.
#[test]
fn every_backend_declares_a_typed_capability() {
    for backend in Backend::ALL {
        let capability = backend.capabilities();
        assert_eq!(
            capability.schema_version,
            CAPABILITY_SCHEMA_VERSION,
            "{}",
            backend.stored_name()
        );
        let class = capability.isolation.isolation_class();
        assert!(
            matches!(
                capability.mount_driver,
                MountDriver::Fuse
                    | MountDriver::LoopbackNfs
                    | MountDriver::SharedFs
                    | MountDriver::None
            ),
            "{}",
            backend.stored_name()
        );
        assert!(
            matches!(
                capability.idle_stop,
                IdleStop::Ours | IdleStop::Provider | IdleStop::None
            ),
            "{}",
            backend.stored_name()
        );
        // the capability's mount driver and isolation class are the values
        // the read model names, never a provider branch
        let expected = match capability.isolation {
            IsolationModel::Container => "container",
            IsolationModel::HostProcess => "process",
            IsolationModel::MicroVm => "vm",
            IsolationModel::RemoteCloud => "remote",
        };
        assert_eq!(class.as_str(), expected);
    }
}

/// B8-158: the bounded activity a Sandbox read model reports, written by the
/// command and the unmount that ran.
#[test]
fn sandbox_activity_records_running_and_recently_closed() {
    use layerfs_control::sandbox::activity;
    let dir = std::env::temp_dir().join(format!("lfs-activity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let token = activity::started(&dir, "/workspaces/a", "sleep 1").expect("a running entry");
    let read = activity::read(&dir);
    let running = match read.get("running") {
        Some(SdkValue::List(items)) => items.clone(),
        other => panic!("no running list: {other:?}"),
    };
    assert_eq!(running.len(), 1, "{read:?}");
    assert_eq!(running[0].get_text("path"), Some("/workspaces/a"));
    assert_eq!(running[0].get_text("command"), Some("sleep 1"));

    activity::finished(&dir, &token);
    let read = activity::read(&dir);
    assert!(matches!(read.get("running"), Some(SdkValue::List(items)) if items.is_empty()));

    activity::closed(&dir, Some("build"), "/workspaces/a", Some("work"));
    let read = activity::read(&dir);
    let closed = match read.get("recently_closed") {
        Some(SdkValue::List(items)) => items.clone(),
        other => panic!("no closed list: {other:?}"),
    };
    assert_eq!(closed.len(), 1, "{read:?}");
    assert_eq!(closed[0].get_text("name"), Some("build"));
    assert_eq!(closed[0].get_text("branch"), Some("work"));
    assert!(closed[0].get_int("closed").is_some());

    // the lists stay bounded
    for i in 0..(activity::CLOSED + 3) {
        activity::closed(&dir, None, &format!("/workspaces/{i}"), None);
    }
    let read = activity::read(&dir);
    assert!(
        matches!(read.get("recently_closed"), Some(SdkValue::List(items)) if items.len() == activity::CLOSED),
        "{read:?}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
