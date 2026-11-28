//! The LayerFS root: its home in `home/`, a root that holds `layerfs.json`
//! being its own home (a legacy `~/.layerfs`), and a root used as a home,
//! which is never a home to move aside.
use layerfs_home::disk::layout::container_fix;
use layerfs_home::disk::root::{home, home_in};
use layerfs_home::disk::{detect, ensure, open, Layout};
use std::path::{Path, PathBuf};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-folder-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Every path under `dir`, sorted.
fn listing(dir: &Path) -> Vec<PathBuf> {
    let mut all = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            all.extend(listing(&path));
        }
        all.push(path);
    }
    all.sort();
    all
}

#[test]
fn a_root_holds_its_home_in_home() {
    let root = tempdir("default");
    assert_eq!(home_in(&root), root.join("home"));
    assert_eq!(home(Some(&root)).unwrap(), root.join("home"));
    // clients' folders, or a home already made there, change nothing
    std::fs::create_dir_all(root.join("dsh")).unwrap();
    assert_eq!(home_in(&root), root.join("home"));
    ensure(&root.join("home")).unwrap();
    assert_eq!(home_in(&root), root.join("home"));
    // a root is absolute
    let refused = home(Some(Path::new("relative/root"))).unwrap_err();
    assert_eq!(refused.code, "InvalidInput");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_root_that_holds_layerfs_json_is_its_own_home() {
    let root = tempdir("legacy");
    ensure(&root).unwrap();
    assert_eq!(home_in(&root), root);
    // a `layerfs.json` this build cannot read still names it, so its own
    // refusal is what the caller sees, not a new empty home beside it
    std::fs::write(root.join("layerfs.json"), "not json").unwrap();
    assert_eq!(home_in(&root), root);
    std::fs::remove_file(root.join("layerfs.json")).unwrap();
    assert_eq!(home_in(&root), root.join("home"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_folder_of_home_and_client_folders_is_a_container_never_moved_aside() {
    let folder = tempdir("container");
    std::fs::create_dir_all(folder.join("dsh/sessions")).unwrap();
    std::fs::write(folder.join("dsh/dsh.json"), "{}").unwrap();
    std::fs::create_dir(folder.join("connect")).unwrap();
    std::fs::write(folder.join(".DS_Store"), "").unwrap();
    // clients only, before the home is made
    assert_eq!(detect(&folder).unwrap(), Layout::Container);
    ensure(&folder.join("home")).unwrap();
    assert_eq!(detect(&folder).unwrap(), Layout::Container);
    let before = listing(&folder);
    for refused in [ensure(&folder).unwrap_err(), open(&folder).unwrap_err()] {
        assert_eq!(refused.code, "InvalidInput");
        assert!(
            refused.message.contains("LayerFS root, not a home"),
            "{}",
            refused.message
        );
        let fix = refused.detail.get_text("fix").unwrap();
        assert_eq!(fix, container_fix(&folder));
        assert_eq!(fix, format!("export LAYERFS_ROOT={}", folder.display()));
        assert!(!fix.contains("mv "), "{fix}");
    }
    let got = layerfs_inspect::home::init::init(&folder).unwrap_err();
    assert_eq!(got.code, "InvalidInput");
    // `home get` says broken, with the same fix, and never `outdated`
    let read = layerfs_inspect::home::get::get(&folder, Default::default(), Default::default());
    let read = read.unwrap();
    assert_eq!(read.get_text("state"), Some("broken"));
    assert_eq!(read.get_text("reason"), Some("unsupported"));
    assert_eq!(read.get_text("fix"), Some(container_fix(&folder).as_str()));
    assert_eq!(before, listing(&folder), "the folder was written");
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_folder_with_layerfs_own_names_or_files_is_still_foreign() {
    for (tag, make) in [
        (
            "own-name",
            (|dir: &Path| std::fs::create_dir(dir.join("machine"))) as fn(&Path) -> _,
        ),
        ("file", |dir: &Path| {
            std::fs::write(dir.join("notes.txt"), "kept")
        }),
        ("home-file", |dir: &Path| {
            std::fs::write(dir.join("home"), "a file")
        }),
    ] {
        let folder = tempdir(tag);
        std::fs::create_dir(folder.join("dsh")).unwrap();
        make(&folder).unwrap();
        assert_eq!(detect(&folder).unwrap(), Layout::Foreign, "{tag}");
        std::fs::remove_dir_all(folder).unwrap();
    }
}
