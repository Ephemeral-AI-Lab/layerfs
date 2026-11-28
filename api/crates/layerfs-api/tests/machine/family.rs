//! The machine family (#533 step 6) over records: the built-in machines
//! appear on the first call, names are checked before any effect, and a
//! create that cannot start leaves no record. Real VMs run in step 19.
#![cfg(feature = "machine")]
use layerfs_api::home::{paths, read_record, write_record};
use layerfs_protocol::sdk::SdkValue;
use std::path::{Path, PathBuf};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-platform-{tag}-{}-{}-{}",
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

/// The call's long form (`detail`).
fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, (String, String)> {
    let detail = std::iter::once(("detail".to_owned(), SdkValue::Bool(true)));
    let args = SdkValue::Map(
        (args.iter().map(|(n, v)| ((*n).to_owned(), v.clone())))
            .chain(detail)
            .collect(),
    );
    layerfs_api::call(Some(crate::root::root_of(home)), name, args, &mut |_| {})
        .map_err(|r| (r.code, r.message))
}

#[test]
fn machines_over_records() {
    let _env = crate::env_lock();
    // no provider tool and no bundle: every create is refused before a VM.
    // Tools are found on PATH, so it and HOME point at an empty folder.
    let empty = tempdir("path");
    std::env::set_var("PATH", &empty);
    // only the stand-ins: no fallback folder after PATH
    std::env::set_var("LAYERFS_TOOL_FALLBACKS", "");
    std::env::set_var("HOME", &empty);
    std::env::set_var("LAYERFS_BUNDLE", empty.join("missing"));
    let home = tempdir("home");

    let list = call(&home, "machine.list", &[]).unwrap();
    let SdkValue::List(machines) = list.get("items").unwrap() else {
        panic!()
    };
    let names: Vec<_> = machines
        .iter()
        .map(|m| m.get_text("machine").unwrap())
        .collect();
    assert_eq!(names, ["docker", layerfs_home::names::HOST_MACHINE]);
    let supports = |machine: &SdkValue, word: &str| {
        let SdkValue::List(words) = machine.get("supports").unwrap() else {
            panic!("{machine:?}")
        };
        words.contains(&t(word))
    };
    for machine in machines {
        assert!(supports(machine, "sqlite"), "{machine:?}");
        // a worker log exists only where the machine's calls go to a
        // worker: `docker` here, never `native` (layerfs-project#178)
        let worker = machine.get_text("machine") == Some("docker");
        assert_eq!(supports(machine, "log"), worker, "{machine:?}");
        // whether it shares this computer's loopback is its platform
        // adapter's answer: native's always, docker's only on a Linux Engine
        let shared = !worker || cfg!(target_os = "linux");
        let loopback = if shared { "shared" } else { "apart" };
        assert_eq!(machine.get_text("loopback"), Some(loopback), "{machine:?}");
    }
    // the support table for this host, each row with its live probe
    // (`home get --with platforms`, `machine.platforms` before 2.0)
    let with = ("with", SdkValue::List(vec![t("platforms")]));
    let platforms = call(&home, "home.get", &[with]).unwrap();
    let SdkValue::List(rows) = platforms.get("platforms").unwrap().get("items").unwrap() else {
        panic!()
    };
    assert_eq!(rows.len(), 4);
    for row in rows {
        let created = row.get_text("how") != Some("built_in");
        if created && row.get("backend") != Some(&SdkValue::Null) {
            // no provider tool on PATH: a platform that needs one is not ready
            assert_eq!(row.get("ready"), Some(&SdkValue::Bool(false)), "{row:?}");
        }
    }

    let native = call(
        &home,
        "machine.get",
        &[("machine", t(layerfs_home::names::HOST_MACHINE))],
    )
    .unwrap();
    assert_eq!(native.get_text("platform"), Some("host"));
    assert_eq!(native.get_text("state"), Some("ready"));
    assert!(!supports(&native, "log"), "{native:?}");
    let docker = call(&home, "machine.get", &[("machine", t("docker"))]).unwrap();
    assert!(supports(&docker, "log"), "{docker:?}");
    assert_eq!(docker.get_text("state"), Some("stopped"), "never started");
    assert_eq!(
        call(&home, "machine.get", &[("machine", t("nope"))])
            .unwrap_err()
            .0,
        "NotFound"
    );

    // built-ins are never created, removed or resized
    let made = call(
        &home,
        "machine.create",
        &[
            ("name", t(layerfs_home::names::HOST_MACHINE)),
            ("platform", t("host")),
        ],
    );
    assert_eq!(made.unwrap_err().0, "InvalidInput");
    assert_eq!(
        call(&home, "machine.remove", &[("machine", t("docker"))])
            .unwrap_err()
            .0,
        "InvalidInput"
    );
    let resized = call(
        &home,
        "machine.update",
        &[
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
            ("cpus", SdkValue::Int(4)),
        ],
    );
    assert_eq!(resized.unwrap_err().0, "InvalidInput");
    let made = call(
        &home,
        "machine.create",
        &[("name", t("dev")), ("platform", t("ssh"))],
    );
    assert_eq!(made.unwrap_err().0, "Unsupported", "ssh was removed");

    // a create that cannot start leaves nothing behind
    let made = call(
        &home,
        "machine.create",
        &[("name", t("dev")), ("platform", t("lima"))],
    );
    assert!(made.is_err());
    assert!(!paths::machine_dir(&home, "dev").exists());

    // the docker machine's socket is a setting of its record
    let updated = call(
        &home,
        "machine.update",
        &[("machine", t("docker")), ("docker", t("/tmp/docker.sock"))],
    )
    .unwrap();
    assert_eq!(updated.get_text("outcome"), Some("updated"));
    assert_eq!(updated.get_text("docker"), Some("/tmp/docker.sock"));
    let docker = read_record(&paths::machine_record(&home, "docker"))
        .unwrap()
        .unwrap();
    assert_eq!(docker.get_text("docker"), Some("/tmp/docker.sock"));

    // a built-in machine's daemon: an absolute path to a file, shown by machine.get
    for bad in ["layerfs-daemon", "/no/such/layerfs-daemon"] {
        let (code, message) = call(
            &home,
            "machine.update",
            &[
                ("machine", t(layerfs_home::names::HOST_MACHINE)),
                ("daemon", t(bad)),
            ],
        )
        .unwrap_err();
        assert_eq!(code, "InvalidInput", "{bad}: {message}");
    }
    let daemon = empty.join("layerfs-daemon");
    std::fs::write(&daemon, "").unwrap();
    let daemon = daemon.display().to_string();
    call(
        &home,
        "machine.update",
        &[
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
            ("daemon", t(&daemon)),
        ],
    )
    .unwrap();
    let native = read_record(&paths::machine_record(
        &home,
        layerfs_home::names::HOST_MACHINE,
    ))
    .unwrap()
    .unwrap();
    assert_eq!(native.get_text("daemon"), Some(daemon.as_str()));
    let shown = call(
        &home,
        "machine.get",
        &[("machine", t(layerfs_home::names::HOST_MACHINE))],
    )
    .unwrap();
    assert_eq!(shown.get_text("daemon"), Some(daemon.as_str()));

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
    let (code, message) = call(&home, "machine.remove", &[("machine", t("box"))]).unwrap_err();
    assert_eq!(code, "InUse", "{message}");
    let got = call(&home, "machine.get", &[("machine", t("box"))]).unwrap();
    assert_eq!(got.get("stores"), Some(&SdkValue::List(vec![t("shop")])));
    // a list's count only beside a cut list (more than 20)
    assert_eq!(got.get("stores_count"), Some(&SdkValue::Null));
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(empty);
}
