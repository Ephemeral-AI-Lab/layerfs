//! One machine's provider and worker, observed without lifecycle actions.
use super::target::{Budget, Checks};
use std::path::Path;

#[cfg(not(all(unix, feature = "machine")))]
pub(super) fn diagnose(_home: &Path, _name: &str, _budget: &Budget, checks: &mut Checks) {
    checks.unknown(
        "machine.platform",
        "machine adapters are not built into this binary",
    );
}

#[cfg(all(unix, feature = "machine"))]
pub(super) fn diagnose(home: &Path, name: &str, budget: &Budget, checks: &mut Checks) {
    use layerfs_home::home;
    use layerfs_home::op::Standing;
    use layerfs_machine::{machine_provider_spec, platform, record::MachineRecord};
    use std::time::{Duration, Instant};

    let record = match MachineRecord::of(home, name) {
        Ok(record) => record,
        Err(error) => {
            checks.fail("machine.record", error.message);
            return;
        }
    };
    if record.int("v") != Some(home::RECORD_VERSION)
        || record.text("name") != Some(name)
        || !valid_saved_fields(&record)
        || (record.field("op").is_some() && record.op().is_none())
    {
        checks.fail(
            "machine.record",
            "machine record identity, state or numeric configuration is invalid",
        );
        return;
    }
    let kind = match record.platform() {
        Ok(kind) => kind,
        Err(error) => {
            checks.fail("machine.platform", error.message);
            return;
        }
    };
    checks.pass(
        "machine.record",
        "machine record identity, platform, state and numeric fields are valid",
    );
    checks.evidence(
        "machine.record",
        layerfs_bridge::control::SdkValue::map([
            (
                "platform",
                layerfs_bridge::control::SdkValue::text(kind.name()),
            ),
            (
                "recorded_state",
                layerfs_bridge::control::SdkValue::opt_text(
                    record.recorded_state().map(str::to_owned),
                ),
            ),
        ]),
    );
    if let Some(op) = record.op() {
        let standing = op.standing();
        match standing {
            Standing::Running => checks.warning(
                "machine.operation",
                "a live operation owns this machine; no provider or worker probes were made",
            ),
            Standing::Unresolved => checks.fail(
                "machine.operation",
                "an unresolved operation owns this machine; no provider or worker probes were made",
            ),
            Standing::Stale => checks.warning(
                "machine.operation",
                "the recorded operation ended with a certain outcome; record was left unchanged",
            ),
        }
        checks.evidence("machine.operation", operation_evidence(&op, standing));
        if standing != Standing::Stale {
            checks.blocked(
                "machine.provider",
                "provider observation blocked by the machine operation",
                "machine.operation",
            );
            block_worker(checks, "machine.operation");
            return;
        }
    } else {
        checks.pass(
            "machine.operation",
            "no machine lifecycle operation is recorded",
        );
    }
    if budget.expired() {
        checks.unknown(
            "machine.provider",
            "overall deadline expired before provider observation",
        );
        block_worker(checks, "machine.provider");
        return;
    }
    let home_id = match home::opened(home) {
        Ok(file) => file.id,
        Err(error) => {
            checks.fail("machine.home", error.message);
            return;
        }
    };
    let spec = match machine_provider_spec(home, &home_id, &record) {
        Ok(spec) => spec,
        Err(error) => {
            checks.fail("machine.configuration", error.message);
            return;
        }
    };
    let started = Instant::now();
    let provider =
        match platform::provider_within(&spec, started + budget.limit(Duration::from_secs(10))) {
            Ok(status) => status,
            Err(error) => {
                if budget.expired() || error.code == platform::Code::Unknown {
                    checks.unknown("machine.provider", error.message);
                } else {
                    checks.fail("machine.provider", error.message);
                }
                checks.duration("machine.provider", started.elapsed());
                block_worker(checks, "machine.provider");
                return;
            }
        };
    let state = provider.state.as_str();
    match provider.state {
        platform::State::Running => checks.pass("machine.provider", "provider runtime is running"),
        platform::State::Stopped => checks.warning(
            "machine.provider",
            "provider runtime is stopped; it was not started",
        ),
        platform::State::Missing => checks.fail(
            "machine.provider",
            "provider holds no runtime with the recorded machine identity",
        ),
        platform::State::Failed => {
            checks.fail("machine.provider", "provider reports the runtime failed")
        }
    }
    checks.duration("machine.provider", started.elapsed());
    if kind.tool().is_some() {
        if let Some(version) = &provider.tool_version {
            checks.pass(
                "machine.provider.tool_version",
                "provider tool version was observed",
            );
            checks.evidence(
                "machine.provider.tool_version",
                layerfs_bridge::control::SdkValue::text(version),
            );
        } else {
            checks.unknown(
                "machine.provider.tool_version",
                "provider tool version could not be observed",
            );
        }
    }
    checks.evidence(
        "machine.provider",
        layerfs_bridge::control::SdkValue::map([
            ("state", layerfs_bridge::control::SdkValue::text(state)),
            (
                "runtime",
                layerfs_bridge::control::SdkValue::text(&spec.vm_name),
            ),
            (
                "tool_version",
                layerfs_bridge::control::SdkValue::opt_text(provider.tool_version),
            ),
        ]),
    );
    if let Some(recorded) = record.recorded_state() {
        if recorded != state {
            checks.warning("machine.state", format!("recorded state {recorded} differs from observed provider state {state}; record was not refreshed"));
        } else {
            checks.pass(
                "machine.state",
                "recorded and observed provider states agree",
            );
        }
    } else {
        checks.skip("machine.state", "no saved runtime state to compare");
    }
    if kind.route(platform::HostOs::current()) == platform::Route::InProcess {
        checks.skip(
            "machine.worker",
            "native execution is local and has no worker",
        );
        checks.skip(
            "machine.capacity",
            "native machine has no separate guest data disk",
        );
        return;
    }
    if provider.state != platform::State::Running {
        for id in [
            "machine.worker.endpoint",
            "machine.worker.auth",
            "machine.worker.compatibility",
            "machine.worker.ping",
            "machine.capacity",
        ] {
            checks.skip(
                id,
                "machine runtime is not running; no worker probe was made",
            );
        }
        return;
    }
    worker(home, name, budget, checks);
}

#[cfg(feature = "machine")]
fn valid_saved_fields(record: &layerfs_machine::record::MachineRecord) -> bool {
    if record.field("state").is_some()
        && !matches!(
            record.recorded_state(),
            Some("running" | "stopped" | "failed")
        )
    {
        return false;
    }
    for field in ["cpus", "memory", "disk"] {
        if record.field(field).is_some()
            && !record
                .int(field)
                .is_some_and(|value| value > 0 && (field != "cpus" || value <= i64::from(u32::MAX)))
        {
            return false;
        }
    }
    if let Some(lasting) = record.field("lasting") {
        if lasting.get("port").is_some()
            && !lasting
                .get_int("port")
                .is_some_and(|port| (1..=65535).contains(&port))
        {
            return false;
        }
    }
    true
}

#[cfg(feature = "machine")]
fn block_worker(checks: &mut Checks, dependency: &str) {
    for id in [
        "machine.worker.endpoint",
        "machine.worker.auth",
        "machine.worker.compatibility",
        "machine.worker.ping",
        "machine.capacity",
    ] {
        checks.blocked(
            id,
            "machine dependency could not be established",
            dependency,
        );
    }
}

#[cfg(feature = "machine")]
fn worker(home: &Path, name: &str, budget: &Budget, checks: &mut Checks) {
    use layerfs_machine::worker::{diagnostic_connect, Endpoint};
    use std::time::{Duration, Instant};
    let endpoint = match layerfs_home::secrets::worker_secret(home, name) {
        Ok(Some(value))
            if !value.secret.is_empty()
                && value.address.ip().is_loopback()
                && value.address.port() != 0 =>
        {
            Endpoint {
                address: value.address,
                secret: value.secret,
            }
        }
        Ok(_) => {
            checks.fail(
                "machine.worker.endpoint",
                "running machine has no valid loopback worker endpoint and secret",
            );
            block_after_endpoint(checks);
            return;
        }
        Err(_) => {
            checks.fail(
                "machine.worker.endpoint",
                "worker endpoint record is unreadable or malformed",
            );
            block_after_endpoint(checks);
            return;
        }
    };
    checks.pass(
        "machine.worker.endpoint",
        "saved worker endpoint and secret are present",
    );
    let started = Instant::now();
    let worker = match diagnostic_connect(&endpoint, started + budget.limit(Duration::from_secs(5)))
    {
        Ok(worker) => worker,
        Err(error) => {
            if budget.expired() || error.code == "Timeout" || error.code == "Unavailable" {
                checks.unknown("machine.worker.auth", "worker connection/authentication was unavailable; no valid hello was established");
            } else {
                checks.fail(
                    "machine.worker.auth",
                    format!("worker authentication or hello failed ({})", error.code),
                );
            }
            checks.duration("machine.worker.auth", started.elapsed());
            checks.blocked(
                "machine.worker.compatibility",
                "worker hello unavailable",
                "machine.worker.auth",
            );
            checks.blocked(
                "machine.worker.ping",
                "authenticated worker unavailable",
                "machine.worker.auth",
            );
            checks.blocked(
                "machine.capacity",
                "authenticated worker unavailable",
                "machine.worker.auth",
            );
            return;
        }
    };
    checks.pass(
        "machine.worker.auth",
        "worker accepted authentication and returned a ready hello",
    );
    checks.duration("machine.worker.auth", started.elapsed());
    checks.evidence("machine.worker.auth", worker.hello.to_sdk());
    if let Some(expected) = worker.hello.expected() {
        checks.fail(
            "machine.worker.compatibility",
            format!(
                "worker differs from this build: {}; expected {}",
                worker.hello.describe(),
                expected.describe()
            ),
        );
        checks.blocked(
            "machine.worker.ping",
            "incompatible worker was sent no request",
            "machine.worker.compatibility",
        );
        checks.blocked(
            "machine.capacity",
            "incompatible worker was sent no request",
            "machine.worker.compatibility",
        );
        return;
    }
    checks.pass(
        "machine.worker.compatibility",
        "worker protocol and applicable sealed bundle match",
    );
    let started = Instant::now();
    match worker.ping() {
        Ok(value)
            if value.get_text("kind") == Some("worker")
                && value.get_int("pid").is_some_and(|pid| pid > 0)
                && value.get_text("version") == Some(env!("CARGO_PKG_VERSION")) =>
        {
            checks.pass(
                "machine.worker.ping",
                "read-only worker.ping returned process and version identity",
            );
            if let Some(bytes) = value.get_int("free_disk").filter(|bytes| *bytes >= 0) {
                checks.pass(
                    "machine.capacity",
                    "guest data-disk available bytes observed by the worker",
                );
                checks.evidence(
                    "machine.capacity",
                    layerfs_bridge::control::SdkValue::map([(
                        "available_bytes",
                        layerfs_bridge::control::SdkValue::Int(bytes),
                    )]),
                );
            } else {
                checks.unknown(
                    "machine.capacity",
                    "worker could not observe guest data-disk available bytes",
                );
            }
            checks.evidence("machine.worker.ping", value);
        }
        Ok(_) => {
            checks.fail(
                "machine.worker.ping",
                "worker.ping returned an invalid process or mismatching version identity",
            );
            checks.blocked(
                "machine.capacity",
                "worker identity was not established",
                "machine.worker.ping",
            );
        }
        Err(error)
            if budget.expired() || matches!(error.code.as_str(), "Timeout" | "Unavailable") =>
        {
            checks.unknown(
                "machine.worker.ping",
                "worker.ping did not complete within its connection budget",
            );
            checks.blocked(
                "machine.capacity",
                "worker ping unavailable",
                "machine.worker.ping",
            );
        }
        Err(error) => {
            checks.fail(
                "machine.worker.ping",
                format!("worker.ping failed ({})", error.code),
            );
            checks.blocked(
                "machine.capacity",
                "worker ping failed",
                "machine.worker.ping",
            );
        }
    }
    checks.duration("machine.worker.ping", started.elapsed());
}

#[cfg(feature = "machine")]
fn block_after_endpoint(checks: &mut Checks) {
    for id in [
        "machine.worker.auth",
        "machine.worker.compatibility",
        "machine.worker.ping",
        "machine.capacity",
    ] {
        checks.blocked(
            id,
            "worker endpoint is unavailable",
            "machine.worker.endpoint",
        );
    }
}

#[cfg(feature = "machine")]
fn operation_evidence(
    op: &layerfs_home::op::Op,
    standing: layerfs_home::op::Standing,
) -> layerfs_bridge::control::SdkValue {
    use layerfs_bridge::control::SdkValue;
    use layerfs_home::op::Standing;
    let id = op.id();
    let valid_id = id.len() == 19
        && id
            .strip_prefix("op-")
            .is_some_and(|suffix| suffix.bytes().all(|byte| byte.is_ascii_hexdigit()));
    SdkValue::map([
        (
            "standing",
            SdkValue::text(match standing {
                Standing::Running => "running",
                Standing::Unresolved => "unresolved",
                Standing::Stale => "stale",
            }),
        ),
        ("uncertain", SdkValue::Bool(op.uncertain())),
        (
            "id",
            if valid_id {
                SdkValue::text(id)
            } else {
                SdkValue::Null
            },
        ),
        (
            "name",
            if matches!(
                op.name(),
                layerfs_machine::record::CREATE | layerfs_machine::record::REMOVE
            ) {
                SdkValue::text(op.name())
            } else {
                SdkValue::Null
            },
        ),
        (
            "phase",
            if matches!(op.phase(), "create" | "delete") {
                SdkValue::text(op.phase())
            } else {
                SdkValue::Null
            },
        ),
    ])
}
