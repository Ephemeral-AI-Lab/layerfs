//! The LayerFS folder: the default home `~/.layerfs/home`, the legacy
//! `~/.layerfs` that still holds `layerfs.json`, and a folder holding
//! `home/` beside clients' folders, which is never a home to move aside.
use layerfs_home::disk::layout::container_fix;
use layerfs_home::disk::{default_home, detect, ensure, open, Layout};
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
fn the_default_home_is_home_in_the_layerfs_folder() {
    let user = tempdir("default");
    assert_eq!(default_home(&user), user.join(".layerfs/home"));
    // a folder of clients only, or a home already moved into it, changes nothing
    std::fs::create_dir_all(user.join(".layerfs/dsh")).unwrap();
    assert_eq!(default_home(&user), user.join(".layerfs/home"));
    ensure(&user.join(".layerfs/home")).unwrap();
    assert_eq!(default_home(&user), user.join(".layerfs/home"));
    std::fs::remove_dir_all(user).unwrap();
}

#[test]
fn a_legacy_home_at_the_layerfs_folder_is_used_while_it_holds_layerfs_json() {
    let user = tempdir("legacy");
    let legacy = user.join(".layerfs");
    ensure(&legacy).unwrap();
    assert_eq!(default_home(&user), legacy);
    // a `layerfs.json` this build cannot read still names the legacy home,
    // so its own refusal is what the caller sees, not a new empty home
    std::fs::write(legacy.join("layerfs.json"), "not json").unwrap();
    assert_eq!(default_home(&user), legacy);
    std::fs::remove_file(legacy.join("layerfs.json")).unwrap();
    assert_eq!(default_home(&user), legacy.join("home"));
    std::fs::remove_dir_all(user).unwrap();
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
            refused.message.contains("LayerFS folder, not a home"),
            "{}",
            refused.message
        );
        let fix = refused.detail.get_text("fix").unwrap();
        assert_eq!(fix, container_fix(&folder));
        assert_eq!(
            fix,
            format!("export LAYERFS_HOME={}", folder.join("home").display())
        );
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
