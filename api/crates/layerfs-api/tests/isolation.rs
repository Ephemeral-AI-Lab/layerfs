//! The #551 isolation contract at the call surface: machine and Sandbox
//! read models carry the class, and `sandbox.create` defaults to `docker`,
//! makes `process` opt-in, and refuses a class the machine cannot provide
//! with `Unavailable { requested, available }` instead of downgrading.
//!
//! Run with `cargo test -p layerfs-api --test isolation --features machine`: the
//! `machine` feature is off by default, so without it this file compiles to
//! an empty test.
#![cfg(feature = "machine")]
use layerfs_api::home::{self, paths, write_record};
use layerfs_bridge::control::SdkValue;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-iso-{tag}-{}", std::process::id()));
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

/// A PostgreSQL Store and its project: its Sandbox has no Store machine, so
/// the create's default isolation decides where it runs.
fn postgres(home: &Path) {
    write_record(
        &paths::store_record(home, "pg"),
        &SdkValue::map([
            ("name", t("pg")),
            ("kind", t("postgresql")),
            ("url", t("postgres://u@127.0.0.1:1/db")),
        ]),
    )
    .unwrap();
    write_record(
        &paths::project_record(home, "pg", "app"),
        &SdkValue::map([("name", t("app"))]),
    )
    .unwrap();
}

#[test]
fn machine_read_models_show_isolation_and_scheduling() {
    let home = tempdir("machine");
    home::open(&home).unwrap();
    // the built-in records are written with the contract on first use
    ok(&home, "machine.list", &[]);
    let record = home::read_record(&paths::machine_record(&home, "native"))
        .unwrap()
        .unwrap();
    assert_eq!(record.get_text("isolation_class"), Some("process"));
    assert_eq!(record.get_text("exec_mode"), Some("worker"));
    assert_eq!(record.get_text("kernel"), Some("shared"));

    for (machine, class) in [("native", "process"), ("docker", "container")] {
        let got = ok(&home, "machine.get", &[("name", t(machine))]);
        assert_eq!(got.get_text("isolation_class"), Some(class), "{got:?}");
        assert_eq!(got.get_text("exec_mode"), Some("worker"), "{got:?}");
        assert_eq!(got.get_text("kernel"), Some("shared"), "{got:?}");
        let isolation = got.get("isolation").unwrap();
        assert_eq!(isolation.get_text("class"), Some(class), "{got:?}");
        assert_eq!(isolation.get_text("kernel"), Some("shared"), "{got:?}");
        assert_eq!(
            got.get("scheduling").unwrap().get("worker"),
            Some(&SdkValue::Bool(true)),
            "{got:?}"
        );
    }

    // a VM machine's record writes the contract too; its read model says
    // `vm`, a separate kernel and the platform's worker
    write_record(
        &paths::machine_record(&home, "dev"),
        &SdkValue::map([("name", t("dev")), ("platform", t("lima"))]),
    )
    .unwrap();
    let got = ok(&home, "machine.get", &[("name", t("dev"))]);
    assert_eq!(got.get_text("isolation_class"), Some("vm"), "{got:?}");
    assert_eq!(got.get_text("kernel"), Some("separate"), "{got:?}");
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn a_class_the_machine_cannot_provide_is_unavailable_not_downgraded() {
    let home = tempdir("mismatch");
    home::open(&home).unwrap();
    ok(&home, "machine.list", &[]);
    postgres(&home);

    // native provides process; container is stronger, so the create is
    // refused and both classes are in the detail
    let refusal = refused(
        &home,
        "sandbox.create",
        &[
            ("project", t("pg/app")),
            ("name", t("t")),
            ("machine", t("native")),
            ("isolation", t("container")),
        ],
    );
    assert_eq!(refusal.get_text("code"), Some("Unavailable"), "{refusal:?}");
    assert_eq!(
        detail_text(&refusal, "requested").as_deref(),
        Some("container"),
        "{refusal:?}"
    );
    assert_eq!(
        detail_text(&refusal, "available").as_deref(),
        Some("process"),
        "{refusal:?}"
    );
    assert!(
        refusal
            .get_text("message")
            .unwrap()
            .contains("provides process isolation, not container"),
        "{refusal:?}"
    );
    // never a downgraded Sandbox: nothing was claimed
    assert!(!paths::sandbox_dir(&home, "t").exists());

    // docker provides container; vm is stronger
    let refusal = refused(
        &home,
        "sandbox.create",
        &[
            ("project", t("pg/app")),
            ("name", t("t")),
            ("machine", t("docker")),
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
        Some("container"),
        "{refusal:?}"
    );

    // an unknown class is the caller's mistake, before any effect
    let refusal = refused(
        &home,
        "sandbox.create",
        &[
            ("project", t("pg/app")),
            ("name", t("t")),
            ("machine", t("native")),
            ("isolation", t("jail")),
        ],
    );
    assert_eq!(
        refusal.get_text("code"),
        Some("InvalidInput"),
        "{refusal:?}"
    );
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn a_postgresql_sandbox_defaults_to_docker_and_process_is_opt_in() {
    let home = tempdir("default");
    home::open(&home).unwrap();
    ok(&home, "machine.list", &[]);
    postgres(&home);

    // no machine and no isolation: docker is the default where the host
    // has it. A recorded image keeps the create from loading the bundle,
    // and a worker endpoint on a closed port makes the create name the
    // docker machine instead of silently using native.
    write_record(
        &paths::machine_record(&home, "docker"),
        &SdkValue::map([
            ("name", t("docker")),
            ("platform", t("docker")),
            (
                "image",
                t("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            ),
        ]),
    )
    .unwrap();
    write_record(
        &paths::worker_record(&home, "docker"),
        &SdkValue::map([
            ("address", t("127.0.0.1:1")),
            (
                "secret",
                t("0000000000000000000000000000000000000000000000000000000000000000"),
            ),
        ]),
    )
    .unwrap();
    let refusal = refused(
        &home,
        "sandbox.create",
        &[("project", t("pg/app")), ("name", t("d"))],
    );
    assert_eq!(refusal.get_text("code"), Some("Unavailable"), "{refusal:?}");
    assert!(
        refusal
            .get_text("message")
            .unwrap()
            .contains("machine docker"),
        "{refusal:?}"
    );
    assert!(!paths::sandbox_dir(&home, "d").exists());

    // process is opt-in: with isolation process the create reaches native
    let refusal = refused(
        &home,
        "sandbox.create",
        &[
            ("project", t("pg/app")),
            ("name", t("n")),
            ("isolation", t("process")),
        ],
    );
    // a build without the PostgreSQL engine answers `Unsupported`; with
    // it, the reach check is `Unavailable`. Either way it names native,
    // never docker.
    assert!(
        matches!(
            refusal.get_text("code"),
            Some("Unavailable") | Some("Unsupported")
        ),
        "{refusal:?}"
    );
    let message = refusal.get_text("message").unwrap();
    assert!(message.contains("native"), "{refusal:?}");
    assert!(!message.contains("docker"), "{refusal:?}");
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn sandbox_read_models_show_requested_and_effective_isolation() {
    let home = tempdir("sandbox");
    home::open(&home).unwrap();
    ok(&home, "machine.list", &[]);
    // no id: the Sandbox never started, so `sandbox.get` asks no worker
    write_record(
        &paths::sandbox_record(&home, "t"),
        &SdkValue::map([
            ("name", t("t")),
            ("project", t("pg/app")),
            ("machine", t("docker")),
            ("platform", t("docker")),
            ("backend", t("docker")),
            ("requested_isolation", t("process")),
            ("effective_isolation", t("container")),
            ("exec_mode", t("worker")),
        ]),
    )
    .unwrap();

    let listed = ok(&home, "sandbox.list", &[]);
    let SdkValue::List(items) = listed.get("items").unwrap() else {
        panic!("{listed:?}")
    };
    let item = items
        .iter()
        .find(|item| item.get_text("name") == Some("t"))
        .unwrap();
    assert_eq!(item.get_text("requested_isolation"), Some("process"));
    assert_eq!(item.get_text("effective_isolation"), Some("container"));
    let isolation = item.get("isolation").unwrap();
    assert_eq!(isolation.get_text("requested"), Some("process"));
    assert_eq!(isolation.get_text("effective"), Some("container"));
    assert_eq!(isolation.get_text("kernel"), Some("shared"));
    assert_eq!(isolation.get_text("strength"), Some("medium"));
    assert_eq!(
        item.get("scheduling").unwrap().get("worker"),
        Some(&SdkValue::Bool(true))
    );

    let got = ok(&home, "sandbox.get", &[("name", t("t"))]);
    assert_eq!(got.get_text("requested_isolation"), Some("process"));
    assert_eq!(got.get_text("effective_isolation"), Some("container"));
    assert_eq!(
        got.get("isolation").unwrap().get_text("class"),
        Some("container")
    );
    let _ = std::fs::remove_dir_all(home);
}
