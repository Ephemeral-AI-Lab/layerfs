//! `home.move` (`relocate`): a legacy home at the LayerFS folder moves into
//! its `home/`, client folders stay, the in-home paths records hold follow
//! it, and anything that runs in the home refuses it before any effect.
use layerfs_home::disk::{detect, ensure, Layout};
use layerfs_home::names::HOST_MACHINE;
use layerfs_inspect::home::relocate::relocate;
use layerfs_protocol::sdk::SdkValue;
use std::path::{Path, PathBuf};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-move-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(dir).unwrap()
}

/// Every path under `dir` with its bytes, sorted.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
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

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn texts(value: &SdkValue, key: &str) -> Vec<String> {
    match value.get(key) {
        Some(SdkValue::List(items)) => items
            .iter()
            .map(|item| match item {
                SdkValue::Text(text) => text.clone(),
                other => panic!("{key}: {other:?}"),
            })
            .collect(),
        other => panic!("{key}: {other:?}"),
    }
}

/// A legacy home at `folder` with a host daemon in its `runtime/`, a gone
/// host Sandbox and its run folder, and a client's `dsh/` beside them.
fn legacy(folder: &Path) -> String {
    let id = ensure(folder).unwrap().id;
    let f = folder.display();
    write(&folder.join("runtime/v1/bin/layerfs-daemon"), "daemon");
    write(&folder.join("dsh/dsh.json"), "{}");
    write(
        &folder.join(format!("machine/{HOST_MACHINE}/record.json")),
        &format!(
            r#"{{"v":1,"name":"{HOST_MACHINE}","platform":"host","daemon":"{f}/runtime/v1/bin/layerfs-daemon","image_archive":"{f}/dsh/image.tar"}}"#
        ),
    );
    write(
        &folder.join("sandbox/gone/record.json"),
        &format!(
            r#"{{"v":1,"name":"gone","machine":"{HOST_MACHINE}","id":null,"daemon":{{"directory":"{f}/machine/{HOST_MACHINE}/run/s1","labels":[{{"key":"k","value":"{f}/runtime/v1"}}]}}}}"#
        ),
    );
    write(
        &folder.join(format!("machine/{HOST_MACHINE}/run/s1/owner")),
        &format!("{f}/sandbox/gone"),
    );
    id
}

#[test]
fn a_legacy_home_moves_into_home_and_its_paths_follow() {
    let folder = tempdir("legacy");
    let id = legacy(&folder);
    let dsh = snapshot(&folder.join("dsh"));
    let home = folder.join("home");
    let answer = relocate(&folder, &home).unwrap();
    assert_eq!(
        answer.get_text("home"),
        Some(home.display().to_string().as_str())
    );
    assert_eq!(answer.get_text("outcome"), Some("moved"));
    assert_eq!(texts(&answer, "left"), ["dsh"]);
    let moved = texts(&answer, "moved");
    for name in ["layerfs.json", "machine", "sandbox", "runtime"] {
        assert!(moved.iter().any(|m| m == name), "{name} in {moved:?}");
    }
    assert_eq!(
        texts(&answer, "rewritten"),
        [
            format!("machine/{HOST_MACHINE}/record.json"),
            "sandbox/gone/record.json".to_owned(),
            format!("machine/{HOST_MACHINE}/run/s1/owner"),
        ]
    );
    // the folder is now a container; the moved home keeps its id
    assert_eq!(detect(&folder).unwrap(), Layout::Container);
    assert!(matches!(detect(&home).unwrap(), Layout::Current(file) if file.id == id));
    assert_eq!(
        snapshot(&folder.join("dsh")),
        dsh,
        "the client folder changed"
    );
    let (h, f) = (home.display(), folder.display());
    let machine = std::fs::read_to_string(home.join(format!("machine/{HOST_MACHINE}/record.json")));
    let machine = machine.unwrap();
    assert!(
        machine.contains(&format!(r#""daemon":"{h}/runtime/v1/bin/layerfs-daemon""#)),
        "{machine}"
    );
    // a path into a client's folder, which stayed, is left as it was
    assert!(
        machine.contains(&format!(r#""image_archive":"{f}/dsh/image.tar""#)),
        "{machine}"
    );
    let sandbox = std::fs::read_to_string(home.join("sandbox/gone/record.json")).unwrap();
    assert!(sandbox.contains(&format!(
        r#""directory":"{h}/machine/{HOST_MACHINE}/run/s1""#
    )));
    assert!(
        sandbox.contains(&format!(r#""value":"{h}/runtime/v1""#)),
        "{sandbox}"
    );
    let owner = home.join(format!("machine/{HOST_MACHINE}/run/s1/owner"));
    assert_eq!(
        std::fs::read_to_string(&owner).unwrap(),
        format!("{h}/sandbox/gone")
    );
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(&owner).unwrap().permissions().mode() & 0o777,
        0o600
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn anything_running_or_a_bad_target_refuses_the_move_before_any_effect() {
    let folder = tempdir("refused");
    legacy(&folder);
    write(&folder.join("home/kept.txt"), "kept");
    let before = snapshot(&folder);
    let refusal = |to: &Path| relocate(&folder, to).unwrap_err();
    assert_eq!(refusal(&folder.join("home")).code, "Exists");
    assert_eq!(refusal(Path::new("relative/home")).code, "InvalidInput");
    assert_eq!(refusal(&folder.join("home/../other")).code, "InvalidInput");
    assert_eq!(refusal(&folder.join("machine/inside")).code, "InvalidInput");
    std::fs::remove_dir_all(folder.join("home")).unwrap();
    // an open host-machine Sandbox
    write(
        &folder.join("sandbox/live/record.json"),
        &format!(r#"{{"v":1,"name":"live","machine":"{HOST_MACHINE}","id":"abc"}}"#),
    );
    let before_live = snapshot(&folder);
    let open = refusal(&folder.join("home"));
    assert_eq!(open.code, "InUse");
    assert_eq!(
        open.detail.get_text("fix"),
        Some("layerfs sandbox remove live")
    );
    assert_eq!(snapshot(&folder), before_live, "a refused move wrote");
    // a Sandbox on another machine names no path in the home
    write(
        &folder.join("sandbox/live/record.json"),
        r#"{"v":1,"name":"live","machine":"docker","id":"abc"}"#,
    );
    // a live Store lease: this test's parent process (its own is skipped)
    let parent = std::os::unix::process::parent_id();
    write(
        &folder.join(format!("store/s/open/{parent}")),
        &parent.to_string(),
    );
    assert_eq!(refusal(&folder.join("home")).code, "InUse");
    std::fs::remove_dir_all(folder.join("store")).unwrap();
    assert!(before.len() > 1);
    // not a home: nothing there, or a LayerFS folder
    let empty = tempdir("empty");
    assert_eq!(
        relocate(&empty, &empty.join("home")).unwrap_err().code,
        "NotFound"
    );
    relocate(&folder, &folder.join("home")).unwrap();
    assert_eq!(
        relocate(&folder, &folder.join("again")).unwrap_err().code,
        "InvalidInput"
    );
    std::fs::remove_dir_all(folder).unwrap();
    std::fs::remove_dir_all(empty).unwrap();
}
