//! `layerfs doctor` (#536 H7): a home with one of each problem gives one
//! finding each, naming the call that fixes it; a clean home is `ok` with
//! no findings; a layout 2 home reports what refuses its conversion; and
//! the doctor writes nothing in the home.
use layerfs_bridge::control::SdkValue;
use layerfs_control::common::home::{self, paths, private_dir, write_entity, write_record};
use layerfs_control::doctor::doctor;
use layerfs_control::machine::worker::{read_frame, read_frame_within, write_frame, MAX_HELLO};
use std::collections::BTreeMap;
use std::net::TcpListener;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A path for a new home (not made yet).
fn new_home(tag: &str) -> PathBuf {
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("layerfs-doctor-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn sqlite(name: &str, machine: &str) -> Vec<(&'static str, SdkValue)> {
    vec![
        ("name", t(name)),
        ("kind", t("sqlite")),
        ("machine", t(machine)),
    ]
}

fn store(home: &Path, name: &str, fields: Vec<(&'static str, SdkValue)>) {
    write_entity(
        &paths::store_record(home, name),
        &SdkValue::Map(fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect()),
    )
    .unwrap();
}

fn op(uncertain: bool) -> SdkValue {
    SdkValue::map([
        ("id", t("op-00000000000000aa")),
        ("name", t("store.remove")),
        ("phase", t("retire")),
        ("args", SdkValue::Null),
        // a process id no kernel gives: its owner is gone
        ("owner", SdkValue::map([("pid", SdkValue::Int(0))])),
        ("started", SdkValue::Int(1)),
        ("updated", SdkValue::Int(1)),
        ("uncertain", SdkValue::Bool(uncertain)),
    ])
}

/// Every path under `dir` with its size, mtime and mode.
fn tree(dir: &Path) -> BTreeMap<PathBuf, (u64, i64, i64, u32)> {
    let mut found = BTreeMap::new();
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

/// The report's findings as `(severity, kind, name, message, fix)`.
fn findings(report: &SdkValue) -> Vec<(String, String, String, String, String)> {
    let Some(SdkValue::List(items)) = report.get("findings") else {
        panic!("no findings list: {report:?}");
    };
    let text = |v: &SdkValue, k: &str| v.get_text(k).unwrap_or("").to_owned();
    items
        .iter()
        .map(|f| {
            let entity = f.get("entity").cloned().unwrap_or_default();
            (
                text(f, "severity"),
                text(&entity, "kind"),
                text(&entity, "name"),
                text(f, "message"),
                text(f, "fix"),
            )
        })
        .collect()
}

/// A stand-in old worker (its hello names no protocol) that reports
/// whether it received a request after its hello.
fn old_worker() -> (String, mpsc::Receiver<Option<SdkValue>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let (sent, received) = mpsc::channel();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let _ = read_frame_within(&mut stream, MAX_HELLO);
        let ready = SdkValue::map([("ready", SdkValue::Bool(true))]);
        write_frame(&mut stream, &ready).unwrap();
        let _ = sent.send(read_frame(&mut stream).ok().flatten());
    });
    (address, received)
}

#[test]
fn a_clean_home_is_ok_with_no_findings() {
    let home = new_home("clean");
    home::open(&home).unwrap();
    store(&home, "shop", sqlite("shop", "native"));
    let project = SdkValue::map([("name", t("api"))]);
    write_entity(&paths::project_record(&home, "shop", "api"), &project).unwrap();
    let sandbox = SdkValue::map([
        ("name", t("sb")),
        ("project", t("shop/api")),
        ("machine", t("native")),
        ("id", t("sb")),
    ]);
    write_entity(&paths::sandbox_record(&home, "sb"), &sandbox).unwrap();
    let native = SdkValue::map([("name", t("native")), ("platform", t("native"))]);
    write_entity(&paths::machine_record(&home, "native"), &native).unwrap();
    let before = tree(&home);
    let report = doctor(&home).unwrap();
    assert_eq!(report.get("ok"), Some(&SdkValue::Bool(true)), "{report:?}");
    assert!(findings(&report).is_empty(), "{report:?}");
    assert_eq!(before, tree(&home), "the doctor wrote in the home");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_home_that_does_not_exist_is_ok_and_stays_absent() {
    let home = new_home("absent");
    let report = doctor(&home).unwrap();
    assert_eq!(report.get("ok"), Some(&SdkValue::Bool(true)));
    assert!(!home.exists());
}

#[test]
fn each_problem_gives_one_finding_naming_its_fix() {
    let home = new_home("problems");
    home::open(&home).unwrap();
    // a record that does not parse
    let bad = paths::store_record(&home, "bad");
    private_dir(bad.parent().unwrap()).unwrap();
    std::fs::write(&bad, "{not json").unwrap();
    std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o600)).unwrap();
    // a record of another version
    let old = SdkValue::map([
        ("v", SdkValue::Int(9)),
        ("name", t("old")),
        ("platform", t("lima")),
    ]);
    write_record(&paths::machine_record(&home, "old"), &old).unwrap();
    // a record others may read
    let bucket = paths::bucket_record(&home, "b1");
    write_entity(&bucket, &SdkValue::map([("name", t("b1"))])).unwrap();
    std::fs::set_permissions(&bucket, std::fs::Permissions::from_mode(0o644)).unwrap();
    // a Sandbox whose Store is gone
    let sandbox = SdkValue::map([
        ("name", t("sx")),
        ("project", t("gone/p")),
        ("machine", t("native")),
        ("id", t("sx")),
    ]);
    write_entity(&paths::sandbox_record(&home, "sx"), &sandbox).unwrap();
    // a Store on a machine with no record
    store(&home, "s2", sqlite("s2", "ghost"));
    // a project of a missing Store
    let orphan = SdkValue::map([("name", t("p"))]);
    write_entity(&paths::project_record(&home, "orphan", "p"), &orphan).unwrap();
    // an uncertain intent and a dead certain one
    let mut s3 = sqlite("s3", "native");
    s3.push(("op", op(true)));
    store(&home, "s3", s3);
    let mut s4 = sqlite("s4", "native");
    s4.push(("op", op(false)));
    store(&home, "s4", s4);
    // a stale lease (a process id above every kernel's pid_max)
    store(&home, "s5", sqlite("s5", "native"));
    private_dir(&paths::lease_dir(&home, "s5")).unwrap();
    std::fs::write(paths::lease_dir(&home, "s5").join("99999999"), "1\n").unwrap();
    // a partial copy an interrupted pull left on native
    store(&home, "s6", sqlite("s6", "ghost6"));
    let ghost6 = SdkValue::map([("name", t("ghost6")), ("platform", t("lima"))]);
    write_entity(&paths::machine_record(&home, "ghost6"), &ghost6).unwrap();
    std::fs::write(
        paths::store_dir(&home, "s6").join("store.sqlite.incoming"),
        "partial",
    )
    .unwrap();
    // a built-in machine whose recorded daemon is not there
    let native = SdkValue::map([
        ("name", t("native")),
        ("platform", t("native")),
        ("daemon", t("/no/such/layerfs-daemon")),
    ]);
    write_entity(&paths::machine_record(&home, "native"), &native).unwrap();
    // a running machine whose worker is of an older build
    let (address, received) = old_worker();
    let m1 = SdkValue::map([("name", t("m1")), ("platform", t("lima"))]);
    write_entity(&paths::machine_record(&home, "m1"), &m1).unwrap();
    let worker = SdkValue::map([("address", t(&address)), ("secret", t("k"))]);
    write_record(&paths::worker_record(&home, "m1"), &worker).unwrap();

    let before = tree(&home);
    let report = doctor(&home).unwrap();
    assert_eq!(before, tree(&home), "the doctor wrote in the home");
    let request = received.recv_timeout(Duration::from_secs(15)).unwrap();
    assert!(request.is_none(), "the worker got a request: {request:?}");
    assert_eq!(report.get("ok"), Some(&SdkValue::Bool(false)));

    let found = findings(&report);
    let expect = [
        ("error", "store", "bad", "does not parse", "by hand"),
        ("error", "machine", "old", "\"v\" 9", "build that wrote"),
        ("error", "bucket", "b1", "0644", "chmod 600"),
        (
            "error",
            "sandbox",
            "sx",
            "Store gone is gone",
            "`layerfs sandbox close sx`",
        ),
        (
            "error",
            "store",
            "s2",
            "machine ghost",
            "machine create ghost",
        ),
        (
            "error",
            "project",
            "orphan/p",
            "no Store record",
            "store/orphan",
        ),
        (
            "error",
            "store",
            "s3",
            "unresolved store.remove",
            "`layerfs store remove s3`",
        ),
        (
            "warning",
            "store",
            "s4",
            "certain outcome",
            "next operation",
        ),
        (
            "warning",
            "store",
            "s5",
            "stale lease of process 99999999",
            "`store/s5/open/99999999` by hand",
        ),
        (
            "error",
            "machine",
            "native",
            "recorded daemon /no/such/layerfs-daemon is not there",
            "`layerfs machine update native --daemon <path>`",
        ),
        (
            "warning",
            "store",
            "s6",
            "store.sqlite.incoming",
            "`layerfs store pull s6`",
        ),
        (
            "warning",
            "machine",
            "m1",
            "another build",
            "`layerfs machine stop m1`, then `layerfs machine start m1`; if the start says the Linux bundle was sealed before this build, rebuild it",
        ),
    ];
    for (severity, kind, name, message, fix) in expect {
        let matching: Vec<_> = (found.iter())
            .filter(|f| f.1 == kind && f.2 == name)
            .collect();
        assert_eq!(matching.len(), 1, "{kind} {name}: {found:#?}");
        let (s, _, _, m, x) = matching[0];
        assert_eq!(s, severity, "{kind} {name}: {m}");
        assert!(m.contains(message), "{kind} {name}: {m}");
        assert!(x.contains(fix), "{kind} {name}: {x}");
    }
    assert_eq!(found.len(), expect.len(), "{found:#?}");
    std::fs::remove_dir_all(home).unwrap();
}

/// #551 B10-179: an old or hand-edited record whose request is stronger
/// than what its machine provides is one warning, naming the call that
/// recreates it; the doctor writes nothing.
#[test]
fn a_sandbox_asking_for_more_isolation_than_its_machine_warns() {
    let home = new_home("isolation");
    home::open(&home).unwrap();
    let docker = SdkValue::map([("name", t("docker")), ("platform", t("docker"))]);
    write_entity(&paths::machine_record(&home, "docker"), &docker).unwrap();
    store(
        &home,
        "pg",
        vec![
            ("name", t("pg")),
            ("kind", t("postgresql")),
            ("url", t("postgres://u@127.0.0.1:1/db")),
        ],
    );
    let project = SdkValue::map([("name", t("app"))]);
    write_entity(&paths::project_record(&home, "pg", "app"), &project).unwrap();
    let sandbox = SdkValue::map([
        ("name", t("t")),
        ("project", t("pg/app")),
        ("machine", t("docker")),
        ("id", t("t")),
        ("requested_isolation", t("vm")),
        ("effective_isolation", t("container")),
    ]);
    write_entity(&paths::sandbox_record(&home, "t"), &sandbox).unwrap();
    let before = tree(&home);
    let report = doctor(&home).unwrap();
    assert_eq!(before, tree(&home), "the doctor wrote in the home");
    let found = findings(&report);
    assert_eq!(found.len(), 1, "{found:?}");
    let (severity, kind, name, message, fix) = &found[0];
    assert_eq!(severity, "warning", "{found:?}");
    assert_eq!(
        (kind.as_str(), name.as_str()),
        ("sandbox", "t"),
        "{found:?}"
    );
    assert!(
        message.contains("requested vm isolation, but machine docker provides container"),
        "{message}"
    );
    assert!(
        fix.contains("`layerfs sandbox close t`")
            && fix.contains("`layerfs sandbox.create pg/app t <machine>`"),
        "{fix}"
    );
    std::fs::remove_dir_all(home).unwrap();
}

/// #551 B10-178: a machine record that declares a class its platform does
/// not provide on this host is one warning; the read model derives the
/// class, so only a record that disagrees warns.
#[cfg(feature = "machine")]
#[test]
fn a_machine_record_that_declares_another_class_warns() {
    let home = new_home("declared");
    home::open(&home).unwrap();
    let docker = SdkValue::map([
        ("name", t("docker")),
        ("platform", t("docker")),
        ("isolation_class", t("process")),
    ]);
    write_entity(&paths::machine_record(&home, "docker"), &docker).unwrap();
    let before = tree(&home);
    let report = doctor(&home).unwrap();
    assert_eq!(before, tree(&home), "the doctor wrote in the home");
    let found = findings(&report);
    assert_eq!(found.len(), 1, "{found:?}");
    let (severity, kind, name, message, fix) = &found[0];
    assert_eq!(severity, "warning", "{found:?}");
    assert_eq!(
        (kind.as_str(), name.as_str()),
        ("machine", "docker"),
        "{found:?}"
    );
    assert!(
        message.contains("declares process isolation, but docker provides container"),
        "{message}"
    );
    assert!(fix.contains("repair"), "{fix}");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_layout_2_home_reports_what_refuses_its_conversion_and_converts_nothing() {
    let home = new_home("layout2");
    private_dir(&home).unwrap();
    let file = SdkValue::map([
        ("layout", SdkValue::Int(2)),
        ("id", t("0123456789abcdef0123456789abcdef")),
    ]);
    write_record(&home.join("layerfs.json"), &file).unwrap();
    // a live process holds a Store open
    let mut holder = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .unwrap();
    let leases = paths::lease_dir(&home, "shop");
    private_dir(&leases).unwrap();
    std::fs::write(leases.join(holder.id().to_string()), "1\n").unwrap();
    // and a dead one held another: its stale lease stays
    private_dir(&paths::lease_dir(&home, "old")).unwrap();
    let stale = paths::lease_dir(&home, "old").join("99999999");
    std::fs::write(&stale, "1\n").unwrap();

    let before = tree(&home);
    let report = doctor(&home);
    let _ = holder.kill();
    let _ = holder.wait();
    let report = report.unwrap();
    assert_eq!(before, tree(&home), "the doctor wrote in the home");
    assert_eq!(report.get("ok"), Some(&SdkValue::Bool(false)));
    let found = findings(&report);
    assert_eq!(found.len(), 2, "{found:#?}");
    let (severity, kind, name, message, fix) = &found[0];
    assert_eq!(
        (severity.as_str(), kind.as_str(), name.as_str()),
        ("error", "store", "shop")
    );
    assert!(message.contains(&holder.id().to_string()), "{message}");
    assert!(fix.contains("end that process"), "{fix}");
    assert_eq!(found[1].0, "warning");
    assert!(found[1].3.contains("layout 2"), "{found:#?}");
    assert!(stale.exists());
    std::fs::remove_dir_all(home).unwrap();
}

/// #551 B10-171: a declared Docker socket that is not there is an error
/// naming the call that records the right one.
#[test]
fn a_declared_docker_socket_that_is_not_there_is_an_error() {
    let home = new_home("docker-socket");
    home::open(&home).unwrap();
    let docker = SdkValue::map([
        ("name", t("docker")),
        ("platform", t("docker")),
        ("docker", t("/no/such/docker.sock")),
    ]);
    write_entity(&paths::machine_record(&home, "docker"), &docker).unwrap();
    let before = tree(&home);
    let report = doctor(&home).unwrap();
    assert_eq!(before, tree(&home), "the doctor wrote in the home");
    let found = findings(&report);
    assert_eq!(found.len(), 1, "{found:?}");
    let (severity, kind, name, message, fix) = &found[0];
    assert_eq!(severity, "error", "{found:?}");
    assert_eq!((kind.as_str(), name.as_str()), ("machine", "docker"));
    assert!(message.contains("/no/such/docker.sock"), "{message}");
    assert!(fix.contains("machine update docker --docker"), "{fix}");
    std::fs::remove_dir_all(home).unwrap();
}

/// #551 B10-173: a Sandbox image that is a file must exist.
#[test]
fn a_sandbox_image_file_that_is_missing_is_an_error() {
    let home = new_home("image");
    home::open(&home).unwrap();
    store(&home, "shop", sqlite("shop", "native"));
    write_entity(
        &paths::project_record(&home, "shop", "api"),
        &SdkValue::map([("name", t("api"))]),
    )
    .unwrap();
    let native = SdkValue::map([("name", t("native")), ("platform", t("native"))]);
    write_entity(&paths::machine_record(&home, "native"), &native).unwrap();
    let sandbox = SdkValue::map([
        ("name", t("img")),
        ("project", t("shop/api")),
        ("machine", t("native")),
        ("id", t("img")),
        ("image", t("/no/such/image.tar")),
    ]);
    write_entity(&paths::sandbox_record(&home, "img"), &sandbox).unwrap();
    let report = doctor(&home).unwrap();
    let found = findings(&report);
    assert_eq!(found.len(), 1, "{found:?}");
    let (severity, kind, name, message, fix) = &found[0];
    assert_eq!(severity, "error", "{found:?}");
    assert_eq!((kind.as_str(), name.as_str()), ("sandbox", "img"));
    assert!(message.contains("/no/such/image.tar"), "{message}");
    assert!(fix.contains("sandbox.create"), "{fix}");
    std::fs::remove_dir_all(home).unwrap();
}

/// #551 B10-174: a recorded process Sandbox on a host that lacks the mount
/// driver is one warning, naming FUSE.
#[cfg(feature = "machine")]
#[test]
fn a_process_sandbox_without_its_mount_driver_warns() {
    let missing = layerfs_machine::native_missing(layerfs_machine::HostOs::current());
    if missing.is_empty() {
        return; // this host has every driver LayerFS can check
    }
    let home = new_home("driver");
    home::open(&home).unwrap();
    store(&home, "shop", sqlite("shop", "native"));
    write_entity(
        &paths::project_record(&home, "shop", "api"),
        &SdkValue::map([("name", t("api"))]),
    )
    .unwrap();
    let native = SdkValue::map([("name", t("native")), ("platform", t("native"))]);
    write_entity(&paths::machine_record(&home, "native"), &native).unwrap();
    let sandbox = SdkValue::map([
        ("name", t("proc")),
        ("project", t("shop/api")),
        ("machine", t("native")),
        ("id", t("proc")),
        ("requested_isolation", t("process")),
        ("effective_isolation", t("process")),
    ]);
    write_entity(&paths::sandbox_record(&home, "proc"), &sandbox).unwrap();
    let report = doctor(&home).unwrap();
    let found = findings(&report);
    let driver = found
        .iter()
        .find(|(_, kind, _, message, _)| kind == "home" && message.contains("mount driver"));
    let (severity, _, _, message, fix) = driver.unwrap_or_else(|| panic!("{found:?}"));
    assert_eq!(severity, "warning", "{found:?}");
    assert!(message.contains("host:"), "{message}");
    assert!(fix.contains("FUSE"), "{fix}");
    std::fs::remove_dir_all(home).unwrap();
}

/// #551 B10-176: a machine recorded running with no worker record is a
/// warning naming `machine start`; a stopped machine that left one is a
/// warning too.
#[test]
fn a_machine_worker_state_that_disagrees_warns() {
    let home = new_home("worker");
    home::open(&home).unwrap();
    let docker = SdkValue::map([
        ("name", t("docker")),
        ("platform", t("docker")),
        ("state", t("running")),
    ]);
    write_entity(&paths::machine_record(&home, "docker"), &docker).unwrap();
    let report = doctor(&home).unwrap();
    let found = findings(&report);
    let missing = found
        .iter()
        .find(|(_, _, name, message, _)| name == "docker" && message.contains("no worker record"));
    let (severity, _, _, message, fix) = missing.unwrap_or_else(|| panic!("{found:?}"));
    assert_eq!(severity, "warning", "{found:?}");
    assert!(message.contains("no worker record"), "{message}");
    assert!(fix.contains("machine start docker"), "{fix}");

    // a stopped machine that left its worker record
    let dev = SdkValue::map([
        ("name", t("dev")),
        ("platform", t("lima")),
        ("state", t("stopped")),
    ]);
    write_entity(&paths::machine_record(&home, "dev"), &dev).unwrap();
    let worker = SdkValue::map([("address", t("127.0.0.1:1")), ("secret", t("k"))]);
    write_record(&paths::worker_record(&home, "dev"), &worker).unwrap();
    let report = doctor(&home).unwrap();
    let found = findings(&report);
    let stale = found
        .iter()
        .find(|(_, _, name, message, _)| name == "dev" && message.contains("worker record remains"));
    assert!(stale.is_some(), "{found:?}");
    std::fs::remove_dir_all(home).unwrap();
}
