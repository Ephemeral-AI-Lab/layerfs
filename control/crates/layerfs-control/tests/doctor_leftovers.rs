//! What interrupted operations leave, in `home leftovers` and under `home
//! prune` (layerfs-project#180, call surface 2.0): each kind is listed with
//! its path, size and modification time on `native` and through a
//! machine's worker; each is removed only by `home prune`, under its guard;
//! lock files and unmount markers are never listed and are kept; a machine
//! that was not walked, or not to the end, is named in `not_listed`; and no
//! id, however it is written, removes anything outside the entry it names.
//!
//! Nothing runs a daemon or a machine: the leftovers are made by hand in a
//! temporary home, the machine `box` is this build's worker server on a
//! thread ([`worker_store`]), and a `sleep` this test starts stands for a
//! live daemon. A mount point is not made (that needs root): the device
//! rule a mount is told by is checked on its own function
//! (`is_mount_is_decided_by_devices_and_the_table`), which is a simulation.
#![cfg(any(target_os = "linux", target_os = "macos"))]
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::process::LocalProcess;
use layerfs_control::doctor::leftover_list::list;
use layerfs_control::doctor::leftover_prune::{prune, prune_on};
use layerfs_control::doctor::leftovers::{self, Bounds, Kind, BOUNDS};
use layerfs_home::home::{self, paths, write_record};
use layerfs_home::time::iso;
use layerfs_machine::worker::{self, Hello, MAX_HELLO};
use layerfs_sandbox::record::{DaemonRecord, Local};
use layerfs_store::backup::create::Creating;
use layerfs_store::project::send::{log_left, Left};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[path = "support/worker_store.rs"]
mod worker_store;

const HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const COPY: &str = "00112233aabbccdd";

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

/// A new layout 3 home and the first 8 digits of its id.
fn new_home(tag: &str) -> (PathBuf, String) {
    let home = worker_store::scratch(tag).join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    layerfs_control::migrate::open(&home).unwrap();
    let id = home::opened(&home).unwrap().id;
    (home, id[..8].to_owned())
}

fn dir(path: &Path) -> PathBuf {
    std::fs::create_dir_all(path).unwrap();
    path.to_owned()
}

fn file(path: &Path, bytes: usize) {
    dir(path.parent().unwrap());
    std::fs::write(path, vec![b'x'; bytes]).unwrap();
}

/// Sets `path`'s modification time `ago` before now.
fn aged(path: &Path, ago: Duration) {
    let at = SystemTime::now() - ago;
    (std::fs::File::open(path).unwrap())
        .set_modified(at)
        .unwrap();
}

/// A Sandbox run folder under side root `root` whose `owner` marker (older
/// than a start's window) names Sandbox `sandbox`'s record folder there:
/// 30 bytes of files, a link out of it, and a Workspace mount folder the
/// walk must not enter.
fn run_folder(root: &Path, name: &str, sandbox: &str, outside: &Path) -> PathBuf {
    let folder = dir(&paths::run_root(root).join(name));
    file(&folder.join("daemon.log"), 10);
    file(&folder.join("overlay/overlay.sqlite"), 20);
    file(&folder.join("workspaces/w1/not-counted"), 1000);
    std::os::unix::fs::symlink(outside, folder.join("link-out")).unwrap();
    let records = std::fs::canonicalize(dir(&root.join("sandbox"))).unwrap();
    std::fs::write(
        folder.join("owner"),
        records.join(sandbox).as_os_str().as_encoded_bytes(),
    )
    .unwrap();
    aged(&folder.join("owner"), Duration::from_secs(600));
    folder
}

/// Every kind of leftover under side root `root`, and what must never be
/// touched beside them. Answers the folder outside the root a link names.
fn leftovers_under(root: &Path, id8: &str) -> PathBuf {
    let outside = dir(&root.parent().unwrap().join("outside"));
    file(&outside.join("precious"), 9);
    let run = paths::run_root(root);
    run_folder(root, &format!("layerfs-{id8}-gone"), "gone", &outside);
    dir(&run.join(format!("layerfs-{id8}-dead.root")));
    // a private root that still holds something is not one
    file(&run.join(format!("layerfs-{id8}-full.root/etc/x")), 1);
    let cache = dir(&run.join("images"));
    file(&cache.join(format!(".tmp-{HEX}.2-ab12/usr/bin/tool")), 40);
    file(&cache.join(format!(".recv-{HEX}.part")), 50);
    aged(
        &cache.join(format!(".recv-{HEX}.part")),
        Duration::from_secs(7200),
    );
    file(&cache.join(format!("{HEX}.2.lock")), 0);
    file(&cache.join(format!("{}.2.lock", HEX.replace('0', "f"))), 0);
    file(&cache.join(format!("{HEX}.2/bin/sh")), 5);
    file(
        &paths::store_dir(root, "shop").join("store.sqlite.backup"),
        7,
    );
    file(&root.join(format!("incoming/init-{COPY}/src/main.rs")), 60);
    file(&root.join("sandbox/s1/workspace/7.unmount"), 2);
    // what an integration keeps in a home is never listed or read
    for reserved in ["secret", "runtime", "job", "stat", "preset"] {
        file(&root.join(reserved).join("init-0000000000000000/x"), 3);
        file(&root.join(reserved).join("backup.staging"), 3);
    }
    outside
}

/// The list's items, each checked against its own fields.
fn found(listed: &SdkValue) -> Vec<SdkValue> {
    let Some(SdkValue::List(items)) = listed.get("items") else {
        panic!("no items: {listed:?}");
    };
    for item in items {
        let id = item.get_text("leftover").unwrap();
        let (machine, kind) = (
            item.get_text("machine").unwrap(),
            item.get_text("kind").unwrap(),
        );
        assert!(id.starts_with(&format!("{machine}:{kind}:")), "{item:?}");
        assert!(Kind::named(kind).is_some(), "{item:?}");
        assert!(item
            .get_text("modified_at")
            .is_some_and(|at| at.ends_with('Z')));
        assert!(item.get_text("path").is_some(), "{item:?}");
    }
    items.clone()
}

/// The leftover of `kind` on `machine` in `listed`.
fn one(listed: &SdkValue, machine: &str, kind: Kind) -> SdkValue {
    let all = found(listed);
    let mut of_kind = (all.iter()).filter(|left| {
        left.get_text("machine") == Some(machine) && left.get_text("kind") == Some(kind.as_str())
    });
    let left = of_kind
        .next()
        .unwrap_or_else(|| panic!("no {} on {machine}: {all:?}", kind.as_str()));
    assert!(of_kind.next().is_none(), "two {}: {all:?}", kind.as_str());
    left.clone()
}

fn ids(listed: &SdkValue) -> Vec<String> {
    (found(listed).iter())
        .map(|left| left.get_text("leftover").unwrap().to_owned())
        .collect()
}

/// The machines `listed` names in `not_listed`, each with its entry.
fn not_listed(listed: &SdkValue) -> Vec<(String, SdkValue)> {
    let Some(SdkValue::List(rows)) = listed.get("not_listed") else {
        panic!("no not_listed: {listed:?}");
    };
    (rows.iter())
        .map(|row| (row.get_text("machine").unwrap().to_owned(), row.clone()))
        .collect()
}

/// What interrupted operations left in `home`, the list `home prune` takes
/// its ids from.
fn doctor(home: &Path) -> SdkValue {
    list(home, None).unwrap()
}

fn refusal(outcome: Result<SdkValue, SdkRefusal>, code: &str) -> String {
    let refused = outcome.expect_err("refused");
    assert_eq!(refused.code, code, "{}", refused.message);
    refused.message
}

/// Checks what every side reports of [`leftovers_under`], on `machine`.
fn reported(report: &SdkValue, machine: &str, root: &Path, id8: &str) {
    let run = paths::run_root(root);
    let expect = |kind: Kind, path: PathBuf, bytes: i64, name: &str| {
        let left = one(report, machine, kind);
        assert_eq!(left.get_text("path"), Some(path.to_str().unwrap()));
        assert_eq!(left.get_int("size"), Some(bytes), "{left:?}");
        let id = format!("{machine}:{}:{name}", kind.as_str());
        assert_eq!(left.get_text("leftover"), Some(id.as_str()));
        left
    };
    // the files of the run folder but not its Workspace mount folder: the
    // two files, the owner marker and the link itself
    let gone = format!("layerfs-{id8}-gone");
    let marker = std::fs::metadata(run.join(&gone).join("owner"))
        .unwrap()
        .len() as i64;
    let link = std::fs::symlink_metadata(run.join(&gone).join("link-out"))
        .unwrap()
        .len() as i64;
    expect(Kind::RunFolder, run.join(&gone), 30 + marker + link, &gone);
    let dead = format!("layerfs-{id8}-dead");
    expect(Kind::EmptyRoot, run.join(format!("{dead}.root")), 0, &dead);
    let tmp = format!("{HEX}.2-ab12");
    expect(
        Kind::ImageTmp,
        run.join("images").join(format!(".tmp-{tmp}")),
        40,
        &tmp,
    );
    // a received image part is not a leftover (spec v19): `machine images`
    // shows it and `machine prune` removes it, under the copy's lock
    assert!(run
        .join("images")
        .join(format!(".recv-{HEX}.part"))
        .is_file());
    assert!(
        !found(report)
            .iter()
            .any(|entry| entry.get_text("kind") == Some("image_part")),
        "{:?}",
        found(report)
    );
    expect(
        Kind::BackupStaging,
        paths::store_dir(root, "shop").join("store.sqlite.backup"),
        7,
        "shop/store.sqlite.backup",
    );
    let copy = expect(
        Kind::InitCopy,
        root.join(format!("incoming/init-{COPY}")),
        60,
        COPY,
    );
    // no uncertain line names it: reported only
    assert!(copy.get_bool("report_only"), "{copy:?}");
    assert_eq!(copy.get("removable_at"), Some(&SdkValue::Null), "{copy:?}");
    // the lock files and the unmount marker are never listed
    assert_eq!(found(report).len(), 5, "{:?}", found(report));
    // the walk ended: the machine is not named as passed over
    assert!(!not_listed(report).iter().any(|(name, _)| name == machine));
}

/// Removes every removable leftover of [`leftovers_under`] on `machine` and
/// checks each is gone, from the disk and from the next report.
fn fixed(home: &Path, machine: &str, root: &Path, outside: &Path) {
    let cache = paths::run_root(root).join("images");
    for kind in [
        Kind::RunFolder,
        Kind::EmptyRoot,
        Kind::ImageTmp,
        Kind::BackupStaging,
    ] {
        let left = one(&doctor(home), machine, kind);
        let (id, path) = (
            left.get_text("leftover").unwrap(),
            left.get_text("path").unwrap(),
        );
        assert!(std::fs::symlink_metadata(path).is_ok(), "{path}");
        let removed = prune(home, id).unwrap_or_else(|r| panic!("{id}: {r:?}"));
        assert_eq!(removed.get_text("outcome"), Some("removed"));
        assert_eq!(removed.get_text("leftover"), Some(id));
        assert_eq!(removed.get_text("machine"), Some(machine));
        assert_eq!(removed.get_text("kind"), Some(kind.as_str()));
        assert_eq!(removed.get_text("path"), Some(path));
        // what it freed is the size it was listed with; a run folder's
        // Workspace mount folder is removed with it but was never measured,
        // and an unpack's temporary is removed without a count
        let freed = removed.get_int("freed_bytes");
        match kind {
            Kind::RunFolder => assert!(freed > left.get_int("size"), "{removed:?}"),
            Kind::ImageTmp => assert_eq!(freed, None, "{removed:?}"),
            _ => assert_eq!(freed, left.get_int("size"), "{removed:?}"),
        }
        assert!(std::fs::symlink_metadata(path).is_err(), "{path} is there");
        assert!(!ids(&doctor(home)).iter().any(|other| other == id), "{id}");
        // gone: the same id is not found again
        refusal(prune(home, id), "NotFound");
        // every removal leaves the image lock files, and what a link named
        assert!(cache.join(format!("{HEX}.2.lock")).is_file());
        assert!(cache
            .join(format!("{}.2.lock", HEX.replace('0', "f")))
            .is_file());
        assert!(cache.join(format!("{HEX}.2/bin/sh")).is_file());
        assert!(outside.join("precious").is_file());
    }
    // the folder copy is still there: nothing names it
    assert_eq!(found(&doctor(home)).len(), 1);
}

#[test]
fn native_lists_each_kind_and_prune_removes_it() {
    let (home, id8) = new_home("native");
    assert!(found(&doctor(&home)).is_empty());
    let outside = leftovers_under(&home, &id8);
    // the home's own copy of a worker's staged snapshot
    file(&paths::store_dir(&home, "web").join("backup.staging"), 11);
    let report = doctor(&home);
    let staged = (found(&report).into_iter())
        .find(|left| left.get_text("leftover") == Some("native:backup_staging:web/backup.staging"))
        .expect("backup.staging");
    assert_eq!(staged.get_int("size"), Some(11));
    prune(&home, "native:backup_staging:web/backup.staging").unwrap();
    assert!(!paths::store_dir(&home, "web")
        .join("backup.staging")
        .exists());
    reported(&doctor(&home), "native", &home, &id8);
    // on native no Init is staged, so no line can name the copy: reported only
    let copy = format!("native:init_copy:{COPY}");
    let why = refusal(prune(&home, &copy), "InvalidInput");
    assert!(why.contains("only reported"), "{why}");
    fixed(&home, "native", &home, &outside);
    assert!(home
        .join(format!("incoming/init-{COPY}/src/main.rs"))
        .is_file());
    // the reserved names were never listed, and are as they were
    for reserved in ["secret", "runtime", "job", "stat", "preset"] {
        assert!(home.join(reserved).join("backup.staging").is_file());
        assert!(home
            .join(reserved)
            .join("init-0000000000000000/x")
            .is_file());
    }
    std::fs::remove_dir_all(home.parent().unwrap()).unwrap();
}

fn serve(listener: std::net::TcpListener, root: PathBuf) {
    std::thread::spawn(move || {
        layerfs_control::worker_server::serve(listener, worker_store::SECRET.to_owned(), root)
    });
}

/// One line of the home's uncertain log naming copy [`COPY`] on `box`,
/// whose Init's deadline is `deadline_in` ms from now.
fn name_copy(home: &Path, deadline_in: i64) {
    let left = Left {
        store: "shop",
        project: "web",
        machine: "box",
        copy: COPY,
        phase: "install",
        sent_ms: home::now_ms() + deadline_in - 1000,
        timeout_ms: 1000,
    };
    log_left(home, &left, "project.init: the answer was lost");
}

#[test]
fn a_worker_lists_each_kind_and_prune_removes_it() {
    let placed = worker_store::placed("worker", serve);
    let (home, root) = (placed.home.clone(), placed.root.clone());
    let id8 = home::opened(&home).unwrap().id[..8].to_owned();
    let outside = leftovers_under(&root, &id8);
    // a run folder of a Sandbox the home records on this machine is not a
    // leftover, whatever the worker's own records say
    let kept = format!("layerfs-{id8}-kept");
    file(&paths::run_root(&root).join(&kept).join("daemon.log"), 4);
    write_record(
        &paths::sandbox_record(&home, "kept"),
        &SdkValue::map([
            ("v", SdkValue::Int(home::RECORD_VERSION)),
            ("name", t("kept")),
            ("project", t("shop/api")),
            ("machine", t("box")),
        ]),
    )
    .unwrap();
    let report = doctor(&home);
    reported(&report, "box", &root, &id8);
    // the home itself holds none of them, and every side was walked
    assert!(not_listed(&report).is_empty(), "{report:?}");
    assert!(found(&report)
        .iter()
        .all(|left| left.get_text("machine") == Some("box")));
    // `--machine` lists one side
    let only = list(&home, Some("box")).unwrap();
    assert_eq!(ids(&only), ids(&report));
    assert!(found(&list(&home, Some("native")).unwrap()).is_empty());
    let why = refusal(prune(&home, &format!("box:run_folder:{kept}")), "InUse");
    assert!(why.contains("a Sandbox record names"), "{why}");
    assert!(paths::run_root(&root).join(&kept).is_dir());

    // a staged snapshot on the machine is removed under the home's lock
    let staged = "box:backup_staging:shop/store.sqlite.backup";
    let held = Creating::take(&home, "shop").unwrap();
    let why = refusal(prune(&home, staged), "InUse");
    assert!(why.contains("backup create"), "{why}");
    drop(held);

    // the folder copy: unnamed it is only reported; named, it is removable
    // an hour after its Init's deadline, by the worker's own discard
    let copy = format!("box:init_copy:{COPY}");
    refusal(prune(&home, &copy), "InvalidInput");
    name_copy(&home, 60_000);
    let left = one(&doctor(&home), "box", Kind::InitCopy);
    assert!(!left.get_bool("report_only"), "{left:?}");
    let removable_at = left.get_text("removable_at").unwrap().to_owned();
    assert!(removable_at > iso(home::now_ms() + 3_500_000), "{left:?}");
    let why = refusal(prune(&home, &copy), "InUse");
    assert!(why.contains("deadline"), "{why}");
    // a deadline that passed, but less than an hour ago
    name_copy(&home, -600_000);
    refusal(prune(&home, &copy), "InUse");
    assert!(root
        .join(format!("incoming/init-{COPY}/src/main.rs"))
        .is_file());

    fixed(&home, "box", &root, &outside);

    name_copy(&home, -2 * 3_600_000);
    let removed = prune(&home, &copy).unwrap();
    assert_eq!(removed.get_text("kind"), Some("init_copy"));
    assert_eq!(removed.get("freed_bytes"), Some(&SdkValue::Null));
    assert!(!root.join(format!("incoming/init-{COPY}")).exists());
    assert!(found(&doctor(&home)).is_empty());
    refusal(prune(&home, &copy), "NotFound");
    placed.remove();
}

#[test]
fn a_stopped_or_silent_machine_is_not_listed_and_a_lost_answer_unknown() {
    let (home, _) = new_home("routes");
    let machine = |name: &str| {
        write_record(
            &paths::machine_record(&home, name),
            &SdkValue::map([("name", t(name)), ("platform", t("lima"))]),
        )
        .unwrap();
    };
    // recorded, with no worker: stopped
    machine("off");
    let id = format!("off:image_tmp:{HEX}.2-ab12");
    let why = refusal(prune(&home, &id), "Unavailable");
    assert!(why.contains("machine start off"), "{why}");
    refusal(
        prune(&home, "off:run_folder:layerfs-00000000-x"),
        "Unavailable",
    );
    // nothing is started to list its leftovers: it is named instead, by
    // the state of the thing at the root
    let passed = not_listed(&doctor(&home));
    assert_eq!(passed.len(), 1, "{passed:?}");
    let (name, entry) = &passed[0];
    assert_eq!(name, "off");
    let by = entry.get("blocked_by").unwrap();
    assert_eq!(by.get_text("machine"), Some("off"));
    assert_eq!(by.get_text("reason"), Some("stopped"));
    assert_eq!(entry.get_text("message"), Some("machine off is stopped"));
    assert!(entry.get("reason").is_none(), "{entry:?}");
    // `--machine` names one machine, which must be the home's
    assert_eq!(not_listed(&list(&home, Some("off")).unwrap()).len(), 1);
    assert!(not_listed(&list(&home, Some("native")).unwrap()).is_empty());
    let why = refusal(list(&home, Some("nowhere")), "NotFound");
    assert!(why.contains("no machine named nowhere"), "{why}");

    // a machine whose recorded worker does not answer is named with why
    machine("silent");
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = closed.local_addr().unwrap().to_string();
    drop(closed);
    write_record(
        &paths::worker_record(&home, "silent"),
        &SdkValue::map([("address", t(&address)), ("secret", t("k"))]),
    )
    .unwrap();
    let passed = not_listed(&list(&home, Some("silent")).unwrap());
    assert_eq!(passed.len(), 1, "{passed:?}");
    assert_eq!(
        passed[0].1.get_text("reason"),
        Some("timeout"),
        "{passed:?}"
    );
    assert!(passed[0].1.get("blocked_by").is_none(), "{passed:?}");

    // a worker of this build that takes the request and then goes away
    machine("lost");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    write_record(
        &paths::worker_record(&home, "lost"),
        &SdkValue::map([("address", t(&address)), ("secret", t("k"))]),
    )
    .unwrap();
    let served = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        (stream.set_read_timeout(Some(Duration::from_secs(10)))).unwrap();
        let _ = worker::read_frame_within(&mut stream, MAX_HELLO);
        worker::write_frame(&mut stream, &Hello::own().to_frame()).unwrap();
        worker::read_frame(&mut stream).ok().flatten()
    });
    let why = refusal(
        prune(&home, &format!("lost:image_tmp:{HEX}.2-ab12")),
        "Unknown",
    );
    assert!(
        why.contains("not resent") || why.contains("may have run"),
        "{why}"
    );
    let request = served.join().unwrap().expect("the request was sent once");
    assert_eq!(request.get_text("op"), Some("home.prune"));
    let args = request.get("args").unwrap();
    // a kind and a name travel, never a path
    assert_eq!(args.get_text("kind"), Some("image_tmp"));
    assert_eq!(
        args.get_text("name"),
        Some(format!("{HEX}.2-ab12").as_str())
    );
    assert!(args.get("path").is_none(), "{args:?}");
    std::fs::remove_dir_all(home.parent().unwrap()).unwrap();
}

/// Sandbox `id`'s record in `home`: started, its daemon the process `pid`,
/// in run folder `folder`.
fn started(home: &Path, id: &str, folder: &Path, pid: u32) {
    let process = LocalProcess::observe(pid).unwrap();
    let daemon = DaemonRecord::local(Local {
        platform: "mac".to_owned(),
        name: folder.file_name().unwrap().to_str().unwrap().to_owned(),
        container: "ab".repeat(32),
        runtime: "native",
        uid: home::flock::euid(),
        gid: home::flock::egid(),
        directory: folder.display().to_string(),
        process: (
            process.pid,
            process.start,
            format!("{:032x}", process.boot.unwrap()),
        ),
        port: 9,
        image: "local".to_owned(),
        labels: Vec::new(),
    });
    write_record(
        &paths::sandbox_record(home, id),
        &SdkValue::map([
            ("v", SdkValue::Int(home::RECORD_VERSION)),
            ("name", t(id)),
            ("project", t("shop/api")),
            ("machine", t("native")),
            ("id", t(id)),
            ("runtime", t("native")),
            ("daemon", daemon.to_record()),
        ]),
    )
    .unwrap();
}

#[test]
fn a_run_folder_in_use_is_refused() {
    let (home, id8) = new_home("inuse");
    let outside = dir(&home.parent().unwrap().join("outside"));
    let run = paths::run_root(&home);
    let name = |sandbox: &str| format!("layerfs-{id8}-{sandbox}");
    let id = |sandbox: &str| format!("native:run_folder:{}", name(sandbox));

    // a Sandbox whose record names its run folder is not a leftover
    let mut daemon = std::process::Command::new("sleep")
        .arg("90")
        .spawn()
        .unwrap();
    let recorded = run_folder(&home, &name("live"), "live", &outside);
    started(&home, "live", &recorded, daemon.id());
    // its earlier run folder, under the earlier prefix, is one: but its
    // `owner` marker names a record whose daemon runs
    let earlier = run_folder(&home, &format!("lfs-{id8}-live"), "live", &outside);
    let report = doctor(&home);
    let listed = ids(&report);
    let earlier_id = format!("native:run_folder:lfs-{id8}-live");
    assert_eq!(listed, [earlier_id.clone()], "{report:?}");
    let why = refusal(prune(&home, &id("live")), "InUse");
    assert!(why.contains("a Sandbox record names"), "{why}");
    let why = refusal(prune(&home, &earlier_id), "InUse");
    assert!(why.contains("live daemon"), "{why}");
    assert!(earlier.join("daemon.log").is_file() && recorded.join("daemon.log").is_file());
    // once that daemon ended, the earlier folder is removable; the recorded
    // one never is
    daemon.kill().unwrap();
    daemon.wait().unwrap();
    prune(&home, &earlier_id).unwrap();
    assert!(!earlier.exists());
    refusal(prune(&home, &id("live")), "InUse");

    // no `owner` marker: not known to be a finished start's folder
    let bare = dir(&run.join(name("bare")));
    file(&bare.join("daemon.log"), 3);
    assert!(ids(&doctor(&home)).contains(&id("bare")));
    let why = refusal(prune(&home, &id("bare")), "InUse");
    assert!(why.contains("no `owner` marker"), "{why}");
    // a marker just written: its Sandbox may be starting
    let fresh = run_folder(&home, &name("fresh"), "fresh", &outside);
    aged(&fresh.join("owner"), Duration::ZERO);
    let why = refusal(prune(&home, &id("fresh")), "InUse");
    assert!(why.contains("may be starting"), "{why}");
    // a Sandbox being created: its record is there, with no daemon yet
    let creating = run_folder(&home, &name("soon"), "soon", &outside);
    write_record(
        &paths::sandbox_record(&home, "soon"),
        &SdkValue::map([
            ("v", SdkValue::Int(home::RECORD_VERSION)),
            ("name", t("soon")),
            ("project", t("shop/api")),
            ("machine", t("native")),
        ]),
    )
    .unwrap();
    assert!(!ids(&doctor(&home)).contains(&id("soon")));
    refusal(prune(&home, &id("soon")), "InUse");
    for kept in [&bare, &fresh, &creating] {
        assert!(kept.join("daemon.log").is_file(), "{}", kept.display());
    }

    // an empty private root beside its run folder belongs to that Sandbox
    dir(&run.join(format!("{}.root", name("bare"))));
    assert!(!ids(&doctor(&home))
        .iter()
        .any(|id| id.contains("empty_root")));
    let why = refusal(
        prune(&home, &format!("native:empty_root:{}", name("bare"))),
        "InUse",
    );
    assert!(why.contains("run folder is there"), "{why}");
    assert!(run.join(format!("{}.root", name("bare"))).is_dir());
    std::fs::remove_dir_all(home.parent().unwrap()).unwrap();
}

#[test]
fn is_mount_is_decided_by_devices_and_the_table() {
    // simulated: a directory on another device than its parent is a mount
    // point, and so is one the mount table lists on the same device (a
    // bind mount); only one that is neither is removed
    assert!(leftovers::is_mount(7, 3, false));
    assert!(leftovers::is_mount(3, 3, true));
    assert!(!leftovers::is_mount(3, 3, false));
}

#[test]
fn guards_held_by_another_are_in_use() {
    let (home, id8) = new_home("guards");
    let outside = leftovers_under(&home, &id8);
    let cache = paths::run_root(&home).join("images");

    // a `backup.create` of the Store holds `backup.lock`
    let staged = "native:backup_staging:shop/store.sqlite.backup";
    let held = Creating::take(&home, "shop").unwrap();
    let why = refusal(prune(&home, staged), "InUse");
    assert!(why.contains("backup create"), "{why}");
    drop(held);

    // an unpack of the image holds its lock
    let lock = (std::fs::OpenOptions::new().read(true).write(true))
        .open(cache.join(format!("{HEX}.2.lock")))
        .unwrap();
    let lock = nix::fcntl::Flock::lock(lock, nix::fcntl::FlockArg::LockExclusiveNonblock)
        .map_err(|(_, error)| error)
        .unwrap();
    let tmp = format!("native:image_tmp:{HEX}.2-ab12");
    let why = refusal(prune(&home, &tmp), "InUse");
    assert!(why.contains("lock"), "{why}");
    drop(lock);

    // an image part is no leftover (spec v19): no id removes it here
    let part = cache.join(format!(".recv-{HEX}.part"));
    refusal(
        prune(&home, &format!("native:image_part:{HEX}")),
        "NotFound",
    );

    // nothing was removed, and with each guard free again all of it goes
    for path in [
        paths::store_dir(&home, "shop").join("store.sqlite.backup"),
        cache.join(format!(".tmp-{HEX}.2-ab12/usr/bin/tool")),
        part.clone(),
    ] {
        assert!(path.is_file(), "{}", path.display());
    }
    fixed(&home, "native", &home, &outside);
    assert!(part.is_file(), "a part stays for `machine prune`");

    // an image's lock file and a Workspace's unmount marker are no
    // leftover: no id names them, and nothing removes them
    refusal(
        prune(&home, &format!("native:image_lock:{HEX}.2")),
        "NotFound",
    );
    refusal(prune(&home, "native:unmount_marker:s1/7"), "NotFound");
    assert!(cache.join(format!("{HEX}.2.lock")).is_file());
    assert!(home.join("sandbox/s1/workspace/7.unmount").is_file());
    std::fs::remove_dir_all(home.parent().unwrap()).unwrap();
}

#[test]
fn a_crafted_id_removes_nothing_outside_its_entry() {
    let (home, id8) = new_home("crafted");
    let outside = leftovers_under(&home, &id8);
    let run = paths::run_root(&home);
    // entries that are links out of the home
    std::os::unix::fs::symlink(&outside, run.join(format!("layerfs-{id8}-link"))).unwrap();
    std::os::unix::fs::symlink(&outside, run.join(format!("layerfs-{id8}-away.root"))).unwrap();
    file(&outside.join("backup.staging"), 8);
    file(&outside.join("store.sqlite.backup"), 8);
    std::os::unix::fs::symlink(&outside, paths::store_dir(&home, "evil")).unwrap();
    std::os::unix::fs::symlink(
        outside.join("precious"),
        paths::store_dir(&home, "shop").join("backup.staging"),
    )
    .unwrap();
    std::os::unix::fs::symlink(&outside, home.join("incoming/init-ffffffffffffffff")).unwrap();
    let before = ids(&doctor(&home));
    assert_eq!(before.len(), 5, "a link is never a leftover: {before:?}");

    let absolute = format!("native:run_folder:{}", outside.display());
    let gone = format!("native:run-folder:layerfs-{id8}-gone");
    let crafted = [
        "",
        "native",
        "native:run_folder",
        "native:run_folder:",
        "native:run_folder:..",
        "native:run_folder:.",
        "native:run_folder:../../../outside",
        "native:run_folder:a/b",
        "native:run_folder:images",
        "native:run_folder:.hidden",
        absolute.as_str(),
        // today's hyphenated kind is no kind
        gone.as_str(),
        "native:empty_root:..",
        "native:empty_root:../x",
        "native:backup_staging:../outside/backup.staging",
        "native:backup_staging:shop/../../layerfs.json",
        "native:backup_staging:shop/store.sqlite",
        "native:backup_staging:shop/record.json",
        "native:image_tmp:../../x",
        "native:image_part:../x",
        "native:init_copy:../../outside",
        "native:nothing:x",
        "../x:run_folder:y",
        "NATIVE:run_folder:y",
    ];
    for id in crafted {
        assert!(leftovers::parse(id).is_none(), "{id}");
        refusal(prune(&home, id), "NotFound");
    }
    // well-formed ids that name a link, or an entry behind one
    for id in [
        format!("native:run_folder:layerfs-{id8}-link"),
        format!("native:empty_root:layerfs-{id8}-away"),
        "native:backup_staging:evil/backup.staging".to_owned(),
        "native:backup_staging:evil/store.sqlite.backup".to_owned(),
        "native:backup_staging:shop/backup.staging".to_owned(),
    ] {
        assert!(leftovers::parse(&id).is_some(), "{id}");
        refusal(prune(&home, &id), "NotFound");
    }
    // the removing side refuses the same names when asked directly, as a
    // worker is
    for (kind, name) in [
        (Kind::RunFolder, ".."),
        (Kind::RunFolder, "../../../outside"),
        (Kind::EmptyRoot, "x/../.."),
        (Kind::BackupStaging, "../outside/backup.staging"),
        (Kind::ImageTmp, "../../x"),
        (Kind::ExportContainer, "../x"),
    ] {
        refusal(prune_on(&home, kind, name, &[]), "NotFound");
    }
    // a run root that is itself a link out of the home is not entered
    let (other, other8) = new_home("crafted-linked");
    let elsewhere = dir(&other.parent().unwrap().join("elsewhere"));
    file(&elsewhere.join(format!("layerfs-{other8}-x/owner")), 1);
    aged(
        &elsewhere.join(format!("layerfs-{other8}-x/owner")),
        Duration::from_secs(600),
    );
    dir(paths::run_root(&other).parent().unwrap());
    std::os::unix::fs::symlink(&elsewhere, paths::run_root(&other)).unwrap();
    assert!(found(&doctor(&other)).is_empty());
    refusal(
        prune(&other, &format!("native:run_folder:layerfs-{other8}-x")),
        "NotFound",
    );
    assert!(elsewhere
        .join(format!("layerfs-{other8}-x/owner"))
        .is_file());

    // everything outside is as it was, and so is every real leftover
    for kept in ["precious", "backup.staging", "store.sqlite.backup"] {
        assert!(outside.join(kept).is_file(), "{kept}");
    }
    assert!(home.join("layerfs.json").is_file());
    assert_eq!(ids(&doctor(&home)), before);
    // the run folder's own link out is unlinked with it, never followed
    prune(&home, &format!("native:run_folder:layerfs-{id8}-gone")).unwrap();
    assert!(outside.join("precious").is_file());
    std::fs::remove_dir_all(other.parent().unwrap()).unwrap();
    std::fs::remove_dir_all(home.parent().unwrap()).unwrap();
}

#[test]
fn a_walk_past_its_bound_says_it_is_not_complete() {
    let (home, id8) = new_home("bound");
    leftovers_under(&home, &id8);
    let whole = leftovers::walk(&home, &[], BOUNDS);
    assert!(whole.get_bool("complete"), "{whole:?}");
    let count = |walked: &SdkValue| match walked.get("leftovers") {
        Some(SdkValue::List(rows)) => rows.len(),
        _ => panic!("{walked:?}"),
    };
    assert_eq!(count(&whole), 5);
    // too few entries: it stops, says so, and measures nothing it did not
    // finish
    let few = Bounds {
        entries: 4,
        ..BOUNDS
    };
    let cut = leftovers::walk(&home, &[], few);
    assert!(!cut.get_bool("complete"), "{cut:?}");
    assert!(count(&cut) < 5, "{cut:?}");
    // no time at all
    let none = Bounds {
        within: Duration::ZERO,
        ..BOUNDS
    };
    let cut = leftovers::walk(&home, &[], none);
    assert!(!cut.get_bool("complete"), "{cut:?}");
    assert_eq!(count(&cut), 0, "{cut:?}");
    // a folder larger than the bound is listed without a size
    let big = paths::run_root(&home).join(format!("layerfs-{id8}-big"));
    for at in 0..40 {
        file(&big.join(format!("f{at}")), 1);
    }
    let bounded = Bounds {
        entries: 30,
        ..BOUNDS
    };
    let cut = leftovers::walk(&home, &[], bounded);
    assert!(!cut.get_bool("complete"), "{cut:?}");
    let Some(SdkValue::List(rows)) = cut.get("leftovers") else {
        panic!("{cut:?}");
    };
    assert!(
        rows.iter()
            .any(|row| row.get("bytes") == Some(&SdkValue::Null)),
        "{rows:?}"
    );
    std::fs::remove_dir_all(home.parent().unwrap()).unwrap();
}
