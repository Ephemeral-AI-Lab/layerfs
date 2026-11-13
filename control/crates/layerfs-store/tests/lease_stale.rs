//! Stale Store leases (H §7.8, O35) through the public lease calls: a
//! read reports a stale lease and changes nothing; `holders` removes and
//! logs it once, keeps the live holders' leases, and a lease already gone
//! is no failure. Replaces the inline `remove_stale` test that sat in
//! product source.
use layerfs_home::home::paths;
use layerfs_store::lease::{holders, read_holders};

/// Above `i32::MAX`: names no single process, so never alive.
const GONE: u32 = i32::MAX as u32 + 1;
/// Always present (`kill` answers it, or refuses it with EPERM).
const LIVE: u32 = 1;

struct Root(std::path::PathBuf);

impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_stale_lease_is_removed_once_and_a_live_one_kept() {
    let root = Root(std::env::temp_dir().join(format!("layerfs-lease-{}", std::process::id())));
    let home = root.0.as_path();
    let dir = paths::lease_dir(home, "s");
    std::fs::create_dir_all(&dir).unwrap();
    let stale = dir.join(GONE.to_string());
    let live = dir.join(LIVE.to_string());
    std::fs::write(&stale, "1\n").unwrap();
    std::fs::write(&live, "1\n").unwrap();
    let log = paths::log_dir(home).join("leases.jsonl");

    // a read reports both and removes nothing
    assert_eq!(read_holders(home, "s").unwrap(), (vec![LIVE], vec![GONE]));
    assert!(stale.exists() && !log.exists());

    // the first use removes the stale lease and logs it; the live one stays
    assert_eq!(holders(home, "s").unwrap(), vec![LIVE]);
    assert!(!stale.exists() && live.exists());
    let logged = std::fs::read_to_string(&log).unwrap();
    assert_eq!(logged.lines().count(), 1, "{logged}");
    assert!(logged.contains("removed_stale_lease_of"), "{logged}");

    // with the stale lease gone, a second use is no failure and logs nothing
    assert_eq!(holders(home, "s").unwrap(), vec![LIVE]);
    assert_eq!(std::fs::read_to_string(&log).unwrap(), logged);
}
