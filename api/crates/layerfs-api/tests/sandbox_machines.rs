//! Sandboxes on the docker machine and Sandboxes of PostgreSQL Stores (#533
//! step 12 deferral closed by the owner, 2026-11-02). Without settings the
//! refusals run: a PostgreSQL Store's Sandbox on a VM comes
//! later (O3), and a sqlite Store's Sandbox runs on its Store's machine.
//! With settings the live paths run, each through the call table:
//!
//! - a PostgreSQL Store's Sandboxes on `native` and on `docker`, each
//!   committing into the Store: LAYERFS_TEST_PG_URL (a server URL without a
//!   password, to a database with no `layerfs` schema; one is left behind),
//!   LAYERFS_TEST_PG_PASSWORD, LAYERFS_TEST_DAEMON (a host daemon built with
//!   `postgresql`); the docker half also LAYERFS_TEST_IMAGE (a local image
//!   digest, the worker container's) and LAYERFS_BUNDLE (a sealed Linux
//!   bundle whose daemon has `postgresql`). The URL is never rewritten: on
//!   Docker Desktop a loopback URL is refused for the docker machine, which
//!   is then given its own URL naming `host.docker.internal`, which its
//!   Sandboxes resolve with the worker's resolver files; their Workspace
//!   and Commit never show those files.
//! - a sqlite Store pushed to the docker machine and its Sandbox, through the
//!   docker worker: LAYERFS_TEST_IMAGE and LAYERFS_BUNDLE. The Sandbox runs
//!   in namespaces inside the privileged worker container and shows
//!   `backend: "namespaces"` with `platform: "docker"` (D-8). The test
//!   removes its worker container and data volume.
//!
//! - a sqlite Store's Sandbox on each VM machine (`lima`,
//!   `apple-container`, ...) named in LAYERFS_TEST_MACHINES (comma
//!   separated), each created and started beforehand in the home
//!   LAYERFS_TEST_MACHINE_HOME: a machine's boot is setup, never part of
//!   this test. The Store is made here, pushed there and its project
//!   initialised through the worker; the test closes its Sandbox and
//!   removes its Store, and leaves the machines running.
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

/// PostgreSQL Store `pg` as a 2.0 home records it: its address and the
/// saved login it names (a test login; nothing is dialled with it here).
fn pg_record(home: &Path) {
    let login = [
        ("auth", t("pg")),
        ("user", t("u")),
        ("password", t("not-a-real-password")),
    ];
    ok(home, "store.save_auth", &login);
    home::write_record(
        &paths::store_record(home, "pg"),
        &SdkValue::map([
            ("name", t("pg")),
            ("kind", t("postgresql")),
            ("url", t("postgres://127.0.0.1:1/db")),
            ("auth", t("pg")),
        ]),
    )
    .unwrap();
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
            ("from", t("main")),
        ],
    );
    let mounted = ok(
        home,
        "workspace.mount",
        &[("sandbox", t(sandbox)), ("branch", t(branch))],
    );
    let ws = mounted.get_text("workspace").unwrap().to_owned();
    let ran = ok(
        home,
        "workspace.exec",
        &[
            ("workspace", t(&ws)),
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
        &[("workspace", t(&ws)), ("message", t(file))],
    );
    // a new Commit: its id, and no `unchanged`
    assert!(committed.get_text("commit").is_some(), "{committed:?}");
    assert!(committed.get("unchanged").is_none(), "{committed:?}");
    ok(home, "workspace.unmount", &[("workspace", t(&ws))]);
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
    pg_record(&home);
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
        &SdkValue::map([
            ("name", t("dev")),
            ("platform", t("lima")),
            ("vm_name", t("layerfs-test-dev")),
        ]),
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
            // a loopback URL is refused before the machine is asked: lima
            // does not share this host's loopback, and nothing rewrites it
            assert_eq!(code, "Unsupported", "{message}");
            assert!(message.contains("host.lima.internal"), "{message}");
            assert!(message.contains("--machine dev`"), "{message}");
            // any other host: the stopped machine is refused naming its start
            let record = paths::store_record(&home, "pg");
            let mut fields = home::read_record(&record).unwrap().unwrap();
            if let SdkValue::Map(entries) = &mut fields {
                for (name, value) in entries.iter_mut() {
                    if name == "url" {
                        *value = t("postgres://db.invalid:1/db");
                    }
                }
            }
            home::write_record(&record, &fields).unwrap();
            let (code, message) = create("dev");
            assert_eq!(code, "Unavailable", "{message}");
            assert!(message.contains("layerfs machine start dev"), "{message}");
            pg_record(&home);
        }
        Err(reason) => {
            assert_eq!(code, "Unsupported", "{message}");
            assert!(message.ends_with(reason), "{message}");
        }
    }
    // an ssh machine left by an older home: its platform was removed in
    // 2.0, refused before any reach or startup
    let (code, message) = create("box");
    assert_eq!(code, "Unsupported", "{message}");
    assert!(
        message == layerfs_sandbox_backend::platform_policy::SSH_REMOVED,
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
    pg_record(&home);
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
    let id = home::open(&home).unwrap().id;
    // the native machine's daemon is a setting of its record
    layerfs_api::call(
        Some(&home),
        "machine.update",
        SdkValue::map([
            ("machine", SdkValue::text("native")),
            ("daemon", SdkValue::text(daemon.as_str())),
        ]),
        &mut |_| {},
    )
    .unwrap();
    let (scheme, rest) = url.split_once("://").unwrap();
    let (user, address) = rest.split_once('@').unwrap();
    let password = std::env::var("LAYERFS_TEST_PG_PASSWORD").unwrap();
    ok(
        &home,
        "store.save_auth",
        &[
            ("auth", t("pg")),
            ("user", t(user)),
            ("password", t(&password)),
        ],
    );
    ok(
        &home,
        "store.connect",
        &[
            ("store", t("pg")),
            ("url", t(&format!("{scheme}://{address}"))),
            ("auth", t("pg")),
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
    // the create proved this computer reaches the Store: `native` with no
    // address of its own is this computer's row (spec v23 V23-2)
    let got = ok(
        &home,
        "store.get",
        &[("store", t("pg")), ("detail", SdkValue::Bool(true))],
    );
    let Some(SdkValue::List(rows)) = got.get("connections") else {
        panic!("{got:?}");
    };
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].get_text("reached"), Some("ok"), "{rows:?}");
    work(&home, "pg/app", "n", "on-native", "native.txt");
    ok(&home, "sandbox.remove", &[("sandbox", t("n"))]);
    if let (Some(image), Some(_)) = (env("LAYERFS_TEST_IMAGE"), env("LAYERFS_BUNDLE")) {
        ok(
            &home,
            "machine.update",
            &[("machine", t("docker")), ("image", t(&image))],
        );
        let outcome = std::panic::catch_unwind(|| {
            // only `machine.start` starts a machine (D11)
            ok(&home, "machine.start", &[("machine", t("docker"))]);
            let create = || {
                call(
                    &home,
                    "sandbox.create",
                    &[
                        ("project", t("pg/app")),
                        ("name", t("d")),
                        ("machine", t("docker")),
                    ],
                )
            };
            // Docker Desktop's worker does not share this host's loopback:
            // a loopback URL is refused, naming the host's name there, and
            // the machine gets its own URL (nothing is rewritten)
            let loopback = ["127.0.0.1", "localhost", "[::1]"]
                .iter()
                .find(|host| address.starts_with(*host));
            if let (Some(host), false) = (loopback, cfg!(target_os = "linux")) {
                let (code, message) = create().unwrap_err();
                assert_eq!(code, "Unsupported", "{message}");
                assert!(message.contains("host.docker.internal"), "{message}");
                assert!(message.contains("--machine docker"), "{message}");
                let own = address.replacen(host, "host.docker.internal", 1);
                ok(
                    &home,
                    "store.connect",
                    &[
                        ("store", t("pg")),
                        ("url", t(&format!("{scheme}://{own}"))),
                        ("machine", t("docker")),
                    ],
                );
            }
            // the create's reach check dials from the worker container
            create().unwrap();
            resolves_as_the_worker(&home, "pg/app", "d");
            work(&home, "pg/app", "d", "on-docker", "docker.txt");
            ok(&home, "sandbox.remove", &[("sandbox", t("d"))]);
        });
        remove_worker(&id);
        if let Err(panic) = outcome {
            std::panic::resume_unwind(panic);
        }
    } else {
        eprintln!("NOT_RUN (docker half): LAYERFS_TEST_IMAGE / LAYERFS_BUNDLE unset");
    }
    // the password is in the home's secret folder only
    let password = std::env::var("LAYERFS_TEST_PG_PASSWORD").unwrap();
    for entry in walk(&home)
        .into_iter()
        .filter(|e| !e.starts_with(home.join("secret")))
    {
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

/// A Sandbox resolves names as its machine's worker does: the worker's
/// `/etc/resolv.conf` and `/etc/hosts` are bound over its root's, so
/// `host.docker.internal` resolves in it on Docker Desktop. Reading them
/// changes nothing a Workspace shows: no change, an UpToDate Commit, and
/// neither file in the Workspace's tree.
fn resolves_as_the_worker(home: &Path, project: &str, sandbox: &str) {
    ok(
        home,
        "branch.fork",
        &[
            ("project", t(project)),
            ("name", t("resolver")),
            ("from", t("main")),
        ],
    );
    let mounted = ok(
        home,
        "workspace.mount",
        &[("sandbox", t(sandbox)), ("branch", t("resolver"))],
    );
    let ws = mounted.get_text("workspace").unwrap().to_owned();
    let name = if cfg!(target_os = "linux") {
        "localhost"
    } else {
        "host.docker.internal"
    };
    let ran = ok(
        home,
        "workspace.exec",
        &[
            ("workspace", t(&ws)),
            (
                "command",
                t(&format!(
                    "test -s /etc/resolv.conf && test -s /etc/hosts && getent hosts {name} && ls -a"
                )),
            ),
        ],
    );
    assert_eq!(ran.get_int("exit_code"), Some(0), "{ran:?}");
    let listed = ran.get_text("stdout").unwrap_or_default().to_owned();
    for line in listed.lines() {
        assert!(!matches!(line, "etc" | "resolv.conf" | "hosts"), "{listed}");
    }
    let status = ok(home, "workspace.status", &[("workspace", t(&ws))]);
    assert_eq!(
        status.get("changes"),
        Some(&SdkValue::List(Vec::new())),
        "{status:?}"
    );
    let committed = ok(home, "workspace.commit", &[("workspace", t(&ws))]);
    assert!(committed.get("unchanged").is_some(), "{committed:?}");
    ok(home, "workspace.unmount", &[("workspace", t(&ws))]);
}

/// Removes the docker worker container and data volume of the home `id`,
/// whatever its test did.
fn remove_worker(id: &str) {
    let worker = format!("layerfs-{}-docker", &id[..8]);
    let volume = format!("{worker}-data");
    for args in [
        ["rm", "-f", worker.as_str()],
        ["volume", "rm", volume.as_str()],
    ] {
        let _ = std::process::Command::new("docker").args(args).output();
    }
}

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
        &[("machine", t("docker")), ("image", t(&image))],
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
    let outcome = std::panic::catch_unwind(|| {
        // only `machine.start` starts a machine (D11)
        ok(&home, "machine.start", &[("machine", t("docker"))]);
        let moved = ok(
            &home,
            "store.move",
            &[("store", t("shop")), ("machine", t("docker"))],
        );
        assert_eq!(moved.get_text("to"), Some("docker"), "{moved:?}");
        let created = ok(
            &home,
            "sandbox.create",
            &[
                ("project", t("shop/app")),
                ("name", t("t")),
                ("machine", t("docker")),
            ],
        );
        // namespaces in the docker machine's worker container, which keeps
        // showing the docker platform (D-8)
        assert_eq!(
            created.get_text("runtime"),
            Some("namespaces"),
            "{created:?}"
        );
        assert_eq!(
            created.get_text("backend"),
            Some("namespaces"),
            "{created:?}"
        );
        assert_eq!(created.get_text("platform"), Some("docker"), "{created:?}");
        work(&home, "shop/app", "t", "on-docker", "docker.txt");
        ok(&home, "sandbox.remove", &[("sandbox", t("t"))]);
    });
    remove_worker(&id);
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&folder);
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

#[test]
fn a_sqlite_store_s_sandbox_on_each_running_vm_machine() {
    let (Some(home), Some(machines)) = (
        env("LAYERFS_TEST_MACHINE_HOME"),
        env("LAYERFS_TEST_MACHINES"),
    ) else {
        eprintln!("NOT_RUN: LAYERFS_TEST_MACHINE_HOME / LAYERFS_TEST_MACHINES unset");
        return;
    };
    let home = PathBuf::from(home);
    for machine in machines.split(',') {
        let status = ok(&home, "machine.status", &[("name", t(machine))]);
        assert_eq!(
            status.get_text("state"),
            Some("running"),
            "start {machine} before this test: {status:?}"
        );
        let store = format!("vm{machine}{}", std::process::id());
        let project = format!("{store}/app");
        let folder = source(&format!("vm-{machine}"));
        ok(&home, "store.create", &[("name", t(&store))]);
        let moved = ok(
            &home,
            "store.move",
            &[("store", t(&store)), ("machine", t(machine))],
        );
        assert_eq!(moved.get_text("to"), Some(machine), "{moved:?}");
        // Init through the machine's worker, where the Store now is
        let made = ok(
            &home,
            "project.init",
            &[
                ("project", t(&project)),
                ("folder", t(&folder.display().to_string())),
                ("detail", SdkValue::Bool(true)),
            ],
        );
        assert_eq!(made.get_text("machine"), Some(machine), "{made:?}");
        let created = ok(
            &home,
            "sandbox.create",
            &[
                ("project", t(&project)),
                ("name", t(&store)),
                ("machine", t(machine)),
            ],
        );
        assert_eq!(
            created.get_text("backend"),
            Some("namespaces"),
            "{created:?}"
        );
        assert_eq!(created.get_text("machine"), Some(machine), "{created:?}");
        work(&home, &project, &store, "on-vm", "vm.txt");
        ok(&home, "sandbox.remove", &[("sandbox", t(&store))]);
        ok(&home, "store.remove", &[("store", t(&store))]);
        std::fs::remove_dir_all(&folder).unwrap();
        eprintln!("RAN on {machine}: {created:?}");
    }
}

/// With no machine named, `sandbox.create` runs where the Store's calls
/// run (#536 §10): a sqlite Store's machine (here a docker machine with no worker,
/// so the create is refused naming its start), and `native` for a
/// PostgreSQL Store (refused here by the reach check, which runs there).
#[test]
fn a_sandbox_with_no_machine_runs_on_its_store_s() {
    let home = tempdir("default");
    home::open(&home).unwrap();
    home::write_record(
        &paths::machine_record(&home, "box"),
        // docker runs on macOS and Linux hosts, so the support table passes
        // this machine on either
        &SdkValue::map([
            ("name", t("box")),
            ("platform", t("docker")),
            ("vm_name", t("layerfs-00000000-box")),
        ]),
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

    pg_record(&home);
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
    if true {
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
