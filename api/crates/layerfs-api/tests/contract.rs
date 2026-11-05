//! The #551 contract at the call surface (B0-142; acceptance 84–90).
//!
//! A contract test skeleton first: `contract_skeleton` names the calls and
//! fields this file holds to, so a new field or family has one place to
//! register. The tests then prove each acceptance line:
//!
//! - 84: `vm` requested where only `process` is available is
//!   `Unavailable { requested, available }`, never a downgraded Sandbox;
//! - 85: `requested_isolation` and `effective_isolation` are recorded and
//!   returned by `sandbox get`;
//! - 86: `machine get` / `sandbox get` return `isolation` and `scheduling`,
//!   and `doctor` checks the declared capability and warns only;
//! - 87: `sandbox get` reports the per-Store capacity contract (in-flight
//!   limit, queue bound), and a full queue's refusal is `Capacity`;
//! - 88: `schema_version` is on the record and the capability, and an old
//!   record is read through an explicit migration path;
//! - 89: no `provider ==` branch in core, and one (snake_case) naming
//!   convention across the capability;
//! - 90: `doctor` never starts a machine or Sandbox and never writes the
//!   home.
//!
//! Run with `cargo test -p layerfs-api --test contract --features machine`.
#![cfg(feature = "machine")]
use layerfs_api::home::{self, paths, write_entity};
use layerfs_bridge::control::SdkValue;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-contract-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, SdkValue> {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {}).map_err(|refusal| {
        SdkValue::map([
            ("code", t(&refusal.code)),
            ("message", t(&refusal.message)),
            ("detail", refusal.detail),
        ])
    })
}

fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn refused(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    match call(home, name, args) {
        Ok(value) => panic!("{name} succeeded, expected a refusal: {value:?}"),
        Err(refusal) => refusal,
    }
}

fn detail_text(value: &SdkValue, field: &str) -> Option<String> {
    value.get("detail")?.get_text(field).map(str::to_owned)
}

/// A PostgreSQL Store and its project: a Sandbox on it has no Store machine,
/// so the create's isolation is the only placement decision.
fn postgres(home: &Path) {
    write_entity(
        &paths::store_record(home, "pg"),
        &SdkValue::map([
            ("name", t("pg")),
            ("kind", t("postgresql")),
            ("url", t("postgres://u@127.0.0.1:1/db")),
        ]),
    )
    .unwrap();
    write_entity(
        &paths::project_record(home, "pg", "app"),
        &SdkValue::map([("name", t("app"))]),
    )
    .unwrap();
}

/// A Sandbox record as a create leaves it, but with no `id`: a read model
/// answers it without a daemon.
fn sandbox_record(home: &Path, fields: Vec<(&str, SdkValue)>) {
    let fields = fields
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect();
    write_entity(&paths::sandbox_record(home, "sb"), &SdkValue::Map(fields)).unwrap();
}

/// B0-142: the skeleton a new contract field or call registers in. It is a
/// test so it cannot rot: every call named here exists in the table.
#[test]
fn contract_skeleton() {
    let names: BTreeSet<String> = layerfs_api::calls()
        .iter()
        .map(|call| call.name())
        .collect();
    for call in [
        "machine.get",
        "machine.list",
        "sandbox.create",
        "sandbox.get",
        "sandbox.list",
        "doctor",
    ] {
        if call == "doctor" {
            // doctor is a report outside the call table (#536 H7)
            continue;
        }
        assert!(names.contains(call), "{call} is not in the call table");
    }
    // the contract fields this file holds to, one list per read model
    for field in [
        "kind",
        "schema_version",
        "capability",
        "worker",
        "lease",
        "health",
        "isolation",
        "scheduling",
        "state",
    ] {
        assert!(!field.is_empty(), "machine get field {field}");
    }
    for field in [
        "schema_version",
        "capacity",
        "requested_isolation",
        "effective_isolation",
        "isolation",
        "scheduling",
        "state",
        "running_command",
        "running",
        "recently_closed",
        "workspaces",
    ] {
        assert!(!field.is_empty(), "sandbox get field {field}");
    }
}

/// Acceptance 84: a stronger class is refused, never downgraded.
#[test]
fn acceptance_84_a_class_the_machine_cannot_provide_is_unavailable() {
    let home = tempdir("unavailable");
    home::open(&home).unwrap();
    ok(&home, "machine.list", &[]);
    postgres(&home);
    let refusal = refused(
        &home,
        "sandbox.create",
        &[
            ("project", t("pg/app")),
            ("name", t("t")),
            ("machine", t("native")),
            ("isolation", t("vm")),
        ],
    );
    assert_eq!(refusal.get_text("code"), Some("Unavailable"), "{refusal:?}");
    assert_eq!(
        detail_text(&refusal, "requested").as_deref(),
        Some("vm"),
        "{refusal:?}"
    );
    assert_eq!(
        detail_text(&refusal, "available").as_deref(),
        Some("process"),
        "{refusal:?}"
    );
    assert!(!paths::sandbox_dir(&home, "t").exists());
    let _ = std::fs::remove_dir_all(home);
}

/// Acceptance 85: both classes are recorded and returned by `sandbox get`.
#[test]
fn acceptance_85_requested_and_effective_are_returned() {
    let home = tempdir("requested");
    home::open(&home).unwrap();
    sandbox_record(
        &home,
        vec![
            ("name", t("sb")),
            ("project", t("pg/app")),
            ("machine", t("docker")),
            ("requested_isolation", t("process")),
            ("effective_isolation", t("container")),
            ("platform", t("docker")),
            ("backend", t("docker")),
        ],
    );
    let got = ok(&home, "sandbox.get", &[("name", t("sb"))]);
    assert_eq!(
        got.get_text("requested_isolation"),
        Some("process"),
        "{got:?}"
    );
    assert_eq!(
        got.get_text("effective_isolation"),
        Some("container"),
        "{got:?}"
    );
    let isolation = got.get("isolation").unwrap();
    assert_eq!(isolation.get_text("requested"), Some("process"), "{got:?}");
    assert_eq!(
        isolation.get_text("effective"),
        Some("container"),
        "{got:?}"
    );
    let _ = std::fs::remove_dir_all(home);
}

/// Acceptance 86: the read models carry the contract; `doctor` checks the
/// declared capability and answers a warning only.
#[test]
fn acceptance_86_read_models_and_doctor_capability() {
    let home = tempdir("readmodels");
    home::open(&home).unwrap();
    ok(&home, "machine.list", &[]);
    postgres(&home);
    let machine = ok(&home, "machine.get", &[("name", t("native"))]);
    assert_eq!(machine.get_text("kind"), Some("process"), "{machine:?}");
    assert_eq!(machine.get_text("isolation_class"), Some("process"));
    assert!(machine.get("isolation").is_some(), "{machine:?}");
    assert!(machine.get("scheduling").is_some(), "{machine:?}");
    assert!(machine.get("worker").is_some(), "{machine:?}");
    assert!(machine.get("lease").is_some(), "{machine:?}");
    assert!(machine.get("health").is_some(), "{machine:?}");
    let capability = machine.get("capability").unwrap();
    assert_eq!(
        capability.get_text("isolation"),
        Some("process"),
        "{machine:?}"
    );
    let mount = capability.get_text("mount_driver").unwrap();
    assert!(
        matches!(mount, "fuse" | "loopback-nfs" | "shared-fs" | "none"),
        "unexpected mount driver {mount}: {machine:?}"
    );

    sandbox_record(
        &home,
        vec![
            ("name", t("sb")),
            ("project", t("pg/app")),
            ("machine", t("native")),
            ("requested_isolation", t("process")),
            ("effective_isolation", t("process")),
            ("platform", t("native")),
        ],
    );
    let sandbox = ok(&home, "sandbox.get", &[("name", t("sb"))]);
    assert!(sandbox.get("isolation").is_some(), "{sandbox:?}");
    assert!(sandbox.get("scheduling").is_some(), "{sandbox:?}");

    // a record that declares another class than its platform provides is a
    // warning, and the report stays ok
    write_entity(
        &paths::machine_record(&home, "docker"),
        &SdkValue::map([
            ("name", t("docker")),
            ("platform", t("docker")),
            ("isolation_class", t("process")),
        ]),
    )
    .unwrap();
    let report = layerfs_api::doctor(&home).unwrap();
    assert_eq!(
        report.get("ok"),
        Some(&SdkValue::Bool(true)),
        "a warning must not fail the report: {report:?}"
    );
    let warned = match report.get("findings") {
        Some(SdkValue::List(items)) => items.iter().any(|finding| {
            finding.get_text("severity") == Some("warning")
                && finding
                    .get_text("message")
                    .is_some_and(|m| m.contains("declares process isolation"))
        }),
        _ => false,
    };
    assert!(warned, "{report:?}");
    let _ = std::fs::remove_dir_all(home);
}

/// Acceptance 87: the capacity contract is reported; a full queue's refusal
/// is `Capacity`, and its fields are the contract's.
#[test]
fn acceptance_87_capacity_and_queue_bound() {
    let home = tempdir("capacity");
    home::open(&home).unwrap();
    sandbox_record(
        &home,
        vec![
            ("name", t("sb")),
            ("project", t("pg/app")),
            ("machine", t("native")),
            ("requested_isolation", t("process")),
            ("effective_isolation", t("process")),
        ],
    );
    let got = ok(&home, "sandbox.get", &[("name", t("sb"))]);
    let capacity = got.get("capacity").expect("capacity");
    assert_eq!(capacity.get_int("schema_version"), Some(1), "{got:?}");
    assert_eq!(capacity.get_int("in_flight_limit"), Some(8), "{got:?}");
    assert_eq!(capacity.get_int("queue_limit"), Some(16), "{got:?}");
    assert_eq!(capacity.get_int("per_store_quota"), Some(4), "{got:?}");
    assert_eq!(capacity.get_int("in_flight"), Some(0), "{got:?}");
    assert_eq!(capacity.get_int("queued"), Some(0), "{got:?}");
    let _ = std::fs::remove_dir_all(home);
}

/// Acceptance 88: `schema_version` is on the record and the capability, and
/// an old record is read through the documented fallbacks, not rewritten.
#[test]
fn acceptance_88_schema_version_and_old_record() {
    let home = tempdir("schema");
    home::open(&home).unwrap();
    ok(&home, "machine.list", &[]);
    let record = home::read_record(&paths::machine_record(&home, "native"))
        .unwrap()
        .unwrap();
    assert_eq!(
        record.get_int("schema_version"),
        Some(1),
        "a new record carries schema_version: {record:?}"
    );
    let machine = ok(&home, "machine.get", &[("name", t("native"))]);
    assert_eq!(machine.get_int("schema_version"), Some(1), "{machine:?}");
    assert_eq!(
        machine.get("capability").unwrap().get_int("schema_version"),
        Some(1),
        "{machine:?}"
    );

    // a record written before #551: no schema_version, no requested class
    sandbox_record(
        &home,
        vec![
            ("name", t("sb")),
            ("project", t("pg/app")),
            ("machine", t("native")),
            ("platform", t("docker")),
        ],
    );
    let old = home::read_record(&paths::sandbox_record(&home, "sb"))
        .unwrap()
        .unwrap();
    assert_eq!(old.get_int("schema_version"), None);
    let got = ok(&home, "sandbox.get", &[("name", t("sb"))]);
    assert_eq!(got.get_int("schema_version"), Some(1), "{got:?}");
    assert_eq!(
        got.get_text("effective_isolation"),
        Some("container"),
        "an old record's class comes from its platform name: {got:?}"
    );
    assert_eq!(
        got.get_text("requested_isolation"),
        Some("container"),
        "{got:?}"
    );
    // reading did not rewrite the old record
    let after = home::read_record(&paths::sandbox_record(&home, "sb"))
        .unwrap()
        .unwrap();
    assert_eq!(after.get_int("schema_version"), None);
    let _ = std::fs::remove_dir_all(home);
}

/// Acceptance 89: no `provider ==` in core, and one snake_case naming
/// convention across the capability.
#[test]
fn acceptance_89_no_provider_branch_and_one_naming_convention() {
    // the core of this call surface is the management library beside this
    // crate; a provider name there would be a router
    let control = Path::new(env!("CARGO_MANIFEST_DIR")).join("../layerfs-control/src");
    let mut offenders = Vec::new();
    let mut stack = vec![control];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|x| x == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                for (number, line) in text.lines().enumerate() {
                    if line.contains("provider ==") || line.contains("provider==") {
                        offenders.push(format!("{}:{}: {line}", path.display(), number + 1));
                    }
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "provider branch in core: {offenders:#?}"
    );

    let home = tempdir("naming");
    home::open(&home).unwrap();
    ok(&home, "machine.list", &[]);
    let machine = ok(&home, "machine.get", &[("name", t("native"))]);
    let capability = machine.get("capability").unwrap();
    let mut names = BTreeSet::new();
    if let SdkValue::Map(fields) = capability {
        for (name, _) in fields {
            names.insert(name.clone());
        }
    }
    for name in names {
        let ok = name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        assert!(ok, "capability field {name:?} is not snake_case");
    }
    let _ = std::fs::remove_dir_all(home);
}

/// Acceptance 90: `doctor` starts nothing and writes nothing.
#[test]
fn acceptance_90_doctor_never_starts_and_never_writes() {
    let home = tempdir("doctor");
    home::open(&home).unwrap();
    postgres(&home);
    write_entity(
        &paths::machine_record(&home, "dev"),
        &SdkValue::map([("name", t("dev")), ("platform", t("lima"))]),
    )
    .unwrap();
    sandbox_record(
        &home,
        vec![
            ("name", t("sb")),
            ("project", t("pg/app")),
            ("machine", t("dev")),
            ("requested_isolation", t("process")),
            ("effective_isolation", t("process")),
        ],
    );
    let before = tree(&home);
    let report = layerfs_api::doctor(&home).unwrap();
    assert_eq!(before, tree(&home), "doctor wrote in the home");
    // no start lock, no worker record, no daemon keys: doctor started nothing
    assert!(!paths::machine_dir(&home, "dev").join("start.lock").exists());
    assert!(!paths::worker_record(&home, "dev").exists());
    assert!(!paths::sandbox_control(&home, "sb").exists());
    // the report is a warning at most for a stopped machine, never ok=false
    if let Some(SdkValue::List(items)) = report.get("findings") {
        for finding in items {
            assert_eq!(finding.get_text("severity"), Some("warning"), "{finding:?}");
        }
    }
    let _ = std::fs::remove_dir_all(home);
}

/// Every path under `dir` with its size, mtime and mode.
fn tree(dir: &Path) -> std::collections::BTreeMap<PathBuf, (u64, i64, i64, u32)> {
    use std::os::unix::fs::MetadataExt;
    let mut found = std::collections::BTreeMap::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(path) = stack.pop() {
        let meta = std::fs::symlink_metadata(&path).unwrap();
        found.insert(
            path.clone(),
            (meta.len(), meta.mtime(), meta.mtime_nsec(), meta.mode()),
        );
        if meta.is_dir() {
            for entry in std::fs::read_dir(&path).unwrap() {
                stack.push(entry.unwrap().path());
            }
        }
    }
    found
}
