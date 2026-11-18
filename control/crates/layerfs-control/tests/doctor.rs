//! The home doctor's records check (`host.records`, #536 H7): a home with
//! one of each problem gives one finding each, naming the thing it is about
//! and the call that fixes it; a clean home has no findings; a layout 2 home
//! reports what refuses its conversion; and the doctor writes nothing in
//! the home. The check reads records only: it dials no worker or daemon.
#[path = "support/doctor_own.rs"]
mod doctor_own;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_control::doctor::{Kind, Options};
use layerfs_home::home::{paths, private_dir, write_entity, write_record};
use std::collections::BTreeMap;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// The home doctor's records findings as the whole-home report showed
/// them: `{ ok, findings: [{ severity, entity: { kind, name }, message, fix
/// }] }`, `ok` false when one is an error.
fn doctor(home: &Path) -> Result<SdkValue, SdkRefusal> {
    let shown = home.display().to_string();
    let report = doctor_own::own(home, Kind::Home, &shown, &Options::default());
    let Some(SdkValue::List(found)) = report.get("findings") else {
        panic!("no findings: {report:?}")
    };
    let found: Vec<SdkValue> = (found.iter())
        .filter(|finding| finding.get_text("check") == Some("host.records"))
        .map(|finding| {
            let SdkValue::Map(fields) = finding else {
                panic!("{finding:?}")
            };
            let (kind, name) = fields.first().cloned().unwrap();
            SdkValue::map([
                ("severity", finding.get("severity").cloned().unwrap()),
                (
                    "entity",
                    SdkValue::map([("kind", SdkValue::Text(kind)), ("name", name)]),
                ),
                ("message", finding.get("message").cloned().unwrap()),
                ("fix", finding.get("fix").cloned().unwrap_or_default()),
            ])
        })
        .collect();
    let ok = !found
        .iter()
        .any(|f| f.get_text("severity") == Some("error"));
    Ok(SdkValue::map([
        ("ok", SdkValue::Bool(ok)),
        ("findings", SdkValue::List(found)),
    ]))
}

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

/// The run root's ancestors are 0700, or 0701 once a namespaces Sandbox's
/// commands must pass through them (`home::traversable`): doctor accepts
/// both and its message names both (layerfs-project#180). A folder its
/// group or others may list is still an error.
#[test]
fn a_home_folder_is_0700_or_0701_and_the_message_names_both() {
    let home = new_home("modes-0701");
    layerfs_control::migrate::open(&home).unwrap();
    let native = SdkValue::map([("name", t("native")), ("platform", t("native"))]);
    write_entity(&paths::machine_record(&home, "native"), &native).unwrap();
    let run = paths::run_root(&home);
    std::fs::create_dir(&run).unwrap();
    std::fs::set_permissions(&run, std::fs::Permissions::from_mode(0o755)).unwrap();
    let ancestors = [home.join("machine"), paths::machine_dir(&home, "native")];
    for mode in [0o700, 0o701] {
        for folder in &ancestors {
            std::fs::set_permissions(folder, std::fs::Permissions::from_mode(mode)).unwrap();
        }
        let report = doctor(&home).unwrap();
        assert_eq!(report.get("ok"), Some(&SdkValue::Bool(true)), "{report:?}");
        assert!(findings(&report).is_empty(), "{mode:o}: {report:?}");
    }
    std::fs::set_permissions(&ancestors[0], std::fs::Permissions::from_mode(0o755)).unwrap();
    let report = doctor(&home).unwrap();
    assert_eq!(report.get("ok"), Some(&SdkValue::Bool(false)), "{report:?}");
    let found = findings(&report);
    assert_eq!(found.len(), 1, "{found:?}");
    let (severity, _, _, message, fix) = &found[0];
    assert_eq!(severity, "error");
    for part in ["mode 0755", "0700", "0701"] {
        assert!(message.contains(part), "{part}: {message}");
    }
    assert!(fix.contains("chmod g-rwx,o-rw"), "{fix}");
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
fn a_symlinked_store_folder_is_an_error_that_does_not_change_the_target() {
    // the home's `store/` as a link: its own mode is 0777, not the folder's,
    // and the remedy must not chmod through the link into its target
    let home = new_home("store-link");
    layerfs_control::migrate::open(&home).unwrap();
    let elsewhere =
        std::env::temp_dir().join(format!("layerfs-doctor-link-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&elsewhere);
    std::fs::create_dir(&elsewhere).unwrap();
    std::fs::set_permissions(&elsewhere, std::fs::Permissions::from_mode(0o700)).unwrap();
    let store = home.join("store");
    let _ = std::fs::remove_dir_all(&store);
    std::os::unix::fs::symlink(&elsewhere, &store).unwrap();
    let report = doctor(&home).unwrap();
    assert_eq!(report.get("ok"), Some(&SdkValue::Bool(false)), "{report:?}");
    let found = findings(&report);
    assert_eq!(found.len(), 1, "{found:?}");
    let (severity, kind, _name, message, fix) = &found[0];
    assert_eq!((severity.as_str(), kind.as_str()), ("error", "home"));
    assert!(message.contains("symlink"), "{message}");
    assert!(!fix.contains("chmod"), "{fix}");
    std::fs::remove_file(&store).unwrap();
    std::fs::remove_dir_all(&elsewhere).unwrap();
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_symlinked_store_entry_is_an_error_that_does_not_change_the_target() {
    // `store/<name>` as a link: the daemon refuses it, so doctor reports it;
    // the remedy is not a chmod, which would change the link's target
    let home = new_home("store-entry-link");
    layerfs_control::migrate::open(&home).unwrap();
    store(&home, "shop", sqlite("shop", "native"));
    let elsewhere =
        std::env::temp_dir().join(format!("layerfs-doctor-entry-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&elsewhere);
    std::fs::rename(paths::store_dir(&home, "shop"), &elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, paths::store_dir(&home, "shop")).unwrap();
    let report = doctor(&home).unwrap();
    assert_eq!(report.get("ok"), Some(&SdkValue::Bool(false)), "{report:?}");
    let found = findings(&report);
    assert_eq!(found.len(), 1, "{found:?}");
    let (severity, kind, name, message, fix) = &found[0];
    assert_eq!(
        (severity.as_str(), kind.as_str(), name.as_str()),
        ("error", "store", "shop")
    );
    assert!(message.contains("symlink"), "{message}");
    assert!(!fix.contains("chmod"), "{fix}");
    std::fs::remove_file(paths::store_dir(&home, "shop")).unwrap();
    std::fs::remove_dir_all(&elsewhere).unwrap();
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_symlink_to_a_file_under_store_does_not_abort_other_findings() {
    // `store/<name>` linked to a regular file: must not fail `read_dir(.../project)`
    // with ENOTDIR and discard findings for the rest of the home
    let home = new_home("store-entry-file-link");
    layerfs_control::migrate::open(&home).unwrap();
    let native = SdkValue::map([("name", t("native")), ("platform", t("native"))]);
    write_entity(&paths::machine_record(&home, "native"), &native).unwrap();
    store(&home, "other", sqlite("other", "native"));
    std::fs::set_permissions(
        paths::store_dir(&home, "other"),
        std::fs::Permissions::from_mode(0o701),
    )
    .unwrap();
    let target = std::env::temp_dir().join(format!("layerfs-doctor-file-{}", std::process::id()));
    let _ = std::fs::remove_file(&target);
    std::fs::write(&target, b"keep").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();
    let link = paths::store_dir(&home, "shop");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let report = doctor(&home).unwrap();
    assert_eq!(report.get("ok"), Some(&SdkValue::Bool(false)), "{report:?}");
    let found = findings(&report);
    assert_eq!(found.len(), 2, "{found:#?}");
    assert_eq!(
        (
            found[0].0.as_str(),
            found[0].1.as_str(),
            found[0].2.as_str()
        ),
        ("error", "store", "other")
    );
    assert!(found[0].3.contains("0701"), "{found:#?}");
    assert_eq!(
        (
            found[1].0.as_str(),
            found[1].1.as_str(),
            found[1].2.as_str()
        ),
        ("error", "store", "shop")
    );
    assert!(found[1].3.contains("symlink"), "{found:#?}");
    assert!(!found[1].4.contains("chmod"), "{found:#?}");

    assert!(std::fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(std::fs::read_link(&link).unwrap(), target);
    let target_meta = std::fs::symlink_metadata(&target).unwrap();
    assert!(target_meta.file_type().is_file());
    assert_eq!(target_meta.permissions().mode() & 0o7777, 0o644);
    assert_eq!(std::fs::read(&target).unwrap(), b"keep");

    std::fs::remove_file(&link).unwrap();
    std::fs::remove_file(&target).unwrap();
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_dangling_symlink_under_store_is_an_error_that_keeps_scanning() {
    let home = new_home("store-entry-dangling-link");
    layerfs_control::migrate::open(&home).unwrap();
    let native = SdkValue::map([("name", t("native")), ("platform", t("native"))]);
    write_entity(&paths::machine_record(&home, "native"), &native).unwrap();
    store(&home, "other", sqlite("other", "native"));
    std::fs::set_permissions(
        paths::store_dir(&home, "other"),
        std::fs::Permissions::from_mode(0o701),
    )
    .unwrap();
    let missing =
        std::env::temp_dir().join(format!("layerfs-doctor-missing-{}", std::process::id()));
    let _ = std::fs::remove_file(&missing);
    let link = paths::store_dir(&home, "dangling");
    std::os::unix::fs::symlink(&missing, &link).unwrap();

    let report = doctor(&home).unwrap();
    assert_eq!(report.get("ok"), Some(&SdkValue::Bool(false)), "{report:?}");
    let found = findings(&report);
    assert_eq!(found.len(), 2, "{found:#?}");
    assert_eq!(
        (
            found[0].0.as_str(),
            found[0].1.as_str(),
            found[0].2.as_str()
        ),
        ("error", "store", "dangling")
    );
    assert!(found[0].3.contains("symlink"), "{found:#?}");
    assert!(!found[0].4.contains("chmod"), "{found:#?}");
    assert_eq!(
        (
            found[1].0.as_str(),
            found[1].1.as_str(),
            found[1].2.as_str()
        ),
        ("error", "store", "other")
    );
    assert!(found[1].3.contains("0701"), "{found:#?}");
    assert_eq!(std::fs::read_link(&link).unwrap(), missing);
    assert!(!missing.exists());

    std::fs::remove_file(&link).unwrap();
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

    let before = tree(&home);
    let report = doctor(&home).unwrap();
    assert_eq!(before, tree(&home), "the doctor wrote in the home");
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
    // a command record's finding names its Workspace
    let stale = found.iter().find(|f| f.3.contains("is stale")).unwrap();
    assert_eq!(
        (stale.0.as_str(), stale.1.as_str(), stale.2.as_str()),
        ("warning", "workspace", &*format!("sb:{live}"))
    );
    assert!(
        stale.3.contains("stale") && stale.4.contains("exec-cancel sb:w e2"),
        "{stale:?}"
    );
    let leftover = found.iter().find(|f| f.2 == format!("sb:{left}")).unwrap();
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
