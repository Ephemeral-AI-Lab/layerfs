//! The home's own checks observe records without opening or changing a home.
#[path = "support/doctor_own.rs"]
mod doctor_own;
use doctor_own::{complete, outcome, own, own_within};
use layerfs_bridge::control::SdkValue;
use layerfs_control::doctor::{Kind, Options};
use layerfs_home::home::{paths, private_dir, write_entity, write_record};
use layerfs_home::op::Op;
use std::collections::BTreeMap;
use std::net::TcpListener;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT: AtomicU64 = AtomicU64::new(0);
fn temp() -> PathBuf {
    std::env::temp_dir().join(format!(
        "doctor-host-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}
fn run(home: &Path) -> SdkValue {
    checked(home, &Options::default())
}
fn checked(home: &Path, options: &Options) -> SdkValue {
    own(home, Kind::Home, &home.display().to_string(), options)
}
fn home(layout: i64) -> PathBuf {
    let home = temp();
    private_dir(&home).unwrap();
    write_record(
        &home.join("layerfs.json"),
        &SdkValue::map([
            ("layout", SdkValue::Int(layout)),
            ("id", t("0123456789abcdef0123456789abcdef")),
        ]),
    )
    .unwrap();
    home
}
fn tree(home: &Path) -> BTreeMap<PathBuf, (u64, i64, i64, u32)> {
    let mut result = BTreeMap::new();
    let mut pending = vec![home.to_owned()];
    while let Some(path) = pending.pop() {
        let meta = std::fs::symlink_metadata(&path).unwrap();
        result.insert(
            path.clone(),
            (meta.len(), meta.mtime(), meta.mtime_nsec(), meta.mode()),
        );
        if meta.is_dir() {
            for entry in std::fs::read_dir(path).unwrap() {
                pending.push(entry.unwrap().path());
            }
        }
    }
    result
}
/// The home's checks, in the order `calls.json` lists them and they run.
const ORDER: [&str; 8] = [
    "home.build",
    "home.prerequisites",
    "home.layout",
    "home.records",
    "home.bundle",
    "home.native_daemon",
    "home.artifacts",
    "home.capacity",
];
/// The report's rows name only the home's checks, each once, in order.
fn in_order(report: &SdkValue) {
    let at: Vec<usize> = doctor_own::rows(report)
        .iter()
        .map(|row| {
            let id = row.get_text("id").unwrap_or("");
            ORDER
                .iter()
                .position(|o| *o == id)
                .unwrap_or_else(|| panic!("{id}"))
        })
        .collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "{report:?}");
}
fn contains(report: &SdkValue, needle: &str) -> bool {
    layerfs_home::json::print(report).contains(needle)
}
#[test]
fn absent_home_stays_absent_and_reports_uninitialized() {
    let home = temp();
    let report = run(&home);
    assert!(!home.exists());
    // a home not made yet has no layout or records to check
    assert_eq!(outcome(&report, "home.layout"), None, "{report:?}");
    assert_eq!(outcome(&report, "home.records"), None, "{report:?}");
    assert!(outcome(&report, "home.native_daemon").is_some());
    // a home that is not there has no disk of its own: no ancestor is
    // measured, so the check is not made rather than passed
    assert_eq!(
        outcome(&report, "home.capacity").as_deref(),
        Some("unknown"),
        "{report:?}"
    );
    in_order(&report);
}
#[test]
fn current_and_foreign_layouts_are_observed_without_writing_files() {
    for layout in [2, 3] {
        let home = home(layout);
        let before = tree(&home);
        let report = run(&home);
        assert_eq!(before, tree(&home));
        if layout == 2 {
            // a foreign home is refused with the fix that moves it aside
            assert!(contains(&report, "not a LayerFS 2.0 home"));
            assert_eq!(outcome(&report, "home.layout").as_deref(), Some("fail"));
            let check = row(&report, "home.layout");
            let fix = layerfs_home::home::layout::foreign_fix(&home);
            assert_eq!(check.get_text("fix"), Some(fix.as_str()), "{check:?}");
            assert_eq!(outcome(&report, "home.records"), None, "{report:?}");
        } else {
            assert_eq!(outcome(&report, "home.layout").as_deref(), Some("pass"));
        }
        in_order(&report);
        std::fs::remove_dir_all(home).unwrap();
    }
}
#[test]
fn malformed_layout_fails_with_the_move_fix_and_changes_nothing() {
    // a `layerfs.json` that cannot be parsed is not a 2.0 home's (MIG,
    // RC-8): the layout fails with the move-aside fix
    for text in [
        "{not json",
        "{}",
        r#"{"layout":"bad"}"#,
        r#"{"layout":3,"id":"xyz"}"#,
    ] {
        let home = home(3);
        std::fs::write(home.join("layerfs.json"), text).unwrap();
        let before = tree(&home);
        let report = run(&home);
        assert_eq!(
            outcome(&report, "home.layout").as_deref(),
            Some("fail"),
            "{text}"
        );
        let check = row(&report, "home.layout");
        let fix = layerfs_home::home::layout::foreign_fix(&home);
        assert_eq!(check.get_text("fix"), Some(fix.as_str()), "{check:?}");
        // the records check waits on the layout: it does not apply
        assert_eq!(outcome(&report, "home.records"), None, "{report:?}");
        assert_eq!(before, tree(&home));
        std::fs::remove_dir_all(home).unwrap();
    }
}
#[test]
fn a_layerfs_json_the_system_refuses_to_read_fails_with_the_os_error_and_no_fix() {
    let home = home(3);
    let file = home.join("layerfs.json");
    std::fs::remove_file(&file).unwrap();
    // a folder in its place: EISDIR, for root too
    std::fs::create_dir(&file).unwrap();
    let before = tree(&home);
    let report = run(&home);
    assert_eq!(outcome(&report, "home.layout").as_deref(), Some("fail"));
    let check = row(&report, "home.layout");
    let message = format!("{}: Is a directory (os error 21)", file.display());
    assert_eq!(
        check.get_text("message"),
        Some(message.as_str()),
        "{check:?}"
    );
    assert_eq!(check.get("fix"), Some(&SdkValue::Null), "{check:?}");
    assert_eq!(outcome(&report, "home.records"), None, "{report:?}");
    assert_eq!(before, tree(&home));
    std::fs::remove_dir_all(home).unwrap();
}
#[test]
fn local_records_detect_missing_references_and_permissions_without_worker_dial() {
    let home = home(3);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    write_entity(
        &paths::machine_record(&home, "vm1"),
        &SdkValue::map([("platform", t("lima"))]),
    )
    .unwrap();
    write_record(
        &paths::worker_record(&home, "vm1"),
        &SdkValue::map([
            ("address", t(&listener.local_addr().unwrap().to_string())),
            ("secret", t("never-reveal")),
        ]),
    )
    .unwrap();
    write_entity(
        &paths::store_record(&home, "broken"),
        &SdkValue::map([("kind", t("sqlite")), ("machine", t("missing"))]),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(
        paths::store_record(&home, "broken"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let before = tree(&home);
    let report = run(&home);
    assert_eq!(before, tree(&home));
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert!(contains(&report, "machine missing"));
    assert!(contains(&report, "0644"));
    assert!(!contains(&report, "never-reveal"));
    std::fs::remove_dir_all(home).unwrap();
}
#[test]
fn stale_lease_and_dead_op_remain_while_live_op_is_not_stale() {
    let home = home(3);
    let live = Op::begin("store.remove", "retire", SdkValue::Null).unwrap();
    for (name, op) in [
        ("live", live.to_record().clone()),
        (
            "dead",
            SdkValue::map([
                ("id", t("op-00000000000000aa")),
                ("name", t("store.remove")),
                ("phase", t("retire")),
                ("owner", SdkValue::map([("pid", SdkValue::Int(0))])),
                ("uncertain", SdkValue::Bool(true)),
            ]),
        ),
    ] {
        write_entity(
            &paths::store_record(&home, name),
            &SdkValue::map([("kind", t("sqlite")), ("machine", t("native")), ("op", op)]),
        )
        .unwrap();
    }
    private_dir(&paths::lease_dir(&home, "dead")).unwrap();
    std::fs::write(paths::lease_dir(&home, "dead").join("99999999"), "1\n").unwrap();
    let before = tree(&home);
    let report = run(&home);
    assert_eq!(before, tree(&home));
    assert!(contains(&report, "Store dead holds a stale lease"));
    assert!(contains(&report, "store dead has an unresolved"));
    assert!(!contains(&report, "store live has an unresolved"));
    std::fs::remove_dir_all(home).unwrap();
}
#[test]
fn an_orphan_project_folder_is_an_error_with_the_prune_fix() {
    let home = home(3);
    write_entity(
        &paths::store_record(&home, "shop"),
        &SdkValue::map([("kind", t("sqlite")), ("machine", t("native"))]),
    )
    .unwrap();
    let orphan = paths::project_dir(&home, "shop", "orphan");
    private_dir(&orphan).unwrap();
    let report = run(&home);
    assert!(
        contains(&report, "project shop/orphan has no record"),
        "{report:?}"
    );
    assert!(
        contains(&report, "layerfs project prune shop/orphan"),
        "{report:?}"
    );
    // the doctor names the leftover; removing it is the call's effect
    assert!(orphan.exists(), "the doctor removed the entry");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn arbitrary_version_value_is_not_echoed_and_timeout_is_incomplete() {
    let home = home(3);
    write_record(
        &paths::store_record(&home, "bad"),
        &SdkValue::map([("v", t("secret-token-value"))]),
    )
    .unwrap();
    let report = run(&home);
    assert!(!contains(&report, "secret-token-value"));
    assert!(contains(&report, "non-integer"));
    // a budget exhausted before local inspection leaves checks not made
    let shown = home.display().to_string();
    let options = Options::default();
    let timed = own_within(
        &home,
        Kind::Home,
        &shown,
        &options,
        Some(Duration::from_nanos(1)),
    );
    assert!(!complete(&timed), "{timed:?}");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn runtime_mount_and_log_subtrees_are_outside_record_fingerprinting() {
    use std::os::unix::fs::PermissionsExt;
    let home = home(3);
    layerfs_machine::record::MachineRecord::built_in("native", "0123456789abcdef0123456789abcdef")
        .write(&home)
        .unwrap();
    let runtime = paths::machine_dir(&home, "native").join("run");
    private_dir(&runtime).unwrap();
    let trap = runtime.join("unreadable-runtime-tree");
    private_dir(&trap).unwrap();
    std::fs::set_permissions(&trap, std::fs::Permissions::from_mode(0o0)).unwrap();
    let report = run(&home);
    assert_eq!(outcome(&report, "home.records").as_deref(), Some("pass"));
    assert_eq!(std::fs::metadata(&trap).unwrap().mode() & 0o777, 0);
    std::fs::set_permissions(&trap, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::remove_dir_all(home).unwrap();
}
#[test]
fn concurrent_management_record_rewrites_leave_consistency_incomplete() {
    use std::sync::{atomic::AtomicBool, Arc};
    let home = home(3);
    for n in 0..128 {
        write_entity(
            &paths::machine_record(&home, &format!("m{n}")),
            &SdkValue::map([("platform", t("lima"))]),
        )
        .unwrap();
    }
    let path = paths::machine_record(&home, "racing");
    write_entity(&path, &SdkValue::map([("platform", t("lima"))])).unwrap();
    let done = Arc::new(AtomicBool::new(false));
    let flag = done.clone();
    let writer = std::thread::spawn(move || {
        let mut epoch = 0;
        while !flag.load(Ordering::Relaxed) {
            epoch += 1;
            write_entity(
                &path,
                &SdkValue::map([
                    ("platform", t("lima")),
                    ("race_epoch", SdkValue::Int(epoch)),
                ]),
            )
            .unwrap();
            std::thread::yield_now();
        }
        epoch
    });
    let report = run(&home);
    done.store(true, Ordering::Relaxed);
    assert!(writer.join().unwrap() > 0);
    assert_eq!(outcome(&report, "home.records").as_deref(), Some("unknown"));
    assert!(!complete(&report));
    std::fs::remove_dir_all(home).unwrap();
}

/// The row `id` of a report.
fn row(report: &SdkValue, id: &str) -> SdkValue {
    doctor_own::row(report, id).unwrap_or_else(|| panic!("no {id}: {report:?}"))
}

#[test]
fn the_native_daemon_check_reads_the_recorded_daemon_and_its_build() {
    use layerfs_control::home::Build;
    use std::os::unix::fs::PermissionsExt;
    let home = home(3);
    let ours = "1111111111111111111111111111111111111111";
    let theirs = "2222222222222222222222222222222222222222";
    let daemon = home.join("layerfs-daemon");
    std::fs::write(
        &daemon,
        format!("binary layerfs-build-identity:{theirs}:clean more"),
    )
    .unwrap();
    std::fs::set_permissions(&daemon, std::fs::Permissions::from_mode(0o755)).unwrap();
    let shown = daemon.display().to_string();
    write_entity(
        &paths::machine_record(&home, "native"),
        &SdkValue::map([
            ("name", t("native")),
            ("platform", t("native")),
            ("daemon", t(&shown)),
        ]),
    )
    .unwrap();
    let with = |commit: &'static str| {
        let options = Options {
            build: Build {
                commit: Some(commit),
                dirty: false,
            },
            ..Options::default()
        };
        checked(&home, &options)
    };
    // another commit: home.get's DaemonMismatch words, as a warning
    let other = with(ours);
    let check = row(&other, "home.native_daemon");
    assert_eq!(check.get_text("severity"), Some("warning"), "{check:?}");
    assert_eq!(
        check.get_text("message"),
        Some(
            format!(
                "the native daemon at {shown} was built from 22222222, this build from 11111111; record this build's daemon with `layerfs machine update native --daemon <path>`"
            )
            .as_str()
        )
    );
    let evidence = check.get("evidence").unwrap();
    assert_eq!(evidence.get_text("source"), Some("recorded"));
    assert_eq!(evidence.get_text("commit"), Some(theirs));
    assert_eq!(evidence.get("matches"), Some(&SdkValue::Bool(false)));
    // the same commit, both clean
    let same = with(theirs);
    let check = row(&same, "home.native_daemon");
    assert_eq!(check.get_text("outcome"), Some("pass"), "{check:?}");
    // a build that names no commit cannot compare: it passes
    let unknown = run(&home);
    let check = row(&unknown, "home.native_daemon");
    assert_eq!(check.get_text("outcome"), Some("pass"), "{check:?}");
    // the native machine's daemon is reported here alone, not by home.artifacts
    assert!(
        !doctor_own::rows(&unknown)
            .iter()
            .any(|r| r.get_text("id") == Some("home.artifacts")
                && r.get_text("message").is_some_and(|m| m.contains("native"))),
        "{unknown:?}"
    );
    // a recorded daemon that is there and not executable says so
    std::fs::set_permissions(&daemon, std::fs::Permissions::from_mode(0o644)).unwrap();
    let plain = run(&home);
    let check = row(&plain, "home.native_daemon");
    assert_eq!(check.get_text("severity"), Some("error"), "{check:?}");
    assert_eq!(
        check.get_text("message"),
        Some(
            format!(
                "the native daemon at {shown} is not executable; `chmod +x` it or record another: `layerfs machine update native --daemon <path>`"
            )
            .as_str()
        )
    );
    // a recorded daemon that is not there is an error, in the Sandbox's words
    std::fs::remove_file(&daemon).unwrap();
    let gone = run(&home);
    let check = row(&gone, "home.native_daemon");
    assert_eq!(check.get_text("severity"), Some("error"), "{check:?}");
    assert_eq!(
        check.get_text("message"),
        Some(
            format!(
                "machine native's recorded daemon {shown} is not there; record another with `layerfs machine update native --daemon <path>`"
            )
            .as_str()
        )
    );
    std::fs::remove_dir_all(home).unwrap();
}

/// Another home's id.
const OTHER_HOME_ID: &str = "fedcba9876543210fedcba9876543210";

/// Rewrites `layerfs.json` with `id`.
fn write_marker(home: &Path, id: &str) {
    write_record(
        &home.join("layerfs.json"),
        &SdkValue::map([("layout", SdkValue::Int(3)), ("id", t(id))]),
    )
    .unwrap();
}

/// A machine record with a `vm_name`, as `machine create` writes one.
fn machine_with_vm(home: &Path, name: &str, vm_name: &str) {
    write_entity(
        &paths::machine_record(home, name),
        &SdkValue::map([
            ("name", t(name)),
            ("platform", t(name)),
            ("vm_name", t(vm_name)),
        ]),
    )
    .unwrap();
}

/// A started Sandbox record whose `daemon` names its run folder.
fn sandbox_with_daemon(home: &Path, name: &str, daemon: &str) {
    write_entity(
        &paths::sandbox_record(home, name),
        &SdkValue::map([
            ("name", t(name)),
            ("project", t("shop/api")),
            ("machine", t("native")),
            ("daemon", SdkValue::map([("name", t(daemon))])),
        ]),
    )
    .unwrap();
}

/// Asserts `home.layout` failed and answers its row.
fn failed_layout(report: &SdkValue) -> SdkValue {
    assert_eq!(
        outcome(report, "home.layout").as_deref(),
        Some("fail"),
        "{report:?}"
    );
    row(report, "home.layout")
}

#[test]
fn a_forged_id_does_not_match_a_machine_record() {
    let home = home(3);
    write_marker(&home, OTHER_HOME_ID);
    machine_with_vm(&home, "docker", "layerfs-0123456789abcdef-docker");
    let before = tree(&home);
    let report = run(&home);
    let check = failed_layout(&report);
    assert_eq!(check.get_text("severity"), Some("error"), "{check:?}");
    assert_eq!(check.get("fix"), Some(&SdkValue::Null), "{check:?}");
    let message = check.get_text("message").unwrap_or_default();
    assert!(
        message.contains(&paths::machine_record(&home, "docker").display().to_string()),
        "{message}"
    );
    assert!(
        message.contains("layerfs-0123456789abcdef-docker"),
        "{message}"
    );
    assert!(
        message.contains("layerfs-fedcba9876543210-docker"),
        "{message}"
    );
    assert_eq!(before, tree(&home), "the doctor wrote in the home");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_matching_machine_record_passes_the_layout() {
    let home = home(3);
    machine_with_vm(&home, "docker", "layerfs-0123456789abcdef-docker");
    let report = run(&home);
    assert_eq!(
        outcome(&report, "home.layout").as_deref(),
        Some("pass"),
        "{report:?}"
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn the_earlier_eight_hex_machine_name_passes() {
    let home = home(3);
    machine_with_vm(&home, "docker", "layerfs-01234567-docker");
    let report = run(&home);
    assert_eq!(
        outcome(&report, "home.layout").as_deref(),
        Some("pass"),
        "{report:?}"
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_bare_home_and_a_nameless_record_pass() {
    let bare = home(3);
    let report = run(&bare);
    assert_eq!(
        outcome(&report, "home.layout").as_deref(),
        Some("pass"),
        "{report:?}"
    );
    std::fs::remove_dir_all(bare).unwrap();

    let home = home(3);
    write_entity(
        &paths::machine_record(&home, "vm1"),
        &SdkValue::map([("name", t("vm1")), ("platform", t("lima"))]),
    )
    .unwrap();
    let report = run(&home);
    assert_eq!(
        outcome(&report, "home.layout").as_deref(),
        Some("pass"),
        "{report:?}"
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_deleted_record_leaves_no_conflict() {
    let home = home(3);
    machine_with_vm(&home, "docker", "layerfs-fedcba9876543210-docker");
    std::fs::remove_dir_all(paths::machine_dir(&home, "docker")).unwrap();
    let report = run(&home);
    assert_eq!(
        outcome(&report, "home.layout").as_deref(),
        Some("pass"),
        "{report:?}"
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn sandbox_daemon_names_of_three_eras_pass() {
    let home = home(3);
    sandbox_with_daemon(&home, "live16", "layerfs-0123456789abcdef-live16");
    sandbox_with_daemon(&home, "live8", "layerfs-01234567-live8");
    sandbox_with_daemon(&home, "lfs8", "lfs-01234567-lfs8");
    let report = run(&home);
    assert_eq!(
        outcome(&report, "home.layout").as_deref(),
        Some("pass"),
        "{report:?}"
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_sandbox_daemon_name_from_another_home_fails_the_layout() {
    let home = home(3);
    sandbox_with_daemon(&home, "live", "layerfs-fedcba9876543210-live");
    let before = tree(&home);
    let report = run(&home);
    let check = failed_layout(&report);
    assert_eq!(check.get_text("severity"), Some("error"), "{check:?}");
    assert_eq!(check.get("fix"), Some(&SdkValue::Null), "{check:?}");
    let message = check.get_text("message").unwrap_or_default();
    assert!(
        message.contains(&paths::sandbox_record(&home, "live").display().to_string()),
        "{message}"
    );
    assert!(
        message.contains("layerfs-fedcba9876543210-live"),
        "{message}"
    );
    assert!(
        message.contains("layerfs-0123456789abcdef-live"),
        "{message}"
    );
    assert_eq!(before, tree(&home), "the doctor wrote in the home");
    std::fs::remove_dir_all(home).unwrap();
}

/// Makes `home` look like a machine's worker root, with no `layerfs.json`.
fn worker_shape(home: &Path) {
    for folder in [
        "store/shop",
        "sandbox/live",
        "machine/native/run",
        "log",
        "incoming",
    ] {
        std::fs::create_dir_all(home.join(folder)).unwrap();
    }
    std::fs::write(home.join("log/worker.log"), "worker started\n").unwrap();
    std::fs::create_dir_all(home.join("incoming/init-0123456789abcdef")).unwrap();
}

#[test]
fn a_worker_root_fails_the_layout_and_gets_no_records_check() {
    // the incident's shape: never a home, and moved nowhere
    let home = temp();
    std::fs::create_dir_all(&home).unwrap();
    worker_shape(&home);
    let before = tree(&home);
    let report = run(&home);
    let check = failed_layout(&report);
    assert_eq!(check.get_text("severity"), Some("error"), "{check:?}");
    assert!(
        check
            .get_text("message")
            .is_some_and(|message| message.contains("worker root")),
        "{check:?}"
    );
    let fix = check.get_text("fix").unwrap_or_default();
    assert!(!fix.contains("mv "), "{fix}");
    assert!(!fix.contains("home init"), "{fix}");
    assert_eq!(outcome(&report, "home.records"), None, "{report:?}");
    assert_eq!(before, tree(&home), "the doctor wrote in the worker root");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_home_that_lost_its_marker_fails_the_layout_with_the_adopt_fix() {
    // the data is there and the marker is not: not a foreign folder, never
    // moved, and the records check does not run (layerfs-project#752, c6)
    let home = temp();
    std::fs::create_dir_all(home.join("store/shop/project/demo")).unwrap();
    std::fs::write(home.join("store/shop/record.json"), "{}").unwrap();
    std::fs::create_dir_all(home.join("trace")).unwrap();
    std::fs::write(home.join("trace/calls.jsonl"), "").unwrap();
    let before = tree(&home);
    let report = run(&home);
    let check = failed_layout(&report);
    assert_eq!(check.get_text("severity"), Some("error"), "{check:?}");
    assert!(
        check
            .get_text("message")
            .is_some_and(|message| message.contains("no layerfs.json")),
        "{check:?}"
    );
    let fix = check.get_text("fix").unwrap_or_default();
    assert_eq!(fix, "layerfs home init --adopt");
    assert!(!fix.contains("mv "), "{fix}");
    assert_eq!(outcome(&report, "home.records"), None, "{report:?}");
    assert_eq!(before, tree(&home), "the doctor wrote in the data home");
    std::fs::remove_dir_all(home).unwrap();
}
