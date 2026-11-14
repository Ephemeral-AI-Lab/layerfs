//! Provider-direct force stop preserves storage and avoids a dead worker entirely.
#![cfg(feature = "machine")]
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_bridge::control::SdkValue;
use std::{net::TcpListener, os::unix::fs::PermissionsExt, path::Path};
fn t(value: &str) -> SdkValue {
    SdkValue::text(value)
}
fn call(
    home: &Path,
    name: &str,
    args: &[(&str, SdkValue)],
) -> Result<SdkValue, layerfs_bridge::control::SdkRefusal> {
    layerfs_api::call(
        Some(home),
        name,
        SdkValue::Map(
            args.iter()
                .map(|(name, value)| ((*name).to_owned(), value.clone()))
                .collect(),
        ),
        &mut |_| {},
    )
}
#[test]
fn force_stop_bypasses_in_use_and_dead_worker_preserving_disk_store_and_records() {
    let root = std::env::temp_dir().join(format!("lfs-force-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let home = root.join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    home::open(&home).unwrap();
    let tool = root.join("limactl");
    std::fs::write(
        &tool,
        r#"#!/bin/sh
d=${0%/*}
echo "$*" >> "$d/calls"
case "$1" in
 list) printf 'Running\t/nonexistent\n';;
 stop) test "$2" = --force || exit 19; test ! -f "$d/refuse" || exit 18;;
 *) exit 17;;
esac
"#,
    )
    .unwrap();
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::env::set_var("PATH", &root);
    write_record(
        &paths::machine_record(&home, "vm"),
        &SdkValue::map([
            ("name", t("vm")),
            ("platform", t("lima")),
            ("state", t("running")),
        ]),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let worker = SdkValue::map([
        ("address", t(&listener.local_addr().unwrap().to_string())),
        ("secret", t("aa")),
    ]);
    write_record(&paths::worker_record(&home, "vm"), &worker).unwrap();
    for (sandbox, machine) in [("one", "vm"), ("two", "vm")] {
        write_record(
            &paths::sandbox_record(&home, sandbox),
            &SdkValue::map([
                ("name", t(sandbox)),
                ("id", t(sandbox)),
                ("machine", t(machine)),
                ("project", t("shop/app")),
            ]),
        )
        .unwrap();
        write_record(
            &paths::workspace_dir(&home, sandbox).join("work.json"),
            &SdkValue::map([("name", t("work")), ("path", t("/workspaces/one"))]),
        )
        .unwrap();
    }
    let (store, disk) = (paths::store_file(&home, "shop"), root.join("disk.img"));
    write_record(
        &paths::store_record(&home, "shop"),
        &SdkValue::map([
            ("name", t("shop")),
            ("kind", t("sqlite")),
            ("machine", t("vm")),
            ("profile", t("disposable")),
        ]),
    )
    .unwrap();
    let store_record = std::fs::read(paths::store_record(&home, "shop")).unwrap();
    std::fs::write(&store, b"saved-store").unwrap();
    std::fs::write(&disk, b"saved-disk").unwrap();
    let denied = call(&home, "machine.stop", &[("name", t("vm"))]).unwrap_err();
    assert_eq!(denied.code, "InUse");
    let stopped = call(
        &home,
        "machine.stop",
        &[("name", t("vm")), ("force", SdkValue::Bool(true))],
    )
    .unwrap();
    assert_eq!(stopped.get_text("state"), Some("stopped"));
    assert!(stopped.get_bool("forced"));
    assert_eq!(
        stopped.get("stopped_sandboxes"),
        Some(&SdkValue::List(vec![t("one"), t("two")]))
    );
    assert_eq!(
        stopped.get("unknown"),
        Some(&SdkValue::List(vec![t("one:work"), t("two:work")]))
    );
    assert!(
        matches!(listener.accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock),
        "worker must never be contacted"
    );
    assert!(!paths::worker_record(&home, "vm").exists());
    assert!(paths::machine_record(&home, "vm").exists());
    let sandbox = call(&home, "sandbox.get", &[("name", t("one"))]).unwrap();
    assert_eq!(sandbox.get_text("state"), Some("gone"));
    let logs = std::fs::read_to_string(root.join("calls")).unwrap();
    assert_eq!(
        logs.lines()
            .filter(|line| line.starts_with("stop --force"))
            .count(),
        1
    );
    assert!(!logs.contains("start") && !logs.contains("delete"));
    assert_eq!(std::fs::read(&store).unwrap(), b"saved-store");
    assert_eq!(
        std::fs::read(paths::store_record(&home, "shop")).unwrap(),
        store_record
    );
    assert_eq!(std::fs::read(&disk).unwrap(), b"saved-disk");
    assert!(read_record(&paths::machine_record(&home, "vm"))
        .unwrap()
        .unwrap()
        .get("op")
        .is_none());
    let again = call(
        &home,
        "machine.stop",
        &[("name", t("vm")), ("force", SdkValue::Bool(true))],
    )
    .unwrap();
    assert_eq!(again.get_text("state"), Some("stopped"));
    // A failed later provider operation retains worker/state and explicit unresolved custody.
    write_record(&paths::worker_record(&home, "vm"), &worker).unwrap();
    std::fs::write(root.join("refuse"), b"fail once").unwrap();
    let before = std::fs::read(paths::worker_record(&home, "vm")).unwrap();
    let failed = call(
        &home,
        "machine.stop",
        &[("name", t("vm")), ("force", SdkValue::Bool(true))],
    )
    .unwrap_err();
    assert_eq!(failed.code, "Failed");
    assert_eq!(
        std::fs::read(paths::worker_record(&home, "vm")).unwrap(),
        before
    );
    let uncertain = call(&home, "sandbox.list", &[]).unwrap();
    assert!(format!("{uncertain:?}").contains("unresolved"));
    let machine = read_record(&paths::machine_record(&home, "vm"))
        .unwrap()
        .unwrap();
    assert!(machine.get("op").unwrap().get_bool("uncertain"));
    // Only a fresh explicit request reconciles the uncertain stop; no internal retry occurred.
    std::fs::remove_file(root.join("refuse")).unwrap();
    let recovered = call(
        &home,
        "machine.stop",
        &[("name", t("vm")), ("force", SdkValue::Bool(true))],
    )
    .unwrap();
    assert_eq!(recovered.get_text("state"), Some("stopped"));
    assert_eq!(
        call(&home, "sandbox.get", &[("name", t("one"))])
            .unwrap()
            .get_text("state"),
        Some("gone")
    );
    let native = call(
        &home,
        "machine.stop",
        &[("name", t("native")), ("force", SdkValue::Bool(true))],
    )
    .unwrap();
    assert_eq!(native.get_text("state"), Some("ready"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn native_force_stop_refused_before_any_signal_leaves_no_uncertain_op() {
    let root = std::env::temp_dir().join(format!("lfs-force-pre-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let home = root.join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    home::open(&home).unwrap();
    // A started Sandbox whose runtime record is missing: the stop is refused
    // before anything is signalled, so nothing about it is uncertain, and the
    // stop goes on to the next Sandbox (one never started, so nothing runs).
    for (name, id) in [("a-lost", Some("a-lost")), ("b-idle", None)] {
        let mut fields = vec![
            ("name", t(name)),
            ("machine", t("native")),
            ("project", t("shop/app")),
        ];
        if let Some(id) = id {
            fields.push(("id", t(id)));
        }
        let record = SdkValue::Map(
            fields
                .into_iter()
                .map(|(field, value)| (field.to_owned(), value))
                .collect(),
        );
        write_record(&paths::sandbox_record(&home, name), &record).unwrap();
    }
    let refused = call(
        &home,
        "machine.stop",
        &[("name", t("native")), ("force", SdkValue::Bool(true))],
    )
    .unwrap_err();
    assert_eq!(refused.code, "Unavailable", "{refused:?}");
    assert_eq!(
        refused.detail.get("refused_sandboxes"),
        Some(&SdkValue::List(vec![t("a-lost")]))
    );
    assert_eq!(
        refused.detail.get("stopped_sandboxes"),
        Some(&SdkValue::List(vec![t("b-idle")]))
    );
    let op = read_record(&paths::machine_record(&home, "native"))
        .unwrap()
        .and_then(|record| record.get("op").cloned());
    assert!(
        op.is_none(),
        "a refusal before any signal left an op on native: {op:?}"
    );
    let sandbox = read_record(&paths::sandbox_record(&home, "a-lost"))
        .unwrap()
        .unwrap();
    for field in ["machine_stopped", "machine_stop_unknown"] {
        assert!(
            sandbox.get(field).is_none(),
            "an unsignalled Sandbox was marked {field}: {sandbox:?}"
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
