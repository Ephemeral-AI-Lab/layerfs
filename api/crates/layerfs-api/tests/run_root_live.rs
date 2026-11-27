//! #819 R with a live host Sandbox (process backend): a home starts its
//! Sandboxes in `machine/macos-host/run/` and leaves a stale
//! `machine/native/run/` as it is;
//! the Sandbox is mounted, run in, listed by `sandbox.files` and removed
//! there.
#![cfg(all(target_os = "macos", feature = "machine"))]
use layerfs_api::home::{self, paths, read_record};
use layerfs_protocol::sdk::SdkValue;
use std::path::{Path, PathBuf};

fn t(value: &str) -> SdkValue {
    SdkValue::text(value)
}

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    layerfs_api::call(
        Some(home),
        name,
        SdkValue::Map(
            (args.iter())
                .map(|(name, value)| ((*name).to_owned(), value.clone()))
                .collect(),
        ),
        &mut |_| {},
    )
    .unwrap_or_else(|error| panic!("{name}: {error:?}"))
}

/// A home with Store `shop`, project `shop/app` and Branch `work`, whose
/// host machine runs `daemon`.
fn home_with_project(tag: &str, daemon: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("lfs-runroot-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    home::open(&home).unwrap();
    let source = home.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("README.md"), b"saved\n").unwrap();
    call(&home, "store.create", &[("name", t("shop"))]);
    call(
        &home,
        "project.init",
        &[
            ("project", t("shop/app")),
            ("folder", t(&source.display().to_string())),
        ],
    );
    call(
        &home,
        "branch.fork",
        &[
            ("project", t("shop/app")),
            ("name", t("work")),
            ("from", t("main")),
        ],
    );
    call(
        &home,
        "machine.update",
        &[
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
            ("daemon", t(daemon)),
        ],
    );
    home
}

/// Starts Sandbox `name` on the host machine; answers its run folder.
fn create(home: &Path, name: &str) -> PathBuf {
    call(
        home,
        "sandbox.create",
        &[
            ("project", t("shop/app")),
            ("name", t(name)),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
        ],
    );
    let record = read_record(&paths::sandbox_record(home, name))
        .unwrap()
        .unwrap();
    let daemon = record.get("daemon").unwrap();
    PathBuf::from(daemon.get_text("directory").unwrap())
}

/// Mounts `<name>:work`, runs a command in it and reads the Sandbox's
/// home through `sandbox.files`.
fn used(home: &Path, name: &str, run: &Path) {
    call(
        home,
        "workspace.mount",
        &[
            ("sandbox", t(name)),
            ("branch", t("work")),
            ("name", t("work")),
        ],
    );
    let ran = call(
        home,
        "workspace.exec",
        &[
            ("workspace", t(&format!("{name}:work"))),
            ("command", t("cat README.md")),
        ],
    );
    assert!(format!("{ran:?}").contains("saved"), "{ran:?}");
    let roots = call(home, "sandbox.files", &[("sandbox", t(name))]);
    let shown = format!("{roots:?}");
    assert!(
        shown.contains(&run.join("tmp").display().to_string()),
        "{shown}"
    );
    let got = call(home, "sandbox.get", &[("sandbox", t(name))]);
    assert_eq!(got.get_text("state"), Some("ready"), "{got:?}");
}

#[test]
fn a_host_sandbox_runs_in_the_host_machine_s_run_root() {
    let Ok(daemon) = std::env::var("LAYERFS_DAEMON") else {
        eprintln!("NOT_RUN: explicit LAYERFS_DAEMON required for live host Sandboxes");
        return;
    };
    // the host machine's run root; what an earlier build left in
    // `machine/native/run` is neither used nor touched
    let fresh = home_with_project("fresh", &daemon);
    let stale = fresh.join("machine/native/run/layerfs-0123abcd-old");
    std::fs::create_dir_all(&stale).unwrap();
    let run = create(&fresh, "first");
    assert_eq!(run.parent(), Some(paths::run_root(&fresh).as_path()));
    assert!(run.starts_with(
        fresh
            .join("machine")
            .join(layerfs_home::names::HOST_MACHINE)
    ));
    assert_eq!(
        std::fs::read_dir(fresh.join("machine/native/run"))
            .unwrap()
            .count(),
        1
    );
    assert!(stale.is_dir());
    used(&fresh, "first", &run);
    call(
        &fresh,
        "sandbox.remove",
        &[("sandbox", t("first")), ("force", SdkValue::Bool(true))],
    );
    assert!(!run.exists());
    let _ = std::fs::remove_dir_all(&fresh);
}
