//! `layerfs doctor` (#536 H7): a home with one of each problem gives one
//! finding each, naming the call that fixes it; a clean home is `ok` with
//! no findings; a layout 2 home reports what refuses its conversion; and
//! the doctor writes nothing in the home. A live fake daemon's capacity model
//! is explained as warnings, and its Capacity refusal is a warning, not an
//! error.
use layerfs_bridge::control::{
    AdmissionModel, Answer, Call, CapacityModel, DaemonPhase, DaemonStatus, Reply, Request,
    RetainedModel, SdkRefusal, SdkValue, CAPACITY_OPERATION,
};
use layerfs_bridge::duplex;
use layerfs_control::doctor::doctor;
use layerfs_home::home::{paths, private_dir, write_entity, write_record};
use layerfs_machine::worker::{read_frame, read_frame_within, write_frame, MAX_HELLO};
use std::collections::BTreeMap;
use std::net::{SocketAddr, TcpListener};
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
    layerfs_control::migrate::open(&home).unwrap();
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
fn a_store_folder_others_may_pass_through_is_an_error_naming_chmod_700() {
    // #534: the daemon refuses a Store folder with any group/other bit, so
    // doctor must not call 0701 fine there; other home folders keep 0701.
    let home = new_home("store-0701");
    layerfs_control::migrate::open(&home).unwrap();
    store(&home, "shop", sqlite("shop", "native"));
    let native = SdkValue::map([("name", t("native")), ("platform", t("native"))]);
    write_entity(&paths::machine_record(&home, "native"), &native).unwrap();
    let dir = paths::store_dir(&home, "shop");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o701)).unwrap();
    std::fs::set_permissions(home.join("store"), std::fs::Permissions::from_mode(0o701)).unwrap();
    let report = doctor(&home).unwrap();
    assert_eq!(report.get("ok"), Some(&SdkValue::Bool(false)), "{report:?}");
    let found = findings(&report);
    assert_eq!(found.len(), 1, "{found:?}");
    let (severity, kind, name, message, fix) = &found[0];
    assert_eq!(
        (severity.as_str(), kind.as_str(), name.as_str()),
        ("error", "store", "shop")
    );
    assert!(message.contains("0701"), "{message}");
    assert!(fix.contains("chmod 700"), "{fix}");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let report = doctor(&home).unwrap();
    assert_eq!(report.get("ok"), Some(&SdkValue::Bool(true)), "{report:?}");
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
    layerfs_control::migrate::open(&home).unwrap();
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
            "restore machine ghost's record for inspection and explicit cleanup",
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
    assert!(found
        .iter()
        .all(|finding| !finding.4.contains("machine connect")));
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

/// A partial copy on the machine a push names is reported without writing
/// anywhere: the doctor reads it through that worker's `stat`.
#[test]
fn a_remote_partial_copy_is_reported_without_modifying_home_or_worker() {
    let home = new_home("remote-incoming");
    layerfs_control::migrate::open(&home).unwrap();
    let root = new_home("remote-root");
    private_dir(&root).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let guest = root.clone();
    std::thread::spawn(move || {
        layerfs_control::worker_server::serve(listener, "k".to_owned(), guest)
    });
    let m1 = SdkValue::map([("name", t("box")), ("platform", t("lima"))]);
    write_entity(&paths::machine_record(&home, "box"), &m1).unwrap();
    let worker = SdkValue::map([("address", t(&address)), ("secret", t("k"))]);
    write_record(&paths::worker_record(&home, "box"), &worker).unwrap();

    // Store `shop` on native with a certain `store.push` intent to `box`, and
    // a partial `store.sqlite.incoming` under `box`'s worker root.
    let stale_push = SdkValue::map([
        ("id", t("op-00000000000000bb")),
        ("name", t("store.push")),
        ("phase", t("send")),
        (
            "args",
            SdkValue::map([("from", t("native")), ("to", t("box"))]),
        ),
        ("owner", SdkValue::map([("pid", SdkValue::Int(0))])),
        ("started", SdkValue::Int(1)),
        ("updated", SdkValue::Int(1)),
        ("uncertain", SdkValue::Bool(false)),
    ]);
    let mut shop = sqlite("shop", "native");
    shop.push(("op", stale_push));
    store(&home, "shop", shop);
    let remote_dir = paths::store_dir(&root, "shop");
    private_dir(&remote_dir).unwrap();
    std::fs::write(remote_dir.join("store.sqlite.incoming"), b"partial-bytes").unwrap();

    // the worker is up and answering before the trees are read
    let mut executor = layerfs_store::place::on_machine(&home, "box").unwrap();
    layerfs_store::sqlite::family::step_on(
        &home,
        &mut executor,
        "shop",
        layerfs_store::sqlite::transfer::step::StoreStep::Stat,
    )
    .unwrap();

    let before_home = tree(&home);
    let before_root = tree(&root);
    let report = doctor(&home).unwrap();
    assert_eq!(before_home, tree(&home), "the doctor wrote in the home");
    assert_eq!(
        before_root,
        tree(&root),
        "the doctor wrote in the worker root"
    );

    let found = findings(&report);
    assert!(
        found.iter().any(|(sev, kind, name, msg, fix)| {
            sev == "warning"
                && kind == "store"
                && name == "shop"
                && msg.contains("machine box holds a 13-byte partial copy (store.sqlite.incoming)")
                && fix.contains("`layerfs store push shop box`")
        }),
        "{found:#?}"
    );
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

/// A partial copy a certain failure left (#13 F1): the push's intent is
/// gone, so the record keeps the machine (`incoming_on`) and the doctor
/// reports the bytes once, naming the calls that clear it.
#[test]
fn a_partial_copy_a_certain_failure_left_is_reported_from_the_record() {
    let home = new_home("certain-incoming");
    layerfs_control::migrate::open(&home).unwrap();
    let root = new_home("certain-root");
    private_dir(&root).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let guest = root.clone();
    std::thread::spawn(move || {
        layerfs_control::worker_server::serve(listener, "k".to_owned(), guest)
    });
    let m1 = SdkValue::map([("name", t("box")), ("platform", t("lima"))]);
    write_entity(&paths::machine_record(&home, "box"), &m1).unwrap();
    let worker = SdkValue::map([("address", t(&address)), ("secret", t("k"))]);
    write_record(&paths::worker_record(&home, "box"), &worker).unwrap();

    // Store `shop` on native with no intent at all, and the machine a
    // certain `store.push` failure kept in place of that intent
    let mut shop = sqlite("shop", "native");
    shop.push(("incoming_on", t("box")));
    store(&home, "shop", shop);
    let remote_dir = paths::store_dir(&root, "shop");
    private_dir(&remote_dir).unwrap();
    std::fs::write(remote_dir.join("store.sqlite.incoming"), b"partial-bytes").unwrap();

    let mut executor = layerfs_store::place::on_machine(&home, "box").unwrap();
    layerfs_store::sqlite::family::step_on(
        &home,
        &mut executor,
        "shop",
        layerfs_store::sqlite::transfer::step::StoreStep::Stat,
    )
    .unwrap();

    let before_home = tree(&home);
    let before_root = tree(&root);
    let report = doctor(&home).unwrap();
    assert_eq!(before_home, tree(&home), "the doctor wrote in the home");
    assert_eq!(
        before_root,
        tree(&root),
        "the doctor wrote in the worker root"
    );

    let found = findings(&report);
    assert_eq!(found.len(), 1, "one finding for one copy: {found:#?}");
    let (severity, kind, name, message, fix) = &found[0];
    assert_eq!(
        (severity.as_str(), kind.as_str(), name.as_str()),
        ("warning", "store", "shop")
    );
    assert!(
        message.contains("machine box holds a 13-byte partial copy (store.sqlite.incoming)"),
        "{message}"
    );
    assert!(
        fix.contains("`layerfs store push shop box` replaces it, or `layerfs store remove shop`"),
        "{fix}"
    );
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

/// Exec record `id` of Workspace `wsid` in Sandbox `sb`, owned by `pid`.
fn exec_record(home: &Path, wsid: &str, id: &str, pid: i64) -> PathBuf {
    let file = paths::workspace_dir(home, "sb")
        .join(wsid)
        .join("exec")
        .join(format!("{id}.json"));
    let record = SdkValue::map([
        ("id", t(id)),
        ("workspace", t(wsid)),
        ("path", t("/w/1")),
        ("name", t("w")),
        ("owner", SdkValue::map([("pid", SdkValue::Int(pid))])),
        ("route", t("local")),
        ("stop", SdkValue::Null),
        ("started", SdkValue::Int(1)),
    ]);
    write_entity(&file, &record).unwrap();
    file
}

#[test]
fn exec_records_are_checked_and_left_as_they_are() {
    let home = new_home("execs");
    layerfs_control::migrate::open(&home).unwrap();
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
    let (live, left) = ("a".repeat(64), "b".repeat(64));
    let workspace = SdkValue::map([("id", t(&live)), ("path", t("/w/1")), ("name", t("w"))]);
    let file = paths::workspace_dir(&home, "sb").join(format!("{live}.json"));
    write_entity(&file, &workspace).unwrap();
    // a live command's record: no finding
    exec_record(&home, &live, "e1", i64::from(std::process::id()));
    let report = doctor(&home).unwrap();
    assert!(findings(&report).is_empty(), "{report:?}");
    // a stale one (its owner gone), and a folder its Workspace record left
    exec_record(&home, &live, "e2", 99_999_999);
    exec_record(&home, &left, "e3", i64::from(std::process::id()));
    let before = tree(&home);
    let report = doctor(&home).unwrap();
    assert_eq!(before, tree(&home), "the doctor wrote in the home");
    let found = findings(&report);
    assert_eq!(found.len(), 2, "{found:?}");
    let stale = found.iter().find(|f| f.1 == "exec").unwrap();
    assert_eq!(
        (stale.0.as_str(), stale.2.as_str()),
        ("warning", &*format!("sb:{live}/e2"))
    );
    assert!(
        stale.3.contains("stale") && stale.4.contains("exec-cancel sb:w e2"),
        "{stale:?}"
    );
    let leftover = found.iter().find(|f| f.1 == "workspace").unwrap();
    assert_eq!(leftover.2, format!("sb:{left}"));
    assert!(leftover.3.contains("no Workspace record"), "{leftover:?}");
    // one that is not a record is an error
    std::fs::write(
        paths::workspace_dir(&home, "sb")
            .join(&live)
            .join("exec/e4.json"),
        "{}\n",
    )
    .unwrap();
    let before = tree(&home);
    let report = doctor(&home).unwrap();
    assert_eq!(before, tree(&home), "the doctor wrote in the home");
    assert_eq!(report.get("ok"), Some(&SdkValue::Bool(false)), "{report:?}");
    assert!(findings(&report)
        .iter()
        .any(|f| f.0 == "error" && f.3.contains("not an exec record")));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_stalled_daemon_endpoint_is_a_bounded_warning_and_starts_nothing() {
    let home = new_home("stalled");
    layerfs_control::migrate::open(&home).unwrap();
    store(&home, "shop", sqlite("shop", "native"));
    let project = SdkValue::map([("name", t("api"))]);
    write_entity(&paths::project_record(&home, "shop", "api"), &project).unwrap();
    let native = SdkValue::map([("name", t("native")), ("platform", t("native"))]);
    write_entity(&paths::machine_record(&home, "native"), &native).unwrap();
    let sandbox = SdkValue::map([
        ("name", t("sb")),
        ("project", t("shop/api")),
        ("machine", t("native")),
        ("id", t("sb")),
    ]);
    write_entity(&paths::sandbox_record(&home, "sb"), &sandbox).unwrap();
    // a daemon endpoint that accepts and then says nothing: the connect
    // succeeds and only the bounded handshake read ends the query
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = listener.local_addr().unwrap();
    let (dialed, dialed_rx) = mpsc::channel();
    std::thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            let _ = dialed.send(());
            std::thread::sleep(Duration::from_secs(6));
            drop(stream);
        }
    });
    let keys = SdkValue::map([
        ("endpoint", t(&endpoint.to_string())),
        ("secret", SdkValue::Bytes(vec![1; 32])),
        ("peer", SdkValue::Bytes(vec![2; 32])),
        ("instance", SdkValue::Bytes(vec![3; 32])),
    ]);
    write_record(&paths::sandbox_control(&home, "sb"), &keys).unwrap();

    let before = tree(&home);
    let at = std::time::Instant::now();
    let report = doctor(&home).unwrap();
    let elapsed = at.elapsed();
    assert!(
        elapsed < Duration::from_secs(4),
        "the doctor stalled for {elapsed:?}"
    );
    assert!(
        elapsed >= Duration::from_secs(1),
        "the doctor did not wait for its bounded query: {elapsed:?}"
    );
    dialed_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("the doctor never dialed the endpoint");
    assert_eq!(before, tree(&home), "the doctor wrote in the home");
    // a daemon that does not answer is a warning, not an error: the doctor
    // stays read-only and does not fail on an idle or stopped daemon
    assert_eq!(report.get("ok"), Some(&SdkValue::Bool(true)), "{report:?}");
    let found = findings(&report);
    assert_eq!(found.len(), 1, "{found:#?}");
    let (severity, kind, name, message, fix) = &found[0];
    assert_eq!(
        (severity.as_str(), kind.as_str(), name.as_str()),
        ("warning", "sandbox", "sb")
    );
    assert!(message.contains(&endpoint.to_string()), "{message}");
    assert!(
        message.contains("backend.daemon.connection_failures"),
        "{message}"
    );
    assert!(message.contains("within 2s"), "{message}");
    assert!(fix.contains("sandbox get sb"), "{fix}");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_sandbox_without_a_recorded_endpoint_is_not_queried() {
    let home = new_home("noendpoint");
    layerfs_control::migrate::open(&home).unwrap();
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
    // daemon keys without an endpoint: nothing tells the doctor where to
    // dial, and it starts nothing to find out
    let keys = SdkValue::map([
        ("secret", SdkValue::Bytes(vec![1; 32])),
        ("peer", SdkValue::Bytes(vec![2; 32])),
        ("instance", SdkValue::Bytes(vec![3; 32])),
    ]);
    write_record(&paths::sandbox_control(&home, "sb"), &keys).unwrap();
    let at = std::time::Instant::now();
    let report = doctor(&home).unwrap();
    assert!(at.elapsed() < Duration::from_secs(1));
    assert_eq!(report.get("ok"), Some(&SdkValue::Bool(true)));
    assert!(findings(&report).is_empty(), "{report:?}");
    std::fs::remove_dir_all(home).unwrap();
}

/// A fake daemon that answers the doctor's two calls: its Hello, then the
/// control-capacity read model with `reply`.
fn capacity_daemon(reply: Reply) -> (SocketAddr, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = listener.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut channel = duplex::accept(
            stream,
            &DAEMON_PRIVATE,
            duplex::public_key(&CLIENT_PRIVATE).unwrap(),
        )
        .unwrap();
        let hello = Call::decode(channel.receive.receive().unwrap()).unwrap();
        assert!(matches!(hello.request, Request::Hello(_)));
        channel
            .send
            .send(
                &Answer {
                    id: hello.id,
                    reply: Reply::Hello(DaemonStatus {
                        instance: DAEMON_INSTANCE,
                        phase: DaemonPhase::ControlReady,
                        overlay_sqlite: Some("3.51.0".into()),
                        store_sqlite: Some("3.51.0".into()),
                    }),
                }
                .encode()
                .unwrap(),
            )
            .unwrap();
        let call = Call::decode(channel.receive.receive().unwrap()).unwrap();
        let id = call.id;
        let Request::Sdk(sdk) = call.request else {
            panic!("expected the capacity read model");
        };
        assert_eq!(sdk.method, CAPACITY_OPERATION);
        channel
            .send
            .send(&Answer { id, reply }.encode().unwrap())
            .unwrap();
    });
    (endpoint, worker)
}

const CLIENT_PRIVATE: [u8; 32] = [2; 32];
const DAEMON_PRIVATE: [u8; 32] = [1; 32];
const DAEMON_INSTANCE: [u8; 32] = [3; 32];

/// A home with one running Sandbox whose daemon keys name `endpoint`.
fn running_sandbox(home: &Path, endpoint: SocketAddr) {
    layerfs_control::migrate::open(home).unwrap();
    store(home, "shop", sqlite("shop", "native"));
    let project = SdkValue::map([("name", t("api"))]);
    write_entity(&paths::project_record(home, "shop", "api"), &project).unwrap();
    let native = SdkValue::map([("name", t("native")), ("platform", t("native"))]);
    write_entity(&paths::machine_record(home, "native"), &native).unwrap();
    let sandbox = SdkValue::map([
        ("name", t("sb")),
        ("project", t("shop/api")),
        ("machine", t("native")),
        ("id", t("sb")),
    ]);
    write_entity(&paths::sandbox_record(home, "sb"), &sandbox).unwrap();
    let keys = SdkValue::map([
        ("endpoint", t(&endpoint.to_string())),
        ("secret", SdkValue::Bytes(CLIENT_PRIVATE.to_vec())),
        (
            "peer",
            SdkValue::Bytes(duplex::public_key(&DAEMON_PRIVATE).unwrap().to_vec()),
        ),
        ("instance", SdkValue::Bytes(DAEMON_INSTANCE.to_vec())),
    ]);
    write_record(&paths::sandbox_control(home, "sb"), &keys).unwrap();
}

#[test]
fn capacity_model_warnings_name_retained_failures_and_an_exhausted_pool() {
    let model = layerfs_bridge::control::CapacityModel {
        admission: AdmissionModel {
            pool_vacant: 0,
            pool_running: 8,
            reserved_vacant: 1,
            reserved_running: 0,
        },
        retained: RetainedModel {
            limit: 64,
            ttl_ms: 3_600_000,
            count: 3,
            dropped: 2,
            recorded: 3,
            failures: Vec::new(),
        },
    };
    let (endpoint, worker) = capacity_daemon(Reply::Sdk(model.to_sdk()));
    let home = new_home("capacity");
    running_sandbox(&home, endpoint);
    let report = doctor(&home).unwrap();
    worker.join().unwrap();
    assert_eq!(report.get("ok"), Some(&SdkValue::Bool(true)), "{report:?}");
    let found = findings(&report);
    assert_eq!(found.len(), 2, "{found:#?}");
    let text: Vec<String> = found
        .iter()
        .map(|(_, _, _, message, fix)| format!("{message} {fix}"))
        .collect();
    assert!(text[0].contains("retained 3 of 64"), "{text:#?}");
    assert!(text[0].contains("2 dropped"), "{text:#?}");
    assert!(
        text[0].contains("backend.daemon.connection_failures"),
        "{text:#?}"
    );
    assert!(text[1].contains("pool_vacant 0"), "{text:#?}");
    assert!(text[1].contains("pool_running 8"), "{text:#?}");
    assert!(text[1].contains("Capacity"), "{text:#?}");
    assert!(text[1].contains("daemon_control_slot"), "{text:#?}");
    for (severity, kind, name, _, _) in &found {
        assert_eq!(
            (severity.as_str(), kind.as_str(), name.as_str()),
            ("warning", "sandbox", "sb")
        );
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_healthy_capacity_model_gives_no_finding() {
    let model = CapacityModel {
        admission: AdmissionModel {
            pool_vacant: 7,
            pool_running: 1,
            reserved_vacant: 1,
            reserved_running: 0,
        },
        retained: RetainedModel {
            limit: 64,
            ttl_ms: 3_600_000,
            count: 0,
            dropped: 0,
            recorded: 0,
            failures: Vec::new(),
        },
    };
    let (endpoint, worker) = capacity_daemon(Reply::Sdk(model.to_sdk()));
    let home = new_home("capacity-healthy");
    running_sandbox(&home, endpoint);
    let report = doctor(&home).unwrap();
    worker.join().unwrap();
    assert_eq!(report.get("ok"), Some(&SdkValue::Bool(true)));
    assert!(findings(&report).is_empty(), "{report:?}");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_capacity_refusal_is_a_warning_not_an_error() {
    let refusal = SdkRefusal {
        code: "Capacity".to_owned(),
        message: "the Sandbox's daemon has no vacant control slot".to_owned(),
        detail: SdkValue::map([
            ("resource", t("daemon_control_slot")),
            ("retained", SdkValue::Int(3)),
            ("pool_vacant", SdkValue::Int(0)),
            ("reserved_vacant", SdkValue::Int(1)),
            ("retryable", SdkValue::Bool(true)),
        ]),
    };
    let (endpoint, worker) = capacity_daemon(Reply::SdkRefused(refusal));
    let home = new_home("capacity-refused");
    running_sandbox(&home, endpoint);
    let report = doctor(&home).unwrap();
    worker.join().unwrap();
    assert_eq!(report.get("ok"), Some(&SdkValue::Bool(true)));
    let found = findings(&report);
    assert_eq!(found.len(), 1, "{found:#?}");
    let message = &found[0].3;
    assert!(message.contains("Capacity"), "{message}");
    assert!(message.contains("daemon_control_slot"), "{message}");
    assert!(message.contains("pool_vacant 0"), "{message}");
    assert!(message.contains("reserved_vacant 1"), "{message}");
    assert!(message.contains("retained 3"), "{message}");
    assert_eq!(found[0].0, "warning");
    std::fs::remove_dir_all(home).unwrap();
}
