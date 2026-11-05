//! The Sandbox scratch lifecycle (#549): a record owns its scratch with an
//! absolute root, `createdAt` and a generation; a cleaner removes only that
//! root, only with the generation the plan was made with, and never an
//! `Unresolved` Sandbox's scratch on an automatic pass. Every case seeds
//! what must survive and asserts it explicitly.
use layerfs_bridge::control::SdkValue;
use layerfs_control::common::json::print;
use layerfs_control::sandbox::record::SandboxRecord;
use layerfs_control::sandbox::scratch::{self, Cleaned, Kept, Mode, Owned, Owner, Scratch};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A fresh home with a Store and the local runtime root.
fn fresh_home() -> PathBuf {
    let home = std::env::temp_dir().join(format!(
        "layerfs-scratch-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(home.join("store/shop")).unwrap();
    fs::create_dir_all(home.join("sandbox")).unwrap();
    fs::create_dir_all(home.join("machine/native/run")).unwrap();
    home
}

/// The scratch root of Sandbox `name` on the native run root.
fn root(home: &Path, name: &str) -> PathBuf {
    home.join("machine/native/run")
        .join(format!("lfs-x-{name}"))
        .join("tmp")
}

/// `path` with its scratch root and one file in it.
fn seed(root: &Path) -> PathBuf {
    fs::create_dir_all(root).unwrap();
    let file = root.join("commands-left-this");
    fs::write(&file, "scratch").unwrap();
    file
}

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

/// A record with `name`, `machine`, an optional id and an optional `op`.
fn record_value(
    name: &str,
    machine: &str,
    id: Option<&str>,
    op: Option<SdkValue>,
    scratch_root: &Path,
    generation: u64,
) -> SdkValue {
    let mut fields = vec![
        ("name".to_owned(), text(name)),
        ("project".to_owned(), text("shop/api")),
        ("machine".to_owned(), text(machine)),
        ("created".to_owned(), SdkValue::Int(1)),
        (
            scratch::FIELD.to_owned(),
            Scratch::new(scratch_root.to_path_buf(), 1, generation)
                .unwrap()
                .to_record(),
        ),
    ];
    if let Some(id) = id {
        fields.push(("id".to_owned(), text(id)));
    }
    if let Some(op) = op {
        fields.push(("op".to_owned(), op));
    }
    SdkValue::Map(fields)
}

/// Writes `value` as `home/sandbox/<name>/record.json`.
fn write_record(home: &Path, name: &str, value: &SdkValue) {
    let dir = home.join("sandbox").join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("record.json"), print(value)).unwrap();
}

/// A gone record: no id, no intent, so its state is derived as `gone`.
fn gone(home: &Path, name: &str, machine: &str, generation: u64) -> PathBuf {
    let root = root(home, name);
    seed(&root);
    write_record(
        home,
        name,
        &record_value(name, machine, None, None, &root, generation),
    );
    root
}

/// The record of a close (no intent, an id: state `running`).
fn running(home: &Path, name: &str, machine: &str, generation: u64) -> PathBuf {
    let root = root(home, name);
    seed(&root);
    write_record(
        home,
        name,
        &record_value(name, machine, Some(name), None, &root, generation),
    );
    root
}

/// A record whose create/close intent has a gone owner and an unknown
/// outcome: derived state `unresolved`.
fn unresolved(home: &Path, name: &str, machine: &str, generation: u64) -> PathBuf {
    let root = root(home, name);
    seed(&root);
    let op = SdkValue::map([
        ("id", text("op-00000000000000aa")),
        ("name", text("sandbox.close")),
        ("phase", text("stop")),
        ("owner", SdkValue::map([("pid", SdkValue::Int(0))])),
        ("uncertain", SdkValue::Bool(true)),
    ]);
    write_record(
        home,
        name,
        &record_value(name, machine, Some(name), Some(op), &root, generation),
    );
    root
}

/// Asserts every seeded bystander still exists.
fn bystanders(home: &Path) {
    let files = [
        home.join("store/shop/store.sqlite"),
        home.join("sandbox/b/workspace/w.json"),
        home.join("machine/native/run/lfs-x-b/overlay/overlay.sqlite"),
        home.join("machine/native/run/lfs-x-b/workspaces/1/file"),
        home.join("machine/native/run/lfs-x-b/daemon.log"),
    ];
    for file in files {
        assert!(file.is_file(), "{} must survive", file.display());
    }
}

/// Seeds a Store, a Workspace and the run directory's non-scratch parts.
fn seed_bystanders(home: &Path) {
    for (path, body) in [
        (home.join("store/shop/store.sqlite"), "store"),
        (home.join("sandbox/b/workspace/w.json"), "workspace"),
        (
            home.join("machine/native/run/lfs-x-b/overlay/overlay.sqlite"),
            "overlay",
        ),
        (
            home.join("machine/native/run/lfs-x-b/workspaces/1/file"),
            "workspace",
        ),
        (home.join("machine/native/run/lfs-x-b/daemon.log"), "log"),
    ] {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }
}

#[test]
fn a_close_removes_only_the_scratch_root() {
    let home = fresh_home();
    seed_bystanders(&home);
    let root = root(&home, "b");
    let inside = seed(&root);
    let outside = home.join("machine/native/run/lfs-x-b/daemon.log");
    let value = record_value("b", "native", Some("b"), None, &root, 7);
    write_record(&home, "b", &value);
    let record = SandboxRecord::from_record("b", value);
    assert_eq!(
        scratch::clean(&home, &record, Owner::Close).unwrap(),
        Cleaned::Deleted(root.clone())
    );
    assert!(!inside.exists(), "the scratch root is gone");
    assert!(!root.exists(), "the scratch root is gone");
    assert!(
        outside.is_file(),
        "a sibling outside the root is not deleted"
    );
    bystanders(&home);
    assert_eq!(
        scratch::clean(&home, &record, Owner::Close).unwrap(),
        Cleaned::Deleted(root),
        "a second close is idempotent"
    );
}

#[test]
fn a_path_outside_the_recorded_root_is_never_deleted() {
    let home = fresh_home();
    seed_bystanders(&home);
    // a root under the Store or a Workspace is not a scratch, even though
    // its leaf is `tmp`
    for (dir, why) in [
        (home.join("store/shop/tmp"), Kept::NotScratch),
        (home.join("sandbox/b/workspace/w/tmp"), Kept::NotScratch),
    ] {
        seed(&dir);
        let record =
            SandboxRecord::from_record("b", record_value("b", "native", Some("b"), None, &dir, 1));
        assert_eq!(
            scratch::clean(&home, &record, Owner::Close).unwrap(),
            Cleaned::Kept(dir.clone(), why),
            "{}",
            dir.display()
        );
        assert!(dir.exists(), "{} must survive", dir.display());
    }
    // a scratch outside the home is not this home's to delete
    let elsewhere =
        std::env::temp_dir().join(format!("layerfs-scratch-elsewhere-{}", std::process::id()));
    let _ = fs::remove_dir_all(&elsewhere);
    seed(&elsewhere.join("tmp"));
    let record = SandboxRecord::from_record(
        "b",
        record_value("b", "native", Some("b"), None, &elsewhere.join("tmp"), 1),
    );
    assert_eq!(
        scratch::clean(&home, &record, Owner::Close).unwrap(),
        Cleaned::Kept(elsewhere.join("tmp"), Kept::Outside)
    );
    assert!(elsewhere.join("tmp/commands-left-this").is_file());
    let _ = fs::remove_dir_all(&elsewhere);
}

#[test]
fn the_host_tmp_is_never_a_scratch_root() {
    assert!(Scratch::new(PathBuf::from("/tmp"), 1, 1).is_err());
    assert!(Scratch::new(PathBuf::from("/private/tmp"), 1, 1).is_err());
    assert!(Scratch::new(PathBuf::from("/var/tmp"), 1, 1).is_err());
    // a real file in the host's temp area is untouched
    let host = std::env::temp_dir().join(format!(
        "layerfs-scratch-host-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let root = host.join("tmp");
    seed(&root);
    let home = fresh_home();
    let record =
        SandboxRecord::from_record("b", record_value("b", "native", Some("b"), None, &root, 1));
    assert_eq!(
        scratch::clean(&home, &record, Owner::Close).unwrap(),
        Cleaned::Kept(root.clone(), Kept::Outside)
    );
    assert!(root.join("commands-left-this").is_file());
    let _ = fs::remove_dir_all(&host);
}

#[test]
fn a_stale_generation_aborts_before_deleting_a_rebuilt_sandbox() {
    let home = fresh_home();
    let root = root(&home, "b");
    let inside = seed(&root);
    // the sweeper read generation 7 and planned; a rebuild then claimed the
    // same name with generation 8 before the delete
    let planned =
        SandboxRecord::from_record("b", record_value("b", "native", Some("b"), None, &root, 7));
    write_record(
        &home,
        "b",
        &record_value("b", "native", Some("b"), None, &root, 8),
    );
    assert_eq!(
        scratch::clean(&home, &planned, Owner::Close).unwrap(),
        Cleaned::Kept(root.clone(), Kept::Stale)
    );
    assert!(inside.is_file(), "the rebuilt Sandbox keeps its scratch");
}

#[test]
fn an_unresolved_sandbox_is_not_swept_but_an_explicit_close_cleans_it() {
    let home = fresh_home();
    let root = unresolved(&home, "b", "native", 3);
    let inside = root.join("commands-left-this");
    let report = scratch::sweep(&home, "native", Mode::Apply).unwrap();
    assert!(report.deleted.is_empty());
    assert_eq!(
        report.kept,
        vec![(Owned::new("b", root.clone()), Kept::Unresolved)]
    );
    assert!(
        inside.is_file(),
        "an automatic pass keeps unresolved scratch"
    );
    // the explicit owner is the close that clears the intent
    let record =
        SandboxRecord::from_record("b", record_value("b", "native", Some("b"), None, &root, 3));
    assert_eq!(
        scratch::clean(&home, &record, Owner::Close).unwrap(),
        Cleaned::Deleted(root.clone())
    );
    assert!(!root.exists());
}

#[test]
fn a_running_sandbox_keeps_its_scratch_during_a_sweep() {
    let home = fresh_home();
    let root = running(&home, "b", "native", 4);
    let report = scratch::sweep(&home, "native", Mode::Apply).unwrap();
    assert_eq!(
        report.kept,
        vec![(Owned::new("b", root.clone()), Kept::Live)]
    );
    assert!(root.join("commands-left-this").is_file());
}

#[test]
fn a_settled_gone_record_is_swept_and_nothing_else_is_deleted() {
    let home = fresh_home();
    seed_bystanders(&home);
    let root = gone(&home, "b", "native", 9);
    let report = scratch::sweep(&home, "native", Mode::Apply).unwrap();
    assert_eq!(report.deleted, vec![Owned::new("b", root.clone())]);
    assert!(!root.exists());
    bystanders(&home);
}

#[test]
fn dry_run_lists_exactly_what_apply_deletes() {
    let home = fresh_home();
    let a = gone(&home, "a", "native", 1);
    let b = gone(&home, "b", "native", 2);
    let live = running(&home, "c", "native", 3);
    let stuck = unresolved(&home, "d", "native", 4);
    let dry = scratch::sweep(&home, "native", Mode::DryRun).unwrap();
    assert_eq!(
        dry.planned,
        vec![Owned::new("a", a.clone()), Owned::new("b", b.clone())]
    );
    assert!(dry.deleted.is_empty());
    for path in [&a, &b, &live, &stuck] {
        assert!(
            path.exists(),
            "a dry run deletes nothing: {}",
            path.display()
        );
    }
    let applied = scratch::sweep(&home, "native", Mode::Apply).unwrap();
    assert_eq!(applied.deleted, dry.planned);
    assert!(!a.exists() && !b.exists());
    assert!(live.join("commands-left-this").is_file());
    assert!(stuck.join("commands-left-this").is_file());
}

#[test]
fn a_sweep_is_keyed_by_machine_never_by_isolation_class() {
    let home = fresh_home();
    let root = gone(&home, "b", "dev", 5);
    let elsewhere = scratch::sweep(&home, "native", Mode::Apply).unwrap();
    assert!(
        elsewhere.deleted.is_empty(),
        "another machine's pass keeps it"
    );
    assert!(root.join("commands-left-this").is_file());
    let mine = scratch::sweep(&home, "dev", Mode::Apply).unwrap();
    assert_eq!(mine.deleted, vec![Owned::new("b", root.clone())]);
    assert!(!root.exists());
}

#[test]
fn a_clean_failure_is_recorded_and_never_changes_a_successful_close() {
    let home = fresh_home();
    seed_bystanders(&home);
    let root = gone(&home, "b", "native", 9);
    let inside = root.join("commands-left-this");
    let record = SandboxRecord::from_record("b", record_value("b", "native", None, None, &root, 9));
    // the generation re-read before the delete cannot be parsed, so the
    // cleaner fails after deciding the root may go
    fs::write(home.join("sandbox/b/record.json"), "{not json").unwrap();
    assert!(scratch::clean(&home, &record, Owner::Close).is_err());
    // the cleaner `sandbox.close` calls cannot fail: it records the failure
    // and leaves the successful close's answer alone
    scratch::clean_on_close(&home, &record);
    assert!(inside.is_file(), "a failed clean deletes nothing");
    bystanders(&home);
    let log = fs::read_to_string(home.join("log/scratch.jsonl")).unwrap();
    assert!(log.contains("\"sandbox\":\"b\""), "{log}");
    assert!(log.contains("\"code\":\"Failed\""), "{log}");
}

#[test]
fn a_sweep_reaches_a_settled_record_past_more_than_a_batch_of_unrelated_ones() {
    let home = fresh_home();
    // 70 records of another machine sort before the one this pass owns; the
    // old pass took 64 names before it looked at the machine, so it never
    // reached `zzz`
    let mut others = Vec::new();
    for index in 0..70 {
        others.push(gone(&home, &format!("a{index:03}"), "elsewhere", 1));
    }
    let mine = gone(&home, "zzz", "native", 2);
    let report = scratch::sweep(&home, "native", Mode::Apply).unwrap();
    assert_eq!(report.deleted, vec![Owned::new("zzz", mine.clone())]);
    assert!(!mine.exists());
    for root in others {
        assert!(
            root.join("commands-left-this").is_file(),
            "another machine's scratch survives: {}",
            root.display()
        );
    }
}

#[test]
fn a_pass_is_bounded_and_the_next_pass_reaches_the_next_batch() {
    let home = fresh_home();
    for index in 0..67 {
        gone(&home, &format!("s{index:03}"), "native", 1);
    }
    let first = scratch::sweep(&home, "native", Mode::Apply).unwrap();
    assert_eq!(first.deleted.len(), 64, "one pass is bounded");
    // the batch is charged only for roots it removed, so the rest are
    // reached by the next pass
    let second = scratch::sweep(&home, "native", Mode::Apply).unwrap();
    assert_eq!(second.deleted.len(), 3, "the next pass reaches the rest");
    let covered: Vec<PathBuf> = first
        .deleted
        .into_iter()
        .chain(second.deleted)
        .map(|owned| owned.root().to_path_buf())
        .filter(|root| root.exists())
        .collect();
    assert!(
        covered.is_empty(),
        "every settled scratch root is gone: {covered:?}"
    );
}

#[test]
fn a_symlinked_scratch_is_not_followed() {
    let home = fresh_home();
    let target = home.join("elsewhere");
    seed(&target);
    let link = home.join("machine/native/run/lfs-x-b/tmp");
    fs::create_dir_all(link.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let record =
        SandboxRecord::from_record("b", record_value("b", "native", Some("b"), None, &link, 1));
    let kept = scratch::clean(&home, &record, Owner::Close).unwrap();
    assert!(
        matches!(kept, Cleaned::Kept(ref path, Kept::Outside | Kept::NotScratch) if path == &link),
        "{kept:?}"
    );
    assert!(target.join("commands-left-this").is_file());
}

#[test]
fn a_scratch_token_round_trips_through_the_record() {
    let root = PathBuf::from("/home/u/layerfs/machine/native/run/lfs-x-b/tmp");
    let scratch = Scratch::new(root.clone(), 1_762_160_000_000, 11).unwrap();
    assert_eq!(scratch.root(), root);
    assert_eq!(scratch.created_at(), 1_762_160_000_000);
    assert_eq!(scratch.generation(), 11);
    let value = SdkValue::map([(scratch::FIELD, scratch.to_record())]);
    assert_eq!(Scratch::of(&value).unwrap(), Some(scratch.clone()));
    let record = SandboxRecord::from_record("b", value);
    assert_eq!(record.scratch().unwrap(), Some(scratch));
    // a record with no scratch owns none
    assert_eq!(
        SandboxRecord::from_record("b", SdkValue::map([("name", text("b"))]))
            .scratch()
            .unwrap(),
        None
    );
    // and the token is part of the shown record, not hidden state
    let shown = record.to_sdk(layerfs_control::common::outcome::View::Full);
    assert!(shown.get(scratch::FIELD).is_some());
}
