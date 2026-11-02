//! The machine family (#533 step 6) over records: the built-in machines
//! appear on the first call, names are checked before any effect, and a
//! create that cannot start leaves no record. Real VMs run in step 19.
#![cfg(feature = "machine")]
use layerfs_api::calls;
use layerfs_api::home::{paths, read_record, write_record};
use layerfs_bridge::control::SdkValue;
use std::path::{Path, PathBuf};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-machine-{tag}-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
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
    calls::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

#[test]
fn machines_over_records() {
    // no provider tool and no bundle: every create is refused before a VM
    let empty = tempdir("path");
    std::env::set_var("PATH", &empty);
    std::env::set_var("LAYERFS_BUNDLE", empty.join("missing"));
    let home = tempdir("home");

    let list = call(&home, "machine.list", &[]).unwrap();
    let SdkValue::List(machines) = list.get("machines").unwrap() else {
        panic!()
    };
    let names: Vec<_> = machines
        .iter()
        .map(|m| m.get_text("name").unwrap())
        .collect();
    assert_eq!(names, ["docker", "native"]);
    assert_eq!(list.get("can_create"), None);
    for machine in machines {
        let supports = machine.get("supports").unwrap();
        assert!(supports.get("sqlite").is_some(), "{machine:?}");
    }
    // the support table for this host, each row with its live probe
    let platforms = call(&home, "machine.platforms", &[]).unwrap();
    let SdkValue::List(rows) = platforms.get("platforms").unwrap() else {
        panic!()
    };
    assert_eq!(rows.len(), 7);
    for row in rows {
        let created = row.get_text("how") != Some("built_in");
        if created && row.get("backend") != Some(&SdkValue::Null) {
            // no provider tool on PATH: a platform that needs one is not ready
            assert_eq!(row.get("ready"), Some(&SdkValue::Bool(false)), "{row:?}");
        }
    }

    let native = call(&home, "machine.get", &[("name", t("native"))]).unwrap();
    assert_eq!(native.get_text("platform"), Some("native"));
    assert_eq!(
        call(&home, "machine.get", &[("name", t("nope"))])
            .unwrap_err()
            .0,
        "NotFound"
    );

    // built-ins are never created, removed or resized
    let made = call(
        &home,
        "machine.create",
        &[("name", t("native")), ("platform", t("native"))],
    );
    assert_eq!(made.unwrap_err().0, "InvalidInput");
    assert_eq!(
        call(&home, "machine.remove", &[("name", t("docker"))])
            .unwrap_err()
            .0,
        "InvalidInput"
    );
    let resized = call(
        &home,
        "machine.update",
        &[("name", t("native")), ("cpus", SdkValue::Int(4))],
    );
    assert_eq!(resized.unwrap_err().0, "InvalidInput");
    let made = call(
        &home,
        "machine.create",
        &[("name", t("dev")), ("platform", t("ssh"))],
    );
    assert_eq!(made.unwrap_err().0, "InvalidInput", "ssh is connected");

    // a create that cannot start leaves nothing behind
    let made = call(
        &home,
        "machine.create",
        &[("name", t("dev")), ("platform", t("lima"))],
    );
    assert!(made.is_err());
    assert!(!paths::machine_dir(&home, "dev").exists());

    // the docker machine's socket is a setting of its record
    call(
        &home,
        "machine.update",
        &[("name", t("docker")), ("docker", t("/tmp/docker.sock"))],
    )
    .unwrap();
    let docker = read_record(&paths::machine_record(&home, "docker"))
        .unwrap()
        .unwrap();
    assert_eq!(docker.get_text("docker"), Some("/tmp/docker.sock"));

    // a machine with a Store is InUse
    write_record(
        &paths::machine_record(&home, "box"),
        &SdkValue::map([
            ("name", t("box")),
            ("platform", t("lima")),
            ("state", t("stopped")),
        ]),
    )
    .unwrap();
    write_record(
        &paths::store_record(&home, "shop"),
        &SdkValue::map([
            ("name", t("shop")),
            ("kind", t("sqlite")),
            ("machine", t("box")),
        ]),
    )
    .unwrap();
    let (code, message) = call(&home, "machine.remove", &[("name", t("box"))]).unwrap_err();
    assert_eq!(code, "InUse", "{message}");
    let got = call(&home, "machine.get", &[("name", t("box"))]).unwrap();
    assert_eq!(got.get("stores"), Some(&SdkValue::List(vec![t("shop")])));
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(empty);
}
