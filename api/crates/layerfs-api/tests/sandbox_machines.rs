//! Sandboxes on the docker machine and Sandboxes of PostgreSQL Stores (#533
//! step 12 deferral closed by the owner, 2026-11-02). Without settings the
//! refusals run: a PostgreSQL Store's Sandbox on a VM or an ssh host comes
//! later (O3), and a sqlite Store's Sandbox runs on its Store's machine.
//! With settings the live paths run, each through the call table:
//!
//! - a PostgreSQL Store's Sandboxes on `native` and on `docker`, each
//!   committing into the Store: LAYERFS_TEST_PG_URL (a server URL without a
//!   password, to a database with no `layerfs` schema; one is left behind),
//!   LAYERFS_TEST_PG_PASSWORD, LAYERFS_TEST_DAEMON (a host daemon built with
//!   `postgresql`); the docker half also LAYERFS_TEST_IMAGE (a local image
//!   digest) and LAYERFS_BUNDLE (a sealed Linux bundle whose daemon has
//!   `postgresql`). macOS reaches a loopback server from the container as
//!   `host.docker.internal`.
//! - a sqlite Store pushed to the docker machine and its Sandbox, through the
//!   docker worker on macOS: LAYERFS_TEST_IMAGE and LAYERFS_BUNDLE. The test
//!   removes its worker container and data volume.
//!
//! A test whose settings are missing says NOT_RUN and passes.
#![cfg(feature = "machine")]
use layerfs_api::home::{self, paths};
use layerfs_bridge::control::SdkValue;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-sbm-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, (String, String)> {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// A folder with one file, for Init.
fn source(tag: &str) -> PathBuf {
    let folder = tempdir(tag);
    std::fs::write(folder.join("readme.txt"), "hello\n").unwrap();
    folder
}

/// Forks `branch` from main in `project`, mounts it in Sandbox `sandbox`,
/// runs one command that writes `file`, commits and unmounts; then reads
/// `file` back from the Store.
fn work(home: &Path, project: &str, sandbox: &str, branch: &str, file: &str) {
    ok(
        home,
        "branch.fork",
        &[
            ("project", t(project)),
            ("name", t(branch)),
            ("from_branch", t("main")),
        ],
    );
    let mounted = ok(
        home,
        "workspace.mount",
        &[("sandbox", t(sandbox)), ("branch", t(branch))],
    );
    let ws = mounted.get_text("ws").unwrap().to_owned();
    let ran = ok(
        home,
        "workspace.exec",
        &[
            ("ws", t(&ws)),
            (
                "command",
                t(&format!("cat readme.txt && echo {file} > {file}")),
            ),
        ],
    );
    assert_eq!(ran.get_int("exit_code"), Some(0), "{ran:?}");
    let committed = ok(
        home,
        "workspace.commit",
        &[("ws", t(&ws)), ("message", t(file))],
    );
    assert_eq!(
        committed.get_text("kind"),
        Some("Committed"),
        "{committed:?}"
    );
    ok(home, "workspace.unmount", &[("ws", t(&ws))]);
    let read = ok(
        home,
        "file.read",
        &[
            ("project", t(project)),
            ("branch", t(branch)),
            ("path", t(&format!("/{file}"))),
        ],
    );
    assert_eq!(read.get_text("text"), Some(format!("{file}\n").as_str()));
}

#[test]
fn a_sandbox_runs_where_its_store_can_be_reached() {
    let home = tempdir("refusals");
    home::open(&home).unwrap();
    ok(&home, "machine.list", &[]);
    // a PostgreSQL Store and a sqlite Store on native, written as records:
    // the refusals come before any connection
    home::write_record(
        &paths::store_record(&home, "pg"),
        &SdkValue::map([
            ("name", t("pg")),
            ("kind", t("postgresql")),
            ("url", t("postgres://u@127.0.0.1:1/db")),
        ]),
    )
    .unwrap();
    home::write_record(
        &paths::store_record(&home, "shop"),
        &SdkValue::map([("name", t("shop")), ("kind", t("sqlite"))]),
    )
    .unwrap();
    for store in ["pg", "shop"] {
        home::write_record(
            &paths::project_record(&home, store, "app"),
            &SdkValue::map([("name", t("app"))]),
        )
        .unwrap();
    }
    home::write_record(
        &paths::machine_record(&home, "dev"),
        &SdkValue::map([("name", t("dev")), ("platform", t("lima"))]),
    )
    .unwrap();
    home::write_record(
        &paths::machine_record(&home, "box"),
        &SdkValue::map([
            ("name", t("box")),
            ("platform", t("ssh")),
            ("ssh", SdkValue::map([("host", t("box.invalid"))])),
        ]),
    )
    .unwrap();
    let create = |machine: &str| {
        call(
            &home,
            "sandbox.create",
            &[
                ("project", t("pg/app")),
                ("name", t("t")),
                ("machine", t(machine)),
            ],
        )
        .unwrap_err()
    };
    // lima runs PostgreSQL Sandboxes on macOS (G7 live receipt): its
    // stopped machine is refused naming its start; elsewhere the support
    // table's reason, word for word (#535 §7.3)
    let (code, message) = create("dev");
    match layerfs_platform::sandbox_allowed(
        layerfs_platform::Platform::Lima,
        layerfs_platform::HostOs::current(),
        layerfs_platform::StoreKind::Postgresql,
    ) {
        Ok(()) => {
            assert_eq!(code, "Unavailable", "{message}");
            assert!(message.contains("layerfs machine start dev"), "{message}");
        }
        Err(reason) => {
            assert_eq!(code, "Unsupported", "{message}");
            assert!(message.ends_with(reason), "{message}");
        }
    }
    // SSH is disabled by product policy before any reach or startup.
    let (code, message) = create("box");
    assert_eq!(code, "Unsupported", "{message}");
    assert!(
        message == layerfs_sandbox_backend::platform_policy::SSH_DISABLED,
        "{message}"
    );
    let (code, message) = call(
        &home,
        "sandbox.create",
        &[
            ("project", t("shop/app")),
            ("name", t("t")),
            ("machine", t("docker")),
        ],
    )
    .unwrap_err();
    assert_eq!(code, "Unsupported", "{message}");
    assert!(message.contains("is on machine native"), "{message}");
    // nothing was claimed
    assert_eq!(
        ok(&home, "sandbox.list", &[]).get("items"),
        Some(&SdkValue::List(Vec::new()))
    );
    std::fs::remove_dir_all(&home).unwrap();
}

/// Every refusal `sandbox.create` gives for a platform and Store kind is
/// the support table's reason (#535 §7.3, D10): one machine record per
/// platform, a sqlite Store on each and one PostgreSQL Store, all records.
#[test]
fn every_platform_refusal_is_the_support_table_s() {
    use layerfs_platform::{sandbox_allowed, HostOs, Platform, StoreKind};
    let home = tempdir("support");
    home::open(&home).unwrap();
    ok(&home, "machine.list", &[]);
    home::write_record(
        &paths::store_record(&home, "pg"),
        &SdkValue::map([
            ("name", t("pg")),
            ("kind", t("postgresql")),
            ("url", t("postgres://u@127.0.0.1:1/db")),
        ]),
    )
    .unwrap();
    home::write_record(
        &paths::project_record(&home, "pg", "app"),
        &SdkValue::map([("name", t("app"))]),
    )
    .unwrap();
    for platform in Platform::ALL {
        let machine = match platform {
            Platform::Native | Platform::Docker => platform.name().to_owned(),
            _ => format!("m-{}", platform.name()),
        };
        if !platform.built_in() {
            home::write_record(
                &paths::machine_record(&home, &machine),
                &SdkValue::map([("name", t(&machine)), ("platform", t(platform.name()))]),
            )
            .unwrap();
        }
        let store = format!("s-{}", platform.name());
        home::write_record(
            &paths::store_record(&home, &store),
            &SdkValue::map([
                ("name", t(&store)),
                ("kind", t("sqlite")),
                ("machine", t(&machine)),
            ]),
        )
        .unwrap();
        home::write_record(
            &paths::project_record(&home, &store, "app"),
            &SdkValue::map([("name", t("app"))]),
        )
        .unwrap();
        for (kind, project) in [
            (StoreKind::Sqlite, format!("{store}/app")),
            (StoreKind::Postgresql, "pg/app".to_owned()),
        ] {
            // an allowed pair would start a real Sandbox; only refusals run
            let Err(reason) = sandbox_allowed(platform, HostOs::current(), kind) else {
                continue;
            };
            let (code, message) = call(
                &home,
                "sandbox.create",
                &[
                    ("project", t(&project)),
                    ("name", t("t")),
                    ("machine", t(&machine)),
                ],
            )
            .unwrap_err();
            assert_eq!(code, "Unsupported", "{message}");
            assert!(
                message.ends_with(reason),
                "{platform:?} {kind:?}: {message}"
            );
        }
    }
    std::fs::remove_dir_all(&home).unwrap();
}

#[cfg(feature = "postgresql")]
#[test]
fn a_postgresql_store_s_sandboxes_on_native_and_docker() {
    let (Some(url), Some(_), Some(daemon)) = (
        env("LAYERFS_TEST_PG_URL"),
        env("LAYERFS_TEST_PG_PASSWORD"),
        env("LAYERFS_TEST_DAEMON"),
    ) else {
        eprintln!(
            "NOT_RUN: LAYERFS_TEST_PG_URL / LAYERFS_TEST_PG_PASSWORD / LAYERFS_TEST_DAEMON unset"
        );
        return;
    };
    let home = tempdir("pg");
    let folder = source("pg-src");
    home::open(&home).unwrap();
    home::import_env(&home, &[("LAYERFS_DAEMON".to_owned(), daemon)]).unwrap();
    ok(
        &home,
        "store.connect",
        &[
            ("name", t("pg")),
            ("url", t(&url)),
            ("password", t("env:LAYERFS_TEST_PG_PASSWORD")),
        ],
    );
    ok(
        &home,
        "project.init",
        &[
            ("project", t("pg/app")),
            ("folder", t(&folder.display().to_string())),
        ],
    );
    ok(
        &home,
        "sandbox.create",
        &[
            ("project", t("pg/app")),
            ("name", t("n")),
            ("machine", t("native")),
        ],
    );
    work(&home, "pg/app", "n", "on-native", "native.txt");
    ok(&home, "sandbox.close", &[("name", t("n"))]);
    if let (Some(image), Some(_)) = (env("LAYERFS_TEST_IMAGE"), env("LAYERFS_BUNDLE")) {
        ok(
            &home,
            "machine.update",
            &[("name", t("docker")), ("image", t(&image))],
        );
        ok(
            &home,
            "sandbox.create",
            &[
                ("project", t("pg/app")),
                ("name", t("d")),
                ("machine", t("docker")),
            ],
        );
        work(&home, "pg/app", "d", "on-docker", "docker.txt");
        ok(&home, "sandbox.close", &[("name", t("d"))]);
    } else {
        eprintln!("NOT_RUN (docker half): LAYERFS_TEST_IMAGE / LAYERFS_BUNDLE unset");
    }
    // the password is resolved per call and never written
    let password = std::env::var("LAYERFS_TEST_PG_PASSWORD").unwrap();
    for entry in walk(&home) {
        let bytes = std::fs::read(&entry).unwrap_or_default();
        assert!(
            !bytes
                .windows(password.len())
                .any(|w| w == password.as_bytes()),
            "{}",
            entry.display()
        );
    }
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&folder).unwrap();
}

#[cfg(feature = "postgresql")]
fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => files.extend(walk(&path)),
            Ok(kind) if kind.is_file() => files.push(path),
            _ => {}
        }
    }
    files
}

#[test]
fn a_sqlite_store_s_sandbox_on_the_docker_machine() {
    let (Some(image), Some(_)) = (env("LAYERFS_TEST_IMAGE"), env("LAYERFS_BUNDLE")) else {
        eprintln!("NOT_RUN: LAYERFS_TEST_IMAGE / LAYERFS_BUNDLE unset");
        return;
    };
    let home = tempdir("docker");
    let folder = source("docker-src");
    let id = home::open(&home).unwrap().id;
    ok(
        &home,
        "machine.update",
        &[("name", t("docker")), ("image", t(&image))],
    );
    ok(&home, "store.create", &[("name", t("shop"))]);
    ok(
        &home,
        "project.init",
        &[
            ("project", t("shop/app")),
            ("folder", t(&folder.display().to_string())),
        ],
    );
    let moved = ok(
        &home,
        "store.push",
        &[("name", t("shop")), ("machine", t("docker"))],
    );
    assert_eq!(moved.get_text("to"), Some("docker"), "{moved:?}");
    let outcome = std::panic::catch_unwind(|| {
        let created = ok(
            &home,
            "sandbox.create",
            &[
                ("project", t("shop/app")),
                ("name", t("t")),
                ("machine", t("docker")),
            ],
        );
        assert_eq!(created.get_text("runtime"), Some("docker"), "{created:?}");
        work(&home, "shop/app", "t", "on-docker", "docker.txt");
        ok(&home, "sandbox.close", &[("name", t("t"))]);
    });
    // this test's own worker container and its data volume, whatever happened
    let worker = format!("lfs-{}-docker", &id[..8]);
    let volume = format!("{worker}-data");
    for args in [
        ["rm", "-f", worker.as_str()],
        ["volume", "rm", volume.as_str()],
    ] {
        let _ = std::process::Command::new("docker").args(args).output();
    }
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&folder);
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

/// With no machine named, `sandbox.create` runs where the Store's calls
/// run (#536 §10): a sqlite Store's machine (here a stopped microsandbox machine,
/// so the create is refused naming its start), and `native` for a
/// PostgreSQL Store (refused here by the reach check, which runs there).
#[test]
fn a_sandbox_with_no_machine_runs_on_its_store_s() {
    let home = tempdir("default");
    home::open(&home).unwrap();
    home::write_record(
        &paths::machine_record(&home, "box"),
        // microsandbox runs on macOS and Linux hosts, so the support table
        // passes this machine on either
        &SdkValue::map([("name", t("box")), ("platform", t("microsandbox"))]),
    )
    .unwrap();
    home::write_record(
        &paths::store_record(&home, "shop"),
        &SdkValue::map([
            ("name", t("shop")),
            ("kind", t("sqlite")),
            ("machine", t("box")),
        ]),
    )
    .unwrap();
    home::write_record(
        &paths::project_record(&home, "shop", "api"),
        &SdkValue::map([("name", t("api"))]),
    )
    .unwrap();
    let (code, message) = call(
        &home,
        "sandbox.create",
        &[("project", t("shop/api")), ("name", t("t1"))],
    )
    .unwrap_err();
    assert_eq!(code, "Unavailable", "{message}");
    assert!(message.contains("layerfs machine start box"), "{message}");
    assert!(
        !paths::sandbox_dir(&home, "t1").exists(),
        "a refused create leaves no claim"
    );

    home::write_record(
        &paths::store_record(&home, "pg"),
        &SdkValue::map([
            ("name", t("pg")),
            ("kind", t("postgresql")),
            ("url", t("postgres://u@127.0.0.1:1/db")),
        ]),
    )
    .unwrap();
    home::write_record(
        &paths::project_record(&home, "pg", "app"),
        &SdkValue::map([("name", t("app"))]),
    )
    .unwrap();
    let refused = call(
        &home,
        "sandbox.create",
        &[("project", t("pg/app")), ("name", t("t2"))],
    )
    .unwrap_err();
    assert_ne!(refused.0, "NotFound", "{refused:?}");
    assert!(!refused.1.contains("machine start"), "{refused:?}");
    // the reach check names the machine; a build without the PostgreSQL
    // engine says so instead of "cannot reach", and a server that takes
    // no connection is said to, not shown as an engine status
    let (code, message) = refused;
    if cfg!(feature = "postgresql") {
        assert_eq!(code, "Unavailable", "{message}");
        assert!(
            message.starts_with("machine native cannot reach PostgreSQL Store pg's server at 127.0.0.1: no connection to the server was made"),
            "{message}"
        );
    } else {
        assert_eq!(code, "Unsupported", "{message}");
        assert!(
            message.starts_with("machine native cannot check PostgreSQL Store pg's server:"),
            "{message}"
        );
    }
    assert!(!message.contains("Closed"), "{message}");
    let _ = std::fs::remove_dir_all(home);
}
