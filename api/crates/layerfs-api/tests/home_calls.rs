//! `home.init` and `home.get`: a home is set up on purpose, and read without
//! being changed. `home.get` answers for a home that does not exist, creates
//! nothing and writes no log line; `home.init` says whether it created or
//! converted the home and leaves a current one alone.
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

fn codes(value: &SdkValue) -> Vec<String> {
    match value.get("warnings") {
        Some(SdkValue::List(list)) => list
            .iter()
            .filter_map(|w| w.get_text("code").map(str::to_owned))
            .collect(),
        _ => panic!("no warnings list"),
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
    let got = call(&home, "home.get");
    let state = got.get("home").unwrap();
    assert_eq!(state.get_text("state"), Some("missing"));
    assert_eq!(state.get("layout"), Some(&SdkValue::Null));
    assert_eq!(state.get("mode"), Some(&SdkValue::Null));
    assert!(codes(&got).contains(&"HomeMissing".to_owned()));
    assert!(!home.exists(), "home.get created {}", home.display());
}

#[test]
fn init_creates_a_home_then_leaves_it_and_get_reads_it_without_a_log_line() {
    let home = place();
    let first = call(&home, "home.init");
    assert_eq!(first.get("created"), Some(&SdkValue::Bool(true)));
    assert_eq!(first.get("converted"), Some(&SdkValue::Bool(false)));
    assert_eq!(first.get("layout"), Some(&SdkValue::Int(3)));
    assert!(home.join("layerfs.json").is_file());
    let mode = std::fs::metadata(&home).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);
    let again = call(&home, "home.init");
    assert_eq!(again.get("created"), Some(&SdkValue::Bool(false)));
    assert_eq!(again.get("converted"), Some(&SdkValue::Bool(false)));
    let before = log_lines(&home);
    let got = call(&home, "home.get");
    assert_eq!(log_lines(&home), before, "home.get wrote a log line");
    let state = got.get("home").unwrap();
    assert_eq!(state.get_text("state"), Some("current"));
    assert_eq!(state.get("layout"), Some(&SdkValue::Int(3)));
    assert_eq!(state.get_text("mode"), Some("0700"));
    assert_eq!(state.get("mode_ok"), Some(&SdkValue::Bool(true)));
    assert!(!codes(&got).iter().any(|c| c.starts_with("Home")));
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn others_may_pass_through_the_home_but_not_read_it() {
    let home = place();
    call(&home, "home.init");
    let set =
        |mode| std::fs::set_permissions(&home, std::fs::Permissions::from_mode(mode)).unwrap();
    set(0o701);
    let got = call(&home, "home.get");
    assert_eq!(
        got.get("home").unwrap().get("mode_ok"),
        Some(&SdkValue::Bool(true))
    );
    assert!(!codes(&got).contains(&"HomeMode".to_owned()));
    set(0o755);
    let got = call(&home, "home.get");
    assert_eq!(got.get("home").unwrap().get_text("mode"), Some("0755"));
    assert_eq!(
        got.get("home").unwrap().get("mode_ok"),
        Some(&SdkValue::Bool(false))
    );
    assert!(codes(&got).contains(&"HomeMode".to_owned()));
    set(0o700);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_layout_2_home_needs_converting_until_init_converts_it() {
    let home = place();
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&home)
        .unwrap();
    std::fs::write(
        home.join("layerfs.json"),
        r#"{"layout":2,"id":"0123456789abcdef0123456789abcdef"}"#,
    )
    .unwrap();
    let got = call(&home, "home.get");
    let state = got.get("home").unwrap();
    assert_eq!(state.get_text("state"), Some("needs_conversion"));
    assert_eq!(state.get("layout"), Some(&SdkValue::Int(2)));
    assert!(codes(&got).contains(&"HomeNeedsConversion".to_owned()));
    // reading it converted nothing
    let file = std::fs::read_to_string(home.join("layerfs.json")).unwrap();
    assert!(file.contains("\"layout\":2"), "{file}");
    let init = call(&home, "home.init");
    assert_eq!(init.get("created"), Some(&SdkValue::Bool(false)));
    assert_eq!(init.get("converted"), Some(&SdkValue::Bool(true)));
    assert_eq!(init.get("layout"), Some(&SdkValue::Int(3)));
    let got = call(&home, "home.get");
    assert_eq!(got.get("home").unwrap().get_text("state"), Some("current"));
    let _ = std::fs::remove_dir_all(&home);
}
