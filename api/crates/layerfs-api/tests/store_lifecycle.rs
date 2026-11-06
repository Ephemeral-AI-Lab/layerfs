//! Store lifecycle races and machines (#533 review, group store; #536
//! §7.1, §7.3): a lease and a push or remove never overlap, even inside the
//! pusher's own process; a live `op` refuses `Busy`, a gone process's
//! uncertain one `Unresolved` until a new push or `store remove` clears it, its certain
//! one nothing; `store.status` shows the `op` and starts no machine;
//! `store.remove` refuses a Store whose machine is unknown and never starts
//! a machine under the home lock; push and pull refuse the wrong side and
//! a stopped machine, naming `machine start`. All from records: no Store is opened, so
//! these run on any host.
use layerfs_api::home::lease;
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_bridge::control::SdkValue;
use layerfs_isolation::local::LocalProcess;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-life-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
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
    layerfs_api::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

fn machine(home: &Path, name: &str, platform: &str) {
    write_record(
        &paths::machine_record(home, name),
        &SdkValue::map([("name", t(name)), ("platform", t(platform))]),
    )
    .unwrap();
}

/// A home with `native`, a worker machine `box` served in this process,
/// and Store `shop` created on native with project `api` recorded.
fn home_with_shop() -> (PathBuf, PathBuf) {
    let home = tempdir("home");
    home::open(&home).unwrap();
    machine(&home, "native", "native");
    machine(&home, "box", "lima");
    let root = tempdir("guest");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let guest = root.clone();
    std::thread::spawn(move || layerfs_api::worker::serve(listener, "k".to_owned(), guest));
    write_record(
        &paths::worker_record(&home, "box"),
        &SdkValue::map([("address", t(&address)), ("secret", t("k"))]),
    )
    .unwrap();
    call(&home, "store.create", &[("name", t("shop"))]).unwrap();
    write_record(
        &paths::project_record(&home, "shop", "api"),
        &SdkValue::map([("name", t("api"))]),
    )
    .unwrap();
    (home, root)
}

fn record(home: &Path) -> SdkValue {
    read_record(&paths::store_record(home, "shop"))
        .unwrap()
        .unwrap()
}

fn set_op(home: &Path, value: SdkValue) {
    let path = paths::store_record(home, "shop");
    write_record(&path, &home::with_field(record(home), "op", value)).unwrap();
}

/// A `store.push` intent of this process, its start time moved by `skew`
/// (a moved start time names a process that is gone).
fn this_process_pushing(skew: u64, uncertain: bool) -> SdkValue {
    let me = LocalProcess::observe(std::process::id()).unwrap();
    SdkValue::map([
        ("id", t("op-00000000000000ab")),
        ("name", t("store.push")),
        ("phase", t("record")),
        (
            "args",
            SdkValue::map([("from", t("native")), ("to", t("box"))]),
        ),
        (
            "owner",
            SdkValue::map([
                ("pid", SdkValue::Int(i64::from(std::process::id()))),
                ("pid_start", t(&(me.start + skew).to_string())),
                ("pid_boot", t(&format!("{:032x}", me.boot.unwrap()))),
            ]),
        ),
        ("started", SdkValue::Int(1)),
        ("updated", SdkValue::Int(2)),
        ("uncertain", SdkValue::Bool(uncertain)),
    ])
}

fn read(home: &Path) -> Result<SdkValue, (String, String)> {
    call(
        home,
        "file.read",
        &[
            ("project", t("shop/api")),
            ("branch", t("main")),
            ("path", t("/a")),
        ],
    )
}

#[test]
fn a_store_call_during_a_push_is_busy_even_from_the_pushers_process() {
    let (home, root) = home_with_shop();
    // a push this process runs: its other threads' calls and leases wait
    set_op(&home, this_process_pushing(0, false));
    let (code, message) = read(&home).unwrap_err();
    assert_eq!(code, "Busy", "{message}");
    assert!(message.contains("store.push"), "{message}");
    assert_eq!(
        lease::hold(&home, "shop").err().map(|r| r.code).as_deref(),
        Some("Busy")
    );
    assert_eq!(
        call(&home, "store.remove", &[("name", t("shop"))])
            .unwrap_err()
            .0,
        "Busy"
    );
    assert!(
        !paths::lease_dir(&home, "shop")
            .join(std::process::id().to_string())
            .exists(),
        "no lease was taken"
    );
    let status = call(&home, "store.status", &[("name", t("shop"))]).unwrap();
    assert_eq!(status.get_text("state"), Some("busy"));
    let op = status.get("op").unwrap();
    assert_eq!(op.get_text("name"), Some("store.push"));
    assert_eq!(op.get_text("phase"), Some("record"));
    assert!(op.get("owner").is_none(), "{op:?}");

    // the same id with another start time is a pusher that is gone: after a
    // certain failure its intent holds nothing
    set_op(&home, this_process_pushing(1, false));
    let refused = read(&home).err().map(|(code, _)| code);
    assert_ne!(refused.as_deref(), Some("Busy"));
    drop(lease::hold(&home, "shop").unwrap());
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn taking_a_lease_removes_the_leases_of_processes_that_ended_and_keeps_live_ones() {
    let (home, root) = home_with_shop();
    let dir = paths::lease_dir(&home, "shop");
    std::fs::create_dir_all(&dir).unwrap();
    // a process id above every kernel's pid_max ended; process 1 is alive
    std::fs::write(dir.join("99999999"), "1\n").unwrap();
    std::fs::write(dir.join("1"), "1\n").unwrap();
    let status = call(&home, "store.status", &[("name", t("shop"))]).unwrap();
    assert!(
        dir.join("99999999").exists(),
        "a read removed a lease: {status:?}"
    );
    let held = lease::hold(&home, "shop").unwrap();
    assert!(!dir.join("99999999").exists(), "the stale lease stayed");
    assert!(dir.join("1").exists(), "a live holder's lease was removed");
    let log = std::fs::read_to_string(home.join("log").join("leases.jsonl")).unwrap();
    assert!(log.contains("\"removed_stale_lease_of\":99999999"), "{log}");
    drop(held);
    std::fs::remove_file(dir.join("1")).unwrap();
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_gone_pushers_uncertain_op_is_unresolved_until_store_remove_clears_it() {
    let (home, root) = home_with_shop();
    set_op(&home, this_process_pushing(1, true));
    let (code, message) = read(&home).unwrap_err();
    assert_eq!(code, "Unresolved", "{message}");
    // a new push is the call that clears a push's intent (H6; the push
    // that does it is in store_transfer_crash.rs)
    for named in [
        "`layerfs store status shop`",
        "`layerfs store push shop box`",
    ] {
        assert!(message.contains(named), "{named} not in {message}");
    }
    assert_eq!(
        lease::hold(&home, "shop").err().map(|r| r.code).as_deref(),
        Some("Unresolved")
    );
    let status = call(&home, "store.status", &[("name", t("shop"))]).unwrap();
    assert_eq!(status.get_text("state"), Some("unresolved"));
    assert_eq!(
        status.get("op").and_then(|op| op.get("uncertain")),
        Some(&SdkValue::Bool(true))
    );
    // the clearing call is not refused by what it clears
    call(&home, "store.remove", &[("name", t("shop"))]).unwrap();
    assert!(!paths::store_dir(&home, "shop").exists());
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_push_or_remove_waits_for_a_lease_another_thread_of_its_process_holds() {
    let (home, root) = home_with_shop();
    let held = lease::hold(&home, "shop").unwrap();
    for (name, args) in [
        (
            "store.push",
            vec![("name", t("shop")), ("machine", t("box"))],
        ),
        ("store.remove", vec![("name", t("shop"))]),
    ] {
        let (code, message) = call(&home, name, &args).unwrap_err();
        assert_eq!(code, "InUse", "{name}: {message}");
        assert!(message.contains("this process"), "{name}: {message}");
        // nothing was sent: the refusal is certain, so `op` is dropped
        assert_eq!(record(&home).get("op"), None);
        assert_eq!(record(&home).get_text("machine"), Some("native"));
        assert!(paths::store_file(&home, "shop").is_file());
    }
    drop(held);
    call(
        &home,
        "store.push",
        &[("name", t("shop")), ("machine", t("box"))],
    )
    .unwrap();
    call(&home, "store.remove", &[("name", t("shop"))]).unwrap();
    assert!(!paths::store_dir(&home, "shop").exists());
    assert!(!paths::store_dir(&root, "shop").exists());
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

/// A home whose Store `shop` is on machine `vm` (a lima VM that is not
/// running: no worker recorded) or on an unknown machine (`Null`).
fn home_with_stopped(machine_field: SdkValue) -> PathBuf {
    let home = tempdir("stopped");
    home::open(&home).unwrap();
    machine(&home, "native", "native");
    machine(&home, "vm", "lima");
    write_record(
        &paths::store_record(&home, "shop"),
        &SdkValue::map([
            ("name", t("shop")),
            ("kind", t("sqlite")),
            ("binding", SdkValue::Bytes(vec![7; 16])),
            ("cursor", SdkValue::Bytes(vec![8; 32])),
            ("profile", t("disposable")),
            ("machine", machine_field),
        ]),
    )
    .unwrap();
    home
}

#[test]
fn status_of_a_store_on_a_stopped_machine_starts_nothing_and_says_down() {
    let home = home_with_stopped(t("vm"));
    let status = call(&home, "store.status", &[("name", t("shop"))]).unwrap();
    assert_eq!(status.get_text("state"), Some("down"));
    let reason = status.get_text("down_reason").unwrap_or("");
    assert!(reason.contains("not running"), "{reason}");
    assert!(
        !paths::machine_dir(&home, "vm").join("start.lock").exists(),
        "no start was attempted"
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn remove_refuses_a_store_whose_machine_is_unknown() {
    let home = home_with_stopped(SdkValue::Null);
    let (code, message) = call(&home, "store.remove", &[("name", t("shop"))]).unwrap_err();
    assert_eq!(code, "Unavailable", "{message}");
    assert!(message.contains("import-env"), "{message}");
    assert!(paths::store_record(&home, "shop").is_file(), "records kept");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn remove_reaches_a_stopped_machine_before_it_takes_the_home_lock() {
    let home = home_with_stopped(t("vm"));
    // this thread holds the home lock: a remove that took it before
    // starting the machine would wait 120 s for it
    let lock = home::lock::take(&home).unwrap();
    let started = Instant::now();
    let (code, message) = call(&home, "store.remove", &[("name", t("shop"))]).unwrap_err();
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "{code}: {message}"
    );
    assert!(!message.contains("layerfs.lock"), "{code}: {message}");
    drop(lock);
    assert_eq!(record(&home).get("op"), None, "nothing was marked");
    assert!(paths::store_record(&home, "shop").is_file());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn push_and_pull_refuse_the_wrong_side_and_a_stopped_machine_naming_machine_start() {
    // on native: a pull is refused, a push to the stopped `vm` names its start
    let home = home_with_stopped(t("native"));
    let (code, message) = call(&home, "store.pull", &[("name", t("shop"))]).unwrap_err();
    assert_eq!(code, "InvalidInput", "{message}");
    assert!(message.contains("already on native"), "{message}");
    for (to, want) in [("native", "InvalidInput"), ("nosuch", "NotFound")] {
        let args = [("name", t("shop")), ("machine", t(to))];
        assert_eq!(
            call(&home, "store.push", &args).unwrap_err().0,
            want,
            "{to}"
        );
    }
    let args = [("name", t("shop")), ("machine", t("vm"))];
    let (code, message) = call(&home, "store.push", &args).unwrap_err();
    assert_eq!(code, "Unavailable", "{message}");
    assert!(message.contains("`layerfs machine start vm`"), "{message}");
    assert_eq!(
        record(&home).get("op"),
        None,
        "a certain refusal keeps no op"
    );
    assert_eq!(record(&home).get_text("machine"), Some("native"));
    std::fs::remove_dir_all(home).unwrap();

    // on the stopped `vm`: a push is refused, a pull names its start
    let home = home_with_stopped(t("vm"));
    let args = [("name", t("shop")), ("machine", t("vm"))];
    let (code, message) = call(&home, "store.push", &args).unwrap_err();
    assert_eq!(code, "InvalidInput", "{message}");
    assert!(message.contains("store pull shop"), "{message}");
    let (code, message) = call(&home, "store.pull", &[("name", t("shop"))]).unwrap_err();
    assert_eq!(code, "Unavailable", "{message}");
    assert!(message.contains("`layerfs machine start vm`"), "{message}");
    assert_eq!(record(&home).get("op"), None);
    assert_eq!(record(&home).get_text("machine"), Some("vm"));
    std::fs::remove_dir_all(home).unwrap();
}

/// The whole refusal of `name` with `args`.
fn refusal(
    home: &Path,
    name: &str,
    args: &[(&str, SdkValue)],
) -> layerfs_bridge::control::SdkRefusal {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {}).unwrap_err()
}

fn related(kind: &str, name: &str) -> SdkValue {
    SdkValue::map([("kind", t(kind)), ("name", t(name))])
}

/// A write-ahead log beside `store` that says it holds 15 TiB (a sparse
/// file: no bytes are written), so no target has room for the Store.
fn huge_wal(store: &Path) -> PathBuf {
    let wal = PathBuf::from(format!("{}-wal", store.display()));
    assert!(!wal.exists(), "the Store is closed and checkpointed");
    let file = std::fs::File::create(&wal).unwrap();
    file.set_len(15 << 40).unwrap();
    wal
}

#[test]
fn push_and_pull_name_what_holds_the_store_and_need_room_on_the_target() {
    let (home, root) = home_with_shop();
    let push = [("name", t("shop")), ("machine", t("box"))];
    // a live lease (process 1 is always alive) and a Sandbox record, even
    // one that is not running: `InUse`, `detail.related` names both
    let lease = paths::lease_dir(&home, "shop").join("1");
    std::fs::create_dir_all(lease.parent().unwrap()).unwrap();
    std::fs::write(&lease, "1\n").unwrap();
    write_record(
        &paths::sandbox_record(&home, "t1"),
        &SdkValue::map([
            ("v", SdkValue::Int(1)),
            ("name", t("t1")),
            ("project", t("shop/api")),
            ("machine", t("native")),
        ]),
    )
    .unwrap();
    let held = refusal(&home, "store.push", &push);
    assert_eq!(held.code, "InUse", "{}", held.message);
    assert_eq!(
        held.detail.get("related"),
        Some(&SdkValue::List(vec![
            related("lease", "1"),
            related("sandbox", "t1")
        ]))
    );
    assert_eq!(held.detail.get_text("phase"), Some("check"));
    let removed = refusal(&home, "store.remove", &[("name", t("shop"))]);
    assert_eq!(removed.code, "InUse");
    assert_eq!(held.detail.get("related"), removed.detail.get("related"));
    std::fs::remove_file(&lease).unwrap();
    std::fs::remove_dir_all(paths::sandbox_dir(&home, "t1")).unwrap();

    // no room on the target: `Capacity` before `op` is written
    for (side, call, args) in [
        (&home, "store.push", push.to_vec()),
        (&root, "store.pull", vec![("name", t("shop"))]),
    ] {
        let wal = huge_wal(&paths::store_file(side, "shop"));
        let full = refusal(&home, call, &args);
        assert_eq!(full.code, "Capacity", "{call}: {}", full.message);
        assert!(full.message.contains("bytes free"), "{}", full.message);
        let needed = full.detail.get_int("needed").unwrap();
        assert!(needed > 15 << 40, "{needed}");
        assert!(full.detail.get_int("free").unwrap() < needed);
        assert_eq!(record(&home).get("op"), None, "nothing was marked");
        std::fs::remove_file(wal).unwrap();
        // with room, it goes
        let moved = call_ok(&home, call, &args);
        assert_eq!(moved.get_text("store"), Some("shop"));
    }
    assert_eq!(record(&home).get_text("machine"), Some("native"));
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

fn call_ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, args).unwrap_or_else(|(code, message)| panic!("{name}: {code}: {message}"))
}
