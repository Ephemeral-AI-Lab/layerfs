use layerfs_home::home::{self, paths};
use layerfs_home::names::StoreName;
use layerfs_store::{lease, sqlite::create};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

struct Home(PathBuf);

impl Home {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "layerfs-lease-release-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        home::open(&path).unwrap();
        create::create(&path, &StoreName::parse("shop").unwrap(), None).unwrap();
        Self(path)
    }

    fn lease_file(&self) -> PathBuf {
        paths::lease_dir(&self.0, "shop").join(std::process::id().to_string())
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn explicit_release_removes_only_the_final_use() {
    let home = Home::new();
    let one = lease::hold(&home.0, "shop").unwrap();
    let two = lease::hold(&home.0, "shop").unwrap();
    one.release().unwrap();
    assert!(home.lease_file().is_file());
    two.release().unwrap();
    assert!(!home.lease_file().exists());

    let absent = lease::hold(&home.0, "shop").unwrap();
    std::fs::remove_file(home.lease_file()).unwrap();
    absent.release().unwrap();
}

#[test]
fn explicit_release_reports_a_failed_unlink() {
    let home = Home::new();
    let lease = lease::hold(&home.0, "shop").unwrap();
    let file = home.lease_file();
    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir(&file).unwrap();
    assert_eq!(lease.release().unwrap_err().code, "Failed");
    assert!(file.is_dir());
    assert!(!lease::held_here(&home.0, "shop"));
}
