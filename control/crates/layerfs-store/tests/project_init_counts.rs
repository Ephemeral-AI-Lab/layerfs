//! A worker's folder copy is counted against the caller's walk before the
//! add: a short copy is refused whole, with nothing added to the Store.
use layerfs_home::names::StoreName;
use layerfs_store::project::frame::{CopyCounts, InitFrame};
use layerfs_store::project::{send, staged};
use std::path::{Path, PathBuf};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct Temp(PathBuf);
impl Temp {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "layerfs-init-counts-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Temp(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A source with a folder, two files, a symlink and a hard link: the walk
/// counts the hard link once per path, exactly as the copy carries it.
#[test]
fn measure_counts_the_paths_the_copy_carries() {
    let temp = Temp::new("measure");
    let source = temp.path().join("source");
    std::fs::create_dir_all(source.join("dir")).unwrap();
    std::fs::write(source.join("a.txt"), b"abc").unwrap();
    std::fs::write(source.join("dir/b.txt"), b"12345").unwrap();
    std::os::unix::fs::symlink("a.txt", source.join("link")).unwrap();
    std::fs::hard_link(source.join("a.txt"), source.join("hard")).unwrap();
    let counts = send::measure(&source, "p").unwrap();
    assert_eq!(
        counts,
        CopyCounts {
            // the root, dir, a.txt, dir/b.txt, link and hard
            entries: 6,
            // a.txt, dir/b.txt and hard: one per path
            files: 3,
            // 3 + 5 + 3
            bytes: 11,
        }
    );
    // the copy keeps the paths, not the link: a second file in place of
    // the hard link, so its own scan counts the same
    let copy = temp.path().join("copy");
    std::fs::create_dir_all(copy.join("dir")).unwrap();
    std::fs::write(copy.join("a.txt"), b"abc").unwrap();
    std::fs::write(copy.join("dir/b.txt"), b"12345").unwrap();
    std::os::unix::fs::symlink("a.txt", copy.join("link")).unwrap();
    std::fs::write(copy.join("hard"), b"abc").unwrap();
    assert_eq!(staged::counts(&copy).unwrap(), counts);
}

#[test]
fn a_short_copy_is_refused_and_removed_before_anything_is_added() {
    let temp = Temp::new("short");
    let root = temp.path().join("root");
    std::fs::create_dir_all(&root).unwrap();
    let id = "0123456789abcdef";
    let copy = staged::folder(&root, id);
    std::fs::create_dir_all(&copy).unwrap();
    std::fs::write(copy.join("one"), b"x").unwrap();
    let frame = InitFrame {
        store: StoreName::parse("shop").unwrap(),
        name: Some("p".to_owned()),
        staged: id.to_owned(),
        timeout_ms: Some(1000),
        store_record: None,
        // the caller says two files; the copy holds one
        counts: Some(CopyCounts {
            entries: 2,
            files: 2,
            bytes: 2,
        }),
    };
    let refusal = layerfs_store::project::init::in_worker(&root, &frame).unwrap_err();
    assert_eq!(refusal.code, "Failed", "{refusal:?}");
    assert!(
        refusal.message.contains("it is short"),
        "{}",
        refusal.message
    );
    assert!(!copy.exists(), "the short copy is removed");
    assert!(!root.join("store").exists(), "nothing was added");
}

#[test]
fn a_frame_without_counts_is_an_older_caller_and_is_not_checked() {
    let frame = InitFrame {
        store: StoreName::parse("shop").unwrap(),
        name: None,
        staged: "0123456789abcdef".to_owned(),
        timeout_ms: None,
        store_record: None,
        counts: None,
    };
    let encoded = frame.encode();
    let decoded = InitFrame::decode(&encoded).unwrap();
    assert_eq!(decoded.counts, None);
    // with counts, the frame carries all three
    let counted = InitFrame {
        counts: Some(CopyCounts {
            entries: 7,
            files: 5,
            bytes: 11,
        }),
        ..frame
    };
    let decoded = InitFrame::decode(&counted.encode()).unwrap();
    assert_eq!(decoded.counts, counted.counts);
}
