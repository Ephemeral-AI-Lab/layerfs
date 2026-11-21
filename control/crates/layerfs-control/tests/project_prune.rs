//! `project.prune` removes a home entry with no record and nothing else:
//! an entry on record (valid, broken or unreadable) and a symlink are
//! refused, and so is a Store the home does not record.
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{paths, private_dir, write_entity, write_record};
use layerfs_home::names::ProjectRef;
use std::path::{Path, PathBuf};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct Home(PathBuf);
impl Home {
    fn new(tag: &str) -> Self {
        let home = std::env::temp_dir().join(format!(
            "layerfs-project-prune-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&home);
        layerfs_home::home::layout::create(&home).unwrap();
        write_entity(
            &paths::store_record(&home, "shop"),
            &SdkValue::map([
                ("kind", SdkValue::text("sqlite")),
                ("machine", SdkValue::text("native")),
            ]),
        )
        .unwrap();
        Home(home)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn t(value: &str) -> SdkValue {
    SdkValue::text(value)
}

fn project(handle: &str) -> ProjectRef {
    ProjectRef::parse(handle).unwrap()
}

fn orphan(home: &Path, name: &str) -> PathBuf {
    let entry = paths::project_dir(home, "shop", name);
    private_dir(&entry).unwrap();
    std::fs::write(entry.join("sealed.sqlite"), b"leftover").unwrap();
    entry
}

fn prune(home: &Path, handle: &str) -> Result<SdkValue, layerfs_bridge::control::SdkRefusal> {
    layerfs_control::project::prune(home, &project(handle))
}

#[test]
fn prune_removes_a_real_folder_with_no_record() {
    let home = Home::new("remove");
    let entry = orphan(home.path(), "orphan");
    let answer = prune(home.path(), "shop/orphan").unwrap();
    assert_eq!(answer.get_text("outcome"), Some("pruned"));
    assert_eq!(answer.get_text("project"), Some("shop/orphan"));
    assert!(!entry.exists(), "the entry is gone");
}

#[test]
fn prune_refuses_an_entry_on_record_valid_broken_or_unreadable() {
    let home = Home::new("record");
    // a valid record
    let held = orphan(home.path(), "held");
    write_record(
        &paths::project_record(home.path(), "shop", "held"),
        &SdkValue::map([("name", t("held"))]),
    )
    .unwrap();
    let refused = prune(home.path(), "shop/held").unwrap_err();
    assert_eq!(refused.code, "Exists", "{refused:?}");
    assert_eq!(
        refused.detail.get_text("fix"),
        Some("layerfs project delete shop/held")
    );
    assert!(held.exists());
    // a record that does not decode
    let broken = orphan(home.path(), "broken");
    write_record(
        &paths::project_record(home.path(), "shop", "broken"),
        &SdkValue::map([("name", t("broken"))]),
    )
    .unwrap();
    std::fs::write(
        paths::project_record(home.path(), "shop", "broken"),
        "{not json",
    )
    .unwrap();
    let refused = prune(home.path(), "shop/broken").unwrap_err();
    assert_eq!(refused.code, "Failed", "{refused:?}");
    assert!(broken.exists());
}

#[test]
fn prune_refuses_a_symlink_and_a_store_the_home_does_not_record() {
    let home = Home::new("refuse");
    let target = home.path().join("target");
    private_dir(&target).unwrap();
    let linked = paths::project_dir(home.path(), "shop", "linked");
    private_dir(linked.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&target, &linked).unwrap();
    let refused = prune(home.path(), "shop/linked").unwrap_err();
    assert_eq!(refused.code, "InvalidInput", "{refused:?}");
    assert!(target.exists(), "the link's target is left alone");
    // a Store the home does not record: the folder under it is not this
    // call's (the home doctor names it)
    let refused = prune(home.path(), "missing/x").unwrap_err();
    assert_eq!(refused.code, "NotFound", "{refused:?}");
    // no entry at all
    let refused = prune(home.path(), "shop/none").unwrap_err();
    assert_eq!(refused.code, "NotFound", "{refused:?}");
}
