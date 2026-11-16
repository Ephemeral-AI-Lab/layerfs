//! What interrupted operations leave, in `layerfs doctor`'s report and
//! under `doctor.fix` (layerfs-project#180): each kind is reported with its
//! path, bytes and age on `native` and through a machine's worker; each is
//! removed only by `doctor.fix`, under its guard; lock files and unmount
//! markers are counted and kept; a walk past its bound says so; and no id,
//! however it is written, removes anything outside the entry it names.
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
use layerfs_control::doctor::leftover_fix::{fix, fix_on};
use layerfs_control::doctor::leftovers::{self, Bounds, Kind, BOUNDS};
use layerfs_home::home::{self, paths, write_record};
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

/// The report's leftover findings: each `leftover` map.
fn found(report: &SdkValue) -> Vec<SdkValue> {
    let Some(SdkValue::List(findings)) = report.get("findings") else {
        panic!("no findings: {report:?}");
    };
    (findings.iter())
        .filter_map(|finding| {
            let left = finding.get("leftover")?.clone();
            let fix = finding.get_text("fix").unwrap();
            let id = left.get_text("id").unwrap();
            let fixable = left.get_bool("fixable");
            assert_eq!(
                fix.contains(&format!("`layerfs doctor fix {id}`")),
                fixable,
                "{finding:?}"
            );
            assert_eq!(finding.get_text("severity"), Some("warning"));
            let message = finding.get_text("message").unwrap();
            assert!(
                message.contains(left.get_text("path").unwrap()),
                "{message}"
            );
            Some(left)
        })
        .collect()
}

/// The leftover of `kind` on `machine` in `report`.
fn one(report: &SdkValue, machine: &str, kind: Kind) -> SdkValue {
    let all = found(report);
    let mut of_kind = (all.iter()).filter(|left| {
        left.get_text("machine") == Some(machine) && left.get_text("kind") == Some(kind.as_str())
    });
    let left = of_kind
        .next()
        .unwrap_or_else(|| panic!("no {} on {machine}: {all:?}", kind.as_str()));
    assert!(of_kind.next().is_none(), "two {}: {all:?}", kind.as_str());
    left.clone()
}

fn ids(report: &SdkValue) -> Vec<String> {
    (found(report).iter())
        .map(|left| left.get_text("id").unwrap().to_owned())
        .collect()
}

fn row(report: &SdkValue, machine: &str) -> SdkValue {
    let Some(SdkValue::List(rows)) = report.get("leftovers") else {
        panic!("no leftovers rows: {report:?}");
    };
    (rows.iter())
        .find(|row| row.get_text("machine") == Some(machine))
        .unwrap_or_else(|| panic!("machine {machine} was not walked: {rows:?}"))
        .clone()
}

fn doctor(home: &Path) -> SdkValue {
    layerfs_control::doctor::doctor(home).unwrap()
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
        assert_eq!(left.get_int("bytes"), Some(bytes), "{left:?}");
        assert!(left.get_int("age").is_some_and(|age| age >= 0), "{left:?}");
        let id = format!("{machine}:{}:{name}", kind.as_str());
        assert_eq!(left.get_text("id"), Some(id.as_str()));
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
    let part = expect(
        Kind::ImagePart,
        run.join("images").join(format!(".recv-{HEX}.part")),
        50,
        HEX,
    );
    // the modification time of the part itself, set two hours back
    let age = part.get_int("age").unwrap();
    assert!((7195..7400).contains(&age), "{age}");
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
    assert!(!copy.get_bool("fixable"), "{copy:?}");
    assert_eq!(found(report).len(), 6, "{:?}", found(report));
    let row = row(report, machine);
    assert!(row.get_bool("complete"), "{row:?}");
    assert_eq!(row.get_int("image_locks"), Some(2), "{row:?}");
    assert_eq!(row.get_int("unmount_markers"), Some(1), "{row:?}");
}

/// Removes every fixable leftover of [`leftovers_under`] on `machine` and
/// checks each is gone, from the disk and from the next report.
fn fixed(home: &Path, machine: &str, root: &Path, outside: &Path) {
    let cache = paths::run_root(root).join("images");
    for kind in [
        Kind::RunFolder,
        Kind::EmptyRoot,
        Kind::ImageTmp,
        Kind::ImagePart,
        Kind::BackupStaging,
    ] {
        let left = one(&doctor(home), machine, kind);
        let (id, path) = (left.get_text("id").unwrap(), left.get_text("path").unwrap());
        assert!(std::fs::symlink_metadata(path).is_ok(), "{path}");
        let removed = fix(home, id).unwrap_or_else(|r| panic!("{id}: {r:?}"));
        assert_eq!(removed.get("removed"), Some(&SdkValue::Bool(true)));
        assert_eq!(removed.get_text("id"), Some(id));
        assert_eq!(removed.get_text("machine"), Some(machine));
        assert_eq!(removed.get_text("kind"), Some(kind.as_str()));
        assert_eq!(removed.get_text("path"), Some(path));
        assert!(std::fs::symlink_metadata(path).is_err(), "{path} is there");
        assert!(!ids(&doctor(home)).iter().any(|other| other == id), "{id}");
        // gone: the same id is not found again
        refusal(fix(home, id), "NotFound");
        // every fix leaves the image lock files, and what a link named
        assert!(cache.join(format!("{HEX}.2.lock")).is_file());
        assert!(cache
            .join(format!("{}.2.lock", HEX.replace('0', "f")))
            .is_file());
        assert!(cache.join(format!("{HEX}.2/bin/sh")).is_file());
        assert!(outside.join("precious").is_file());
    }
    // the folder copy is still there: nothing names it
    assert_eq!(found(&doctor(home)).len(), 1);
    let row = row(&doctor(home), machine);
    assert_eq!(row.get_int("image_locks"), Some(2), "{row:?}");
    assert_eq!(row.get_int("unmount_markers"), Some(1), "{row:?}");
}

#[test]
fn native_reports_each_kind_and_fix_removes_it() {
    let (home, id8) = new_home("native");
    assert!(found(&doctor(&home)).is_empty());
    let outside = leftovers_under(&home, &id8);
    // the home's own copy of a worker's staged snapshot
    file(&paths::store_dir(&home, "web").join("backup.staging"), 11);
    let report = doctor(&home);
    let staged = (found(&report).into_iter())
        .find(|left| left.get_text("id") == Some("native:backup-staging:web/backup.staging"))
        .expect("backup.staging");
    assert_eq!(staged.get_int("bytes"), Some(11));
    fix(&home, "native:backup-staging:web/backup.staging").unwrap();
    assert!(!paths::store_dir(&home, "web")
        .join("backup.staging")
        .exists());
    reported(&doctor(&home), "native", &home, &id8);
    // on native no Init is staged, so no line can name the copy: reported only
    let copy = format!("native:init-copy:{COPY}");
    let why = refusal(fix(&home, &copy), "InvalidInput");
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
fn a_worker_reports_each_kind_and_fix_removes_it() {
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
    // the home itself holds none of them
    assert!(row(&report, "native").get_bool("complete"));
    assert!(found(&report)
        .iter()
        .all(|left| left.get_text("machine") == Some("box")));
    let why = refusal(fix(&home, &format!("box:run-folder:{kept}")), "InUse");
    assert!(why.contains("a Sandbox record names"), "{why}");
    assert!(paths::run_root(&root).join(&kept).is_dir());

    // a staged snapshot on the machine is removed under the home's lock
    let staged = "box:backup-staging:shop/store.sqlite.backup";
    let held = Creating::take(&home, "shop").unwrap();
    let why = refusal(fix(&home, staged), "InUse");
    assert!(why.contains("backup create"), "{why}");
    drop(held);

    // the folder copy: unnamed it is only reported; named, it is removable
    // an hour after its Init's deadline, by the worker's own discard
    let copy = format!("box:init-copy:{COPY}");
    refusal(fix(&home, &copy), "InvalidInput");
    name_copy(&home, 60_000);
    let left = one(&doctor(&home), "box", Kind::InitCopy);
    assert!(left.get_bool("fixable"), "{left:?}");
    let not_before = left.get_int("not_before").unwrap();
    assert!(not_before > home::now_ms() + 3_500_000, "{left:?}");
    let why = refusal(fix(&home, &copy), "InUse");
    assert!(why.contains("deadline"), "{why}");
    // a deadline that passed, but less than an hour ago
    name_copy(&home, -600_000);
    refusal(fix(&home, &copy), "InUse");
    assert!(root
        .join(format!("incoming/init-{COPY}/src/main.rs"))
        .is_file());

    fixed(&home, "box", &root, &outside);

    name_copy(&home, -2 * 3_600_000);
    let removed = fix(&home, &copy).unwrap();
    assert_eq!(removed.get_text("kind"), Some("init-copy"));
    assert!(!root.join(format!("incoming/init-{COPY}")).exists());
    assert!(found(&doctor(&home)).is_empty());
    refusal(fix(&home, &copy), "NotFound");
    placed.remove();
}

#[test]
fn a_stopped_machine_is_unavailable_and_a_lost_answer_unknown() {
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
    let id = format!("off:image-part:{HEX}");
    let why = refusal(fix(&home, &id), "Unavailable");
    assert!(why.contains("machine start off"), "{why}");
    refusal(
        fix(&home, "off:run-folder:layerfs-00000000-x"),
        "Unavailable",
    );
    // and its leftovers are not in the report: nothing is started to ask
    assert!(row_names(&doctor(&home)) == ["native"]);

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
    let why = refusal(fix(&home, &format!("lost:image-part:{HEX}")), "Unknown");
    assert!(
        why.contains("not resent") || why.contains("may have run"),
        "{why}"
    );
    let request = served.join().unwrap().expect("the request was sent once");
    assert_eq!(request.get_text("op"), Some("doctor.fix"));
    let args = request.get("args").unwrap();
    // a kind and a name travel, never a path
    assert_eq!(args.get_text("kind"), Some("image-part"));
    assert_eq!(args.get_text("name"), Some(HEX));
    assert!(args.get("path").is_none(), "{args:?}");
    std::fs::remove_dir_all(home.parent().unwrap()).unwrap();
}

fn row_names(report: &SdkValue) -> Vec<String> {
    let Some(SdkValue::List(rows)) = report.get("leftovers") else {
        panic!("no leftovers rows: {report:?}");
    };
    (rows.iter())
        .map(|row| row.get_text("machine").unwrap().to_owned())
        .collect()
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
    let id = |sandbox: &str| format!("native:run-folder:{}", name(sandbox));

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
    let earlier_id = format!("native:run-folder:lfs-{id8}-live");
    assert_eq!(listed, [earlier_id.clone()], "{report:?}");
    let why = refusal(fix(&home, &id("live")), "InUse");
    assert!(why.contains("a Sandbox record names"), "{why}");
    let why = refusal(fix(&home, &earlier_id), "InUse");
    assert!(why.contains("live daemon"), "{why}");
    assert!(earlier.join("daemon.log").is_file() && recorded.join("daemon.log").is_file());
    // once that daemon ended, the earlier folder is removable; the recorded
    // one never is
    daemon.kill().unwrap();
    daemon.wait().unwrap();
    fix(&home, &earlier_id).unwrap();
    assert!(!earlier.exists());
    refusal(fix(&home, &id("live")), "InUse");

    // no `owner` marker: not known to be a finished start's folder
    let bare = dir(&run.join(name("bare")));
    file(&bare.join("daemon.log"), 3);
    assert!(ids(&doctor(&home)).contains(&id("bare")));
    let why = refusal(fix(&home, &id("bare")), "InUse");
    assert!(why.contains("no `owner` marker"), "{why}");
    // a marker just written: its Sandbox may be starting
    let fresh = run_folder(&home, &name("fresh"), "fresh", &outside);
    aged(&fresh.join("owner"), Duration::ZERO);
    let why = refusal(fix(&home, &id("fresh")), "InUse");
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
    refusal(fix(&home, &id("soon")), "InUse");
    for kept in [&bare, &fresh, &creating] {
        assert!(kept.join("daemon.log").is_file(), "{}", kept.display());
    }

    // an empty private root beside its run folder belongs to that Sandbox
    dir(&run.join(format!("{}.root", name("bare"))));
    assert!(!ids(&doctor(&home))
        .iter()
        .any(|id| id.contains("empty-root")));
    let why = refusal(
        fix(&home, &format!("native:empty-root:{}", name("bare"))),
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
    let staged = "native:backup-staging:shop/store.sqlite.backup";
    let held = Creating::take(&home, "shop").unwrap();
    let why = refusal(fix(&home, staged), "InUse");
    assert!(why.contains("backup create"), "{why}");
    drop(held);

    // an unpack of the image holds its lock
    let lock = (std::fs::OpenOptions::new().read(true).write(true))
        .open(cache.join(format!("{HEX}.2.lock")))
        .unwrap();
    let lock = nix::fcntl::Flock::lock(lock, nix::fcntl::FlockArg::LockExclusiveNonblock)
        .map_err(|(_, error)| error)
        .unwrap();
    let tmp = format!("native:image-tmp:{HEX}.2-ab12");
    let why = refusal(fix(&home, &tmp), "InUse");
    assert!(why.contains("lock"), "{why}");
    drop(lock);

    // an image part written within the hour may still be received
    let part = cache.join(format!(".recv-{HEX}.part"));
    aged(&part, Duration::from_secs(600));
    let why = refusal(fix(&home, &format!("native:image-part:{HEX}")), "InUse");
    assert!(why.contains("unchanged"), "{why}");

    // nothing was removed, and with each guard free again all of it goes
    for path in [
        paths::store_dir(&home, "shop").join("store.sqlite.backup"),
        cache.join(format!(".tmp-{HEX}.2-ab12/usr/bin/tool")),
        part.clone(),
    ] {
        assert!(path.is_file(), "{}", path.display());
    }
    aged(&part, Duration::from_secs(7200));
    fixed(&home, "native", &home, &outside);

    // the kinds that are only counted are never removed
    let lock = format!("native:image-lock:{HEX}.2");
    let why = refusal(fix(&home, &lock), "InvalidInput");
    assert!(why.contains("only reported"), "{why}");
    refusal(fix(&home, "native:unmount-marker:s1/7"), "InvalidInput");
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
    assert_eq!(before.len(), 6, "a link is never a leftover: {before:?}");

    let absolute = format!("native:run-folder:{}", outside.display());
    let crafted = [
        "",
        "native",
        "native:run-folder",
        "native:run-folder:",
        "native:run-folder:..",
        "native:run-folder:.",
        "native:run-folder:../../../outside",
        "native:run-folder:a/b",
        "native:run-folder:images",
        "native:run-folder:.hidden",
        absolute.as_str(),
        "native:empty-root:..",
        "native:empty-root:../x",
        "native:backup-staging:../outside/backup.staging",
        "native:backup-staging:shop/../../layerfs.json",
        "native:backup-staging:shop/store.sqlite",
        "native:backup-staging:shop/record.json",
        "native:image-tmp:../../x",
        "native:image-part:../x",
        "native:init-copy:../../outside",
        "native:nothing:x",
        "../x:run-folder:y",
        "NATIVE:run-folder:y",
    ];
    for id in crafted {
        assert!(leftovers::parse(id).is_none(), "{id}");
        refusal(fix(&home, id), "NotFound");
    }
    // well-formed ids that name a link, or an entry behind one
    for id in [
        format!("native:run-folder:layerfs-{id8}-link"),
        format!("native:empty-root:layerfs-{id8}-away"),
        "native:backup-staging:evil/backup.staging".to_owned(),
        "native:backup-staging:evil/store.sqlite.backup".to_owned(),
        "native:backup-staging:shop/backup.staging".to_owned(),
    ] {
        assert!(leftovers::parse(&id).is_some(), "{id}");
        refusal(fix(&home, &id), "NotFound");
    }
    // the removing side refuses the same names when asked directly, as a
    // worker is
    for (kind, name) in [
        (Kind::RunFolder, ".."),
        (Kind::RunFolder, "../../../outside"),
        (Kind::EmptyRoot, "x/../.."),
        (Kind::BackupStaging, "../outside/backup.staging"),
        (Kind::ImageTmp, "../../x"),
        (Kind::ImagePart, "../x"),
    ] {
        refusal(fix_on(&home, kind, name, &[]), "NotFound");
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
        fix(&other, &format!("native:run-folder:layerfs-{other8}-x")),
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
    fix(&home, &format!("native:run-folder:layerfs-{id8}-gone")).unwrap();
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
    assert_eq!(count(&whole), 6);
    // too few entries: it stops, says so, and measures nothing it did not
    // finish
    let few = Bounds {
        entries: 4,
        ..BOUNDS
    };
    let cut = leftovers::walk(&home, &[], few);
    assert!(!cut.get_bool("complete"), "{cut:?}");
    assert!(count(&cut) < 6, "{cut:?}");
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
