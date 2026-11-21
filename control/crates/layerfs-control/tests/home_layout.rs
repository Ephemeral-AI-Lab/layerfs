//! `layerfs.json` and the layout version (#533 step 3): an empty home, the
//! current layout, a foreign or unparsable `layerfs.json` (MIG, RC-8) and one
//! the system refuses to read, each in its own test.
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{detect, ensure, open, Layout, LAYOUT_VERSION};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-home-{tag}-{}-{}-{}",
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

#[test]
fn a_missing_or_empty_home_is_empty_and_gets_a_layout_and_id() {
    let root = tempdir("empty");
    let home = root.join("home");
    assert_eq!(detect(&home).unwrap(), Layout::Empty);
    assert!(!home.exists(), "detect writes nothing");
    let file = ensure(&home).unwrap();
    assert_eq!(file.layout, LAYOUT_VERSION);
    assert_eq!(file.id.len(), 32);
    let meta = std::fs::metadata(home.join("layerfs.json")).unwrap();
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    assert_eq!(
        std::fs::metadata(&home).unwrap().permissions().mode() & 0o777,
        0o700
    );
    // a second ensure reads the same id
    assert_eq!(ensure(&home).unwrap(), file);
    assert_eq!(detect(&home).unwrap(), Layout::Current(file));
    // a home of this layout keeps the call log
    layerfs_home::logs::append(&home, &line());
    assert!(layerfs_home::home::paths::calls_file(&home).is_file());
    std::fs::remove_dir_all(root).unwrap();
}

/// One call-log line.
fn line() -> layerfs_bridge::control::SdkValue {
    use layerfs_bridge::control::SdkValue;
    SdkValue::map([("call", SdkValue::text("home.get"))])
}

/// The fix a refusal of a foreign folder carries.
fn fix(refusal: &layerfs_bridge::control::SdkRefusal) -> Option<&str> {
    refusal.detail.get_text("fix")
}

/// Every entry under `dir` with its bytes (folders as `None`), sorted.
fn snapshot(dir: &std::path::Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    let mut all = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            all.push((path.clone(), None));
            all.extend(snapshot(&path));
        } else {
            all.push((path.clone(), Some(std::fs::read(&path).unwrap())));
        }
    }
    all.sort();
    all
}

#[test]
fn what_making_a_home_writes_first_still_reads_as_empty() {
    // the home lock `open` takes before `layerfs.json`, and the temporary
    // `layerfs.json` is renamed from (another process mid-write, or a killed one)
    let home = tempdir("own");
    std::fs::write(home.join("layerfs.lock"), "").unwrap();
    assert_eq!(detect(&home).unwrap(), Layout::Empty);
    std::fs::write(home.join(".layerfs.json.7.0.tmp"), "{").unwrap();
    assert_eq!(detect(&home).unwrap(), Layout::Empty);
    let file = open(&home).unwrap();
    assert_eq!(detect(&home).unwrap(), Layout::Current(file));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn integration_folders_and_ds_store_read_as_empty_and_init_leaves_them() {
    // an integration's reserved folders, made before the first call (as
    // `@layerfs/runtime` makes `runtime/`), and macOS's `.DS_Store`
    let home = tempdir("integration");
    std::fs::write(home.join("layerfs.lock"), "").unwrap();
    for folder in ["runtime", "job"] {
        std::fs::create_dir(home.join(folder)).unwrap();
        std::fs::write(home.join(folder).join("kept"), folder).unwrap();
    }
    std::fs::write(home.join(".DS_Store"), "finder").unwrap();
    let before = snapshot(&home);
    assert_eq!(detect(&home).unwrap(), Layout::Empty);
    let created = layerfs_control::home::init(&home, false).unwrap();
    assert_eq!(created.get_text("outcome"), Some("created"));
    assert!(matches!(detect(&home).unwrap(), Layout::Current(_)));
    let after = snapshot(&home);
    for entry in &before {
        assert!(after.contains(entry), "{:?} changed", entry.0);
    }
    // a reserved name that is a file, not a folder, is not the integration's
    let other = tempdir("integration-file");
    std::fs::write(other.join("runtime"), "a file").unwrap();
    assert_eq!(detect(&other).unwrap(), Layout::Foreign);
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(other).unwrap();
}

#[test]
fn a_folder_with_a_stray_file_and_no_layerfs_json_is_foreign_and_left_as_it_was() {
    let home = tempdir("stray");
    std::fs::write(home.join("notes.txt"), "kept").unwrap();
    std::fs::write(home.join("layerfs.lock"), "").unwrap();
    std::fs::create_dir(home.join("log")).unwrap();
    let before = snapshot(&home);
    assert_eq!(detect(&home).unwrap(), Layout::Foreign);
    for refused in [ensure(&home).unwrap_err(), open(&home).unwrap_err()] {
        assert_eq!(refused.code, "Unsupported");
        assert!(
            refused.message.contains("not a LayerFS 2.0 home"),
            "{}",
            refused.message
        );
        let moved = format!("{}.old", home.display());
        assert_eq!(
            fix(&refused),
            Some(format!("mv {} {moved} && layerfs home init", home.display()).as_str())
        );
    }
    // a call refused there writes no call log into it
    layerfs_home::logs::append(&home, &line());
    assert_eq!(before, snapshot(&home), "the folder was written");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_layerfs_json_of_another_layout_or_unparsable_is_foreign() {
    let home = tempdir("other");
    let id = "b".repeat(32);
    // any layout but this build's, older or newer: refused with the fix,
    // never converted or written
    for layout in [1, 2, LAYOUT_VERSION + 1] {
        let text = format!("{{\"layout\":{layout},\"id\":\"{id}\"}}");
        std::fs::write(home.join("layerfs.json"), &text).unwrap();
        let before = snapshot(&home);
        assert_eq!(detect(&home).unwrap(), Layout::Foreign, "layout {layout}");
        layerfs_home::logs::append(&home, &line());
        let refused = open(&home).unwrap_err();
        assert_eq!(refused.code, "Unsupported");
        assert!(fix(&refused).is_some_and(|fix| fix.ends_with("&& layerfs home init")));
        assert_eq!(before, snapshot(&home), "layout {layout} was written");
    }
    // one that cannot be parsed (not JSON, no layout number, this layout
    // with a bad id) is not a 2.0 home's either: the same refusal (MIG, RC-8)
    for broken in [
        "{not json".to_owned(),
        "{}".to_owned(),
        "not json".to_owned(),
        format!("{{\"id\":\"{id}\"}}"),
        "{\"layout\":3,\"id\":\"xyz\"}".to_owned(),
        "{\"layout\":3}".to_owned(),
    ] {
        std::fs::write(home.join("layerfs.json"), &broken).unwrap();
        let before = snapshot(&home);
        assert_eq!(detect(&home).unwrap(), Layout::Foreign, "{broken}");
        layerfs_home::logs::append(&home, &line());
        let refused = open(&home).unwrap_err();
        assert_eq!(refused.code, "Unsupported", "{broken}");
        assert_eq!(
            fix(&refused),
            Some(layerfs_home::home::layout::foreign_fix(&home).as_str())
        );
        assert_eq!(before, snapshot(&home), "{broken} was written");
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_layerfs_json_the_system_refuses_to_read_is_refused_unreadable() {
    let home = tempdir("unreadable");
    // a folder in its place: the read is refused with EISDIR, for root too
    std::fs::create_dir(home.join("layerfs.json")).unwrap();
    let refused = detect(&home).unwrap_err();
    assert_eq!(refused.code, "Failed");
    assert!(refused.detail.get_bool("unreadable"), "{refused:?}");
    assert!(
        refused.message.ends_with("Is a directory (os error 21)"),
        "{}",
        refused.message
    );
    assert_eq!(fix(&refused), None);
    std::fs::remove_dir_all(home).unwrap();
}

/// Makes `dir` look like a machine's worker root: its data (`store/`,
/// `sandbox/`, `machine/native/run/`) and what its worker keeps (a
/// `project.init` copy and its `log/worker.log`).
fn worker_shape(dir: &std::path::Path) {
    for folder in [
        "store/shop",
        "sandbox/live",
        "machine/native/run",
        "log",
        "incoming",
    ] {
        std::fs::create_dir_all(dir.join(folder)).unwrap();
    }
    std::fs::write(dir.join("log/worker.log"), "worker started\n").unwrap();
    std::fs::create_dir_all(dir.join("incoming/init-0123456789abcdef")).unwrap();
}

#[test]
fn a_workers_data_disk_is_a_worker_root_not_a_home() {
    // the incident's shape: a data disk with no `layerfs.json`, mistaken
    // for a new home because nothing was written there yet
    let home = tempdir("worker-root");
    worker_shape(&home);
    assert_eq!(detect(&home).unwrap(), Layout::WorkerRoot);
    let before = snapshot(&home);
    for refused in [ensure(&home).unwrap_err(), open(&home).unwrap_err()] {
        assert_eq!(refused.code, "Unsupported");
        assert!(
            refused.message.contains("worker root"),
            "{}",
            refused.message
        );
        let fix = fix(&refused).unwrap_or_default();
        assert!(!fix.contains("mv "), "{fix}");
        assert!(!fix.contains("home init"), "{fix}");
    }
    // refused before the home lock: no `layerfs.json`, no `layerfs.lock`
    assert_eq!(before, snapshot(&home), "the worker root was written");
    assert!(!home.join("layerfs.json").exists());
    assert!(!home.join("layerfs.lock").exists());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn one_half_of_a_worker_root_is_not_a_worker_root() {
    // one half only: not a worker's root. A home's own data with no
    // marker is adoptable (c6); the worker's output or an init copy alone
    // is a home that lost its marker too, never a worker's root
    for data in ["store/shop", "sandbox/live", "machine/native/run"] {
        let home = tempdir("worker-data-only");
        std::fs::create_dir_all(home.join(data)).unwrap();
        assert_eq!(detect(&home).unwrap(), Layout::DataWithoutMarker, "{data}");
        std::fs::remove_dir_all(home).unwrap();
    }
    let home = tempdir("worker-log-only");
    std::fs::create_dir_all(home.join("log")).unwrap();
    std::fs::write(home.join("log/worker.log"), "worker started\n").unwrap();
    assert_eq!(detect(&home).unwrap(), Layout::DataWithoutMarker);
    std::fs::remove_dir_all(home).unwrap();
    let home = tempdir("worker-incoming-only");
    std::fs::create_dir_all(home.join("incoming/init-0123456789abcdef")).unwrap();
    assert_eq!(detect(&home).unwrap(), Layout::DataWithoutMarker);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn home_entries_keep_a_worker_shaped_folder_from_being_a_worker_root() {
    // a home keeps these; a worker's root never does
    for entry in ["trace", "backup"] {
        let home = tempdir("worker-home-entry");
        worker_shape(&home);
        std::fs::create_dir_all(home.join(entry)).unwrap();
        assert_ne!(detect(&home).unwrap(), Layout::WorkerRoot, "{entry}");
        std::fs::remove_dir_all(home).unwrap();
    }
}

#[test]
fn a_worker_shaped_folder_with_layerfs_json_is_current() {
    // a real home may hold a Store's file and a worker-like log; its marker
    // decides, and the shape check never sees it
    let home = tempdir("worker-marker");
    worker_shape(&home);
    let file = layerfs_home::home::layout::HomeFile {
        layout: LAYOUT_VERSION,
        id: "c".repeat(32),
    };
    layerfs_home::home::layout::write(&home, &file).unwrap();
    assert_eq!(detect(&home).unwrap(), Layout::Current(file));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_home_missing_its_marker_is_not_a_worker_root() {
    // a real home's data, log and backup with no `layerfs.json`. c6 gives
    // this its own variant; under either build it is not a worker's root,
    // which also needs an `init-*` copy or `log/worker.log`
    let home = tempdir("markerless-home");
    for folder in ["store/shop/project/demo", "trace", "backup/bucket", "log"] {
        std::fs::create_dir_all(home.join(folder)).unwrap();
    }
    std::fs::write(home.join("store/shop/record.json"), "{}").unwrap();
    std::fs::write(home.join("trace/calls.jsonl"), "").unwrap();
    std::fs::write(home.join("backup/bucket/record.json"), "{}").unwrap();
    std::fs::write(home.join("log/leases.jsonl"), "").unwrap();
    let found = detect(&home).unwrap();
    assert_eq!(found, Layout::DataWithoutMarker);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn home_get_answers_a_worker_root_broken_unsupported_without_moving_it() {
    use layerfs_control::home::{get, Build, With};
    let home = tempdir("worker-get");
    worker_shape(&home);
    let before = snapshot(&home);
    let answer = get(&home, Build::default(), With::default()).unwrap();
    assert_eq!(answer.get_text("state"), Some("broken"));
    assert_eq!(answer.get_text("reason"), Some("unsupported"));
    assert_eq!(answer.get_text("layout"), None);
    let fix = answer.get_text("fix").unwrap_or_default();
    assert!(!fix.contains("mv "), "{fix}");
    assert!(!fix.contains("home init"), "{fix}");
    assert!(
        answer
            .get_text("message")
            .is_some_and(|message| message.contains("worker root")),
        "{answer:?}"
    );
    assert_eq!(before, snapshot(&home), "home get wrote in the worker root");
    std::fs::remove_dir_all(home).unwrap();
}

/// A real home's data with no `layerfs.json`: a Store with a project, a
/// machine record whose `vm_name` names the home, a Sandbox record whose
/// daemon names it too, a trace file, a backup bucket and a log.
fn data_home(tag: &str) -> PathBuf {
    let home = tempdir(tag);
    for folder in [
        "store/shop/project/demo",
        "machine/docker",
        "machine/native",
        "sandbox/live",
        "backup/bucket",
        "trace",
        "log",
    ] {
        std::fs::create_dir_all(home.join(folder)).unwrap();
    }
    std::fs::write(
        home.join("store/shop/record.json"),
        r#"{"name":"shop","kind":"sqlite","machine":"native"}"#,
    )
    .unwrap();
    std::fs::write(
        home.join("store/shop/project/demo/record.json"),
        r#"{"name":"demo"}"#,
    )
    .unwrap();
    std::fs::write(
        home.join("machine/docker/record.json"),
        r#"{"name":"docker","vm_name":"layerfs-0123456789abcdef-docker"}"#,
    )
    .unwrap();
    // `native` keeps no provider name
    std::fs::write(
        home.join("machine/native/record.json"),
        r#"{"name":"native"}"#,
    )
    .unwrap();
    std::fs::write(
        home.join("sandbox/live/record.json"),
        r#"{"name":"live","daemon":{"name":"layerfs-0123456789abcdef-live"}}"#,
    )
    .unwrap();
    std::fs::write(
        home.join("backup/bucket/record.json"),
        r#"{"bucket":"bucket"}"#,
    )
    .unwrap();
    std::fs::write(home.join("trace/calls.jsonl"), "").unwrap();
    std::fs::write(home.join("log/leases.jsonl"), "").unwrap();
    home
}

#[test]
fn a_home_that_lost_its_marker_is_data_without_marker() {
    let home = data_home("markerless-data");
    assert_eq!(detect(&home).unwrap(), Layout::DataWithoutMarker);
    assert_eq!(
        layerfs_control::home::recorded_prefixes(&home).unwrap(),
        ["0123456789abcdef".to_owned()]
    );
    let before = snapshot(&home);
    // every call that opens the home is refused with the adopt fix, before
    // the home lock: nothing is written, not even the call log
    for refused in [ensure(&home).unwrap_err(), open(&home).unwrap_err()] {
        assert_eq!(refused.code, "Unsupported");
        assert_eq!(fix(&refused), Some("layerfs home init --adopt"));
        assert!(
            refused.message.contains("no layerfs.json"),
            "{}",
            refused.message
        );
        assert!(!refused.message.contains(".old"), "{}", refused.message);
    }
    assert_eq!(before, snapshot(&home), "the data home was written");
    // `home get` says broken missing with the adopt fix, reading only
    let answer = layerfs_control::home::get(
        &home,
        layerfs_control::home::Build::default(),
        layerfs_control::home::With::default(),
    )
    .unwrap();
    assert_eq!(answer.get_text("state"), Some("broken"));
    assert_eq!(answer.get_text("reason"), Some("missing"));
    assert_eq!(answer.get_text("fix"), Some("layerfs home init --adopt"));
    assert!(
        answer
            .get_text("message")
            .is_some_and(|message| message.contains("no layerfs.json")),
        "{answer:?}"
    );
    assert!(!answer.get_text("fix").unwrap_or_default().contains("mv "));
    assert_eq!(before, snapshot(&home), "home get wrote in the data home");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn init_without_adopt_refuses_a_markerless_home_and_writes_nothing() {
    let home = data_home("no-adopt");
    let before = snapshot(&home);
    let refused = layerfs_control::home::init(&home, false).unwrap_err();
    assert_eq!(refused.code, "Unsupported");
    assert_eq!(fix(&refused), Some("layerfs home init --adopt"));
    assert_eq!(before, snapshot(&home), "the data home was written");
    assert!(!home.join("layerfs.json").exists());
    assert!(!home.join("layerfs.lock").exists());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn adopt_writes_the_marker_and_keeps_every_data_byte() {
    let home = data_home("adopt");
    let before = snapshot(&home);
    let answer = layerfs_control::home::init(&home, true).unwrap();
    assert_eq!(answer.get_text("outcome"), Some("adopted"));
    assert_eq!(answer.get("unchanged"), Some(&SdkValue::Bool(false)));
    let id = answer.get_text("id").expect("an id");
    assert_eq!(id.len(), 32, "{id}");
    assert!(id.starts_with("0123456789abcdef"), "{id}");
    assert_eq!(answer.get_text("id_source"), Some("records"));
    match answer.get("warnings") {
        Some(SdkValue::List(items)) => assert!(!items.is_empty(), "the audit warns"),
        other => panic!("no warnings: {other:?}"),
    }
    assert!(matches!(detect(&home).unwrap(), Layout::Current(_)));
    assert!(
        !std::path::Path::new(&format!("{}.old", home.display())).exists(),
        "adopt moved the data"
    );
    // the data is byte for byte what it was; only the marker and the lock
    // the adopt took are new
    let after = snapshot(&home);
    for entry in &before {
        assert!(after.contains(entry), "{:?} changed", entry.0);
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn adopt_reuses_an_eight_hex_prefix_from_records_of_the_earlier_era() {
    let home = data_home("adopt-8");
    std::fs::write(
        home.join("machine/docker/record.json"),
        r#"{"name":"docker","vm_name":"layerfs-01234567-docker"}"#,
    )
    .unwrap();
    std::fs::write(
        home.join("sandbox/live/record.json"),
        r#"{"name":"live","daemon":{"name":"lfs-01234567-live"}}"#,
    )
    .unwrap();
    assert_eq!(
        layerfs_control::home::recorded_prefixes(&home).unwrap(),
        ["01234567".to_owned()]
    );
    let answer = layerfs_control::home::init(&home, true).unwrap();
    assert_eq!(answer.get_text("outcome"), Some("adopted"));
    let id = answer.get_text("id").unwrap();
    assert_eq!(id.len(), 32);
    assert!(id.starts_with("01234567"), "{id}");
    assert_eq!(answer.get_text("id_source"), Some("records"));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn adopt_makes_a_fresh_id_when_no_record_names_the_home() {
    let home = tempdir("adopt-fresh");
    std::fs::create_dir_all(home.join("store/shop")).unwrap();
    let answer = layerfs_control::home::init(&home, true).unwrap();
    assert_eq!(answer.get_text("outcome"), Some("adopted"));
    let id = answer.get_text("id").unwrap();
    assert_eq!(id.len(), 32);
    assert_eq!(answer.get_text("id_source"), Some("fresh"));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn adopt_refuses_records_that_disagree_on_the_home_id() {
    let home = data_home("adopt-conflict");
    std::fs::write(
        home.join("sandbox/live/record.json"),
        r#"{"name":"live","daemon":{"name":"layerfs-fedcba9876543210-live"}}"#,
    )
    .unwrap();
    let before = snapshot(&home);
    let refused = layerfs_control::home::init(&home, true).unwrap_err();
    assert_eq!(refused.code, "Failed");
    assert!(refused.message.contains("disagree"), "{}", refused.message);
    assert!(!home.join("layerfs.json").exists());
    let after = snapshot(&home);
    for entry in &before {
        assert!(after.contains(entry), "{:?} changed", entry.0);
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn adopt_refuses_a_record_it_cannot_decode() {
    let home = data_home("adopt-unreadable");
    std::fs::write(home.join("machine/docker/record.json"), "{not json").unwrap();
    let refused = layerfs_control::home::init(&home, true).unwrap_err();
    assert_eq!(refused.code, "Failed");
    assert!(!home.join("layerfs.json").exists());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn adopt_leaves_a_current_home_as_it_was() {
    let home = tempdir("current-adopt");
    let file = open(&home).unwrap();
    let before = snapshot(&home);
    let answer = layerfs_control::home::init(&home, true).unwrap();
    assert_eq!(answer.get_text("outcome"), Some("unchanged"));
    assert_eq!(answer.get("id"), None);
    assert_eq!(detect(&home).unwrap(), Layout::Current(file));
    assert_eq!(before, snapshot(&home), "adopt wrote in a current home");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_worker_root_is_never_adoptable() {
    let home = tempdir("worker-adopt");
    worker_shape(&home);
    let before = snapshot(&home);
    let refused = layerfs_control::home::init(&home, true).unwrap_err();
    assert_eq!(refused.code, "Unsupported");
    assert!(
        refused.message.contains("worker root"),
        "{}",
        refused.message
    );
    assert_eq!(fix(&refused), Some("unset LAYERFS_HOME"));
    assert_eq!(before, snapshot(&home), "the worker root was written");
    assert!(!home.join("layerfs.json").exists());
    std::fs::remove_dir_all(home).unwrap();
}
