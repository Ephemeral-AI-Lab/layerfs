//! `home.init` and `home.get`: a home is set up on purpose, and read without
//! being changed. `home.get` answers for a home that does not exist, creates
//! nothing and writes no log line; `home.init` says whether it created the
//! home and leaves a current one alone; an older LayerFS's home is refused
//! with the fix that moves it aside, and never written. Both answer in the
//! call surface's forms (2.0): `home.get` its `state` with `reason` and
//! `fix`, the build's `version` and the home's `generation`; `home.init` its
//! `outcome` and the build's `version`.
use layerfs_bridge::control::SdkValue;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A fresh folder under the temporary directory, not yet created.
fn place() -> PathBuf {
    std::env::temp_dir().join(format!(
        "layerfs-home-calls-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ))
}

fn call(home: &Path, name: &str) -> SdkValue {
    layerfs_api::call(Some(home), name, SdkValue::Null, &mut |_| {})
        .unwrap_or_else(|r| panic!("{name}: {}: {}", r.code, r.message))
}

/// `name` in its long form.
fn long(home: &Path, name: &str) -> SdkValue {
    let detail = SdkValue::map([("detail", SdkValue::Bool(true))]);
    layerfs_api::call(Some(home), name, detail, &mut |_| {})
        .unwrap_or_else(|r| panic!("{name}: {}: {}", r.code, r.message))
}

/// `home.get` in its long form.
fn get(home: &Path) -> SdkValue {
    let detail = SdkValue::map([("detail", SdkValue::Bool(true))]);
    layerfs_api::call(Some(home), "home.get", detail, &mut |_| {})
        .unwrap_or_else(|r| panic!("home.get: {}: {}", r.code, r.message))
}

fn keys(value: &SdkValue) -> Vec<&str> {
    match value {
        SdkValue::Map(fields) => fields.iter().map(|(name, _)| name.as_str()).collect(),
        other => panic!("not a map: {other:?}"),
    }
}

fn log_lines(home: &Path) -> usize {
    std::fs::read_to_string(home.join("trace").join("calls.jsonl"))
        .map(|text| text.lines().count())
        .unwrap_or(0)
}

#[test]
fn get_answers_for_a_missing_home_and_creates_nothing() {
    let home = place();
    let got = get(&home);
    assert_eq!(got.get_text("home"), Some(home.to_str().unwrap()));
    assert_eq!(got.get_text("state"), Some("broken"));
    assert_eq!(got.get_text("reason"), Some("missing"));
    assert_eq!(got.get_text("fix"), Some("layerfs home init"));
    assert!(got.get_text("message").unwrap().contains("no LayerFS home"));
    assert_eq!(got.get("layout"), Some(&SdkValue::Null));
    assert_eq!(got.get("mode"), Some(&SdkValue::Null));
    assert_eq!(got.get("generation"), Some(&SdkValue::Null));
    // the short form says the same in fewer keys
    let short = call(&home, "home.get");
    assert_eq!(keys(&short), ["home", "state", "version", "reason", "fix"]);
    assert_eq!(short.get_text("version"), Some(env!("CARGO_PKG_VERSION")));
    assert!(!home.exists(), "home.get created {}", home.display());
}

#[test]
fn init_creates_a_home_then_leaves_it_and_get_reads_it_without_a_log_line() {
    let home = place();
    let first = long(&home, "home.init");
    assert_eq!(
        keys(&first),
        ["home", "unchanged", "outcome", "layout", "version"]
    );
    assert_eq!(first.get_text("outcome"), Some("created"));
    assert_eq!(first.get("unchanged"), Some(&SdkValue::Bool(false)));
    assert_eq!(first.get("layout"), Some(&SdkValue::Int(3)));
    assert_eq!(first.get_text("version"), Some(env!("CARGO_PKG_VERSION")));
    assert!(home.join("layerfs.json").is_file());
    let mode = std::fs::metadata(&home).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);
    // the short form names the home, and says only what is news
    let again = call(&home, "home.init");
    assert_eq!(keys(&again), ["home", "unchanged"]);
    assert_eq!(again.get("unchanged"), Some(&SdkValue::Bool(true)));
    assert_eq!(
        long(&home, "home.init").get_text("outcome"),
        Some("unchanged")
    );
    let before = log_lines(&home);
    let got = get(&home);
    assert_eq!(log_lines(&home), before, "home.get wrote a log line");
    assert_eq!(got.get_text("state"), Some("ready"));
    assert_eq!(got.get("reason"), Some(&SdkValue::Null));
    assert_eq!(got.get("fix"), Some(&SdkValue::Null));
    assert_eq!(got.get("message"), Some(&SdkValue::Null));
    assert_eq!(got.get("layout"), Some(&SdkValue::Int(3)));
    assert_eq!(got.get_text("mode"), Some("0700"));
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn the_mode_is_answered_as_it_is() {
    let home = place();
    call(&home, "home.init");
    let set =
        |mode| std::fs::set_permissions(&home, std::fs::Permissions::from_mode(mode)).unwrap();
    // whether it is owner-only is the home doctor's to say; `home.get`
    // answers the bits and stays ready
    for (mode, text) in [(0o701, "0701"), (0o755, "0755")] {
        set(mode);
        let got = get(&home);
        assert_eq!(got.get_text("mode"), Some(text));
        assert_eq!(got.get_text("state"), Some("ready"));
    }
    set(0o700);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn an_older_home_is_outdated_and_init_refuses_it_with_the_move_fix() {
    let home = place();
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&home)
        .unwrap();
    let older = r#"{"layout":2,"id":"0123456789abcdef0123456789abcdef"}"#;
    std::fs::write(home.join("layerfs.json"), older).unwrap();
    let shown = home.display().to_string();
    let fix = format!("mv {shown} {shown}.old && layerfs home init");
    let got = get(&home);
    assert_eq!(got.get_text("state"), Some("broken"));
    assert_eq!(got.get_text("reason"), Some("outdated"));
    assert_eq!(got.get_text("fix"), Some(fix.as_str()));
    let refused =
        layerfs_api::call(Some(&home), "home.init", SdkValue::Null, &mut |_| {}).unwrap_err();
    assert_eq!(refused.code, "Unsupported", "{}", refused.message);
    assert_eq!(refused.detail.get_text("fix"), Some(fix.as_str()));
    // any other call is refused the same way, and nothing but the call log
    // is written
    let other =
        layerfs_api::call(Some(&home), "machine.list", SdkValue::Null, &mut |_| {}).unwrap_err();
    assert_eq!(other.code, "Unsupported", "{}", other.message);
    let file = std::fs::read_to_string(home.join("layerfs.json")).unwrap();
    assert_eq!(file, older);
    let mut names: Vec<String> = (std::fs::read_dir(&home).unwrap())
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["layerfs.json", "trace"]);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_home_it_cannot_read_is_broken_not_refused() {
    let home = place();
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&home)
        .unwrap();
    std::fs::write(home.join("layerfs.json"), "not json").unwrap();
    let got = get(&home);
    assert_eq!(got.get_text("state"), Some("broken"));
    assert_eq!(got.get_text("reason"), Some("failed"));
    assert_eq!(got.get("fix"), Some(&SdkValue::Null));
    assert!(got.get_text("message").is_some());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn the_generation_moves_when_a_record_is_written_or_removed_and_not_on_a_read() {
    let home = place();
    call(&home, "home.init");
    let generation = |home: &Path| get(home).get_int("generation").unwrap();
    let start = generation(&home);
    // reads move nothing
    call(&home, "store.list");
    call(&home, "home.log");
    assert_eq!(generation(&home), start);
    let named = SdkValue::map([("name", SdkValue::text("shop"))]);
    layerfs_api::call(Some(&home), "store.create", named.clone(), &mut |_| {}).unwrap();
    let created = generation(&home);
    assert!(created > start, "{created} after {start}");
    let removed = SdkValue::map([("store", SdkValue::text("shop"))]);
    layerfs_api::call(Some(&home), "store.remove", removed, &mut |_| {}).unwrap();
    assert!(generation(&home) > created);
    let _ = std::fs::remove_dir_all(&home);
}

/// The machine nodes of `home.get --with tree`, long form.
fn tree_machines(home: &Path) -> Vec<SdkValue> {
    let asked = SdkValue::map([
        ("detail", SdkValue::Bool(true)),
        ("with", SdkValue::List(vec![SdkValue::text("tree")])),
    ]);
    let answer = layerfs_api::call(Some(home), "home.get", asked, &mut |_| {})
        .unwrap_or_else(|r| panic!("home.get: {}: {}", r.code, r.message));
    match answer.get("tree").and_then(|tree| tree.get("machines")) {
        Some(SdkValue::List(machines)) => machines.clone(),
        other => panic!("no machines: {other:?}"),
    }
}

#[test]
fn the_tree_lists_a_machine_only_a_store_names() {
    let home = place();
    call(&home, "home.init");
    let named = SdkValue::map([("name", SdkValue::text("shop"))]);
    layerfs_api::call(Some(&home), "store.create", named, &mut |_| {}).unwrap();
    // `native` exists with or without a record: its Store is under it, ready
    let _ = std::fs::remove_dir_all(home.join("machine"));
    let machines = tree_machines(&home);
    assert_eq!(machines.len(), 1, "{machines:?}");
    let native = &machines[0];
    assert_eq!(native.get_text("machine"), Some("native"));
    assert_eq!(native.get_text("platform"), Some("native"));
    assert_eq!(native.get_text("state"), Some("ready"));
    let Some(SdkValue::List(stores)) = native.get("stores") else {
        panic!("{native:?}")
    };
    assert_eq!(stores[0].get_text("store"), Some("shop"));
    assert_eq!(stores[0].get_text("state"), Some("ready"));
    // a Store whose record names a machine that has none is still listed,
    // under a machine that says it is missing
    let record = home.join("store").join("shop").join("record.json");
    let text = std::fs::read_to_string(&record).unwrap();
    assert!(text.contains(r#""machine":"native""#), "{text}");
    let moved = text.replace(r#""machine":"native""#, r#""machine":"gone""#);
    std::fs::write(&record, moved).unwrap();
    let machines = tree_machines(&home);
    assert_eq!(machines.len(), 1, "{machines:?}");
    let gone = &machines[0];
    assert_eq!(gone.get_text("machine"), Some("gone"));
    assert_eq!(gone.get("platform"), Some(&SdkValue::Null));
    assert_eq!(gone.get_text("state"), Some("broken"));
    assert_eq!(gone.get_text("reason"), Some("missing"));
    let Some(SdkValue::List(stores)) = gone.get("stores") else {
        panic!("{gone:?}")
    };
    assert_eq!(stores[0].get_text("store"), Some("shop"));
    assert_eq!(stores[0].get_text("state"), Some("blocked"));
    let by = stores[0].get("blocked_by").unwrap();
    assert_eq!(by.get_text("machine"), Some("gone"));
    assert_eq!(by.get_text("reason"), Some("missing"));
    let _ = std::fs::remove_dir_all(&home);
}
