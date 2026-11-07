//! The Store connection reaper stays under #535 D11 (manual machine
//! lifecycle): it frees an open Store this process no longer uses. No real
//! waiting: the reaper's own pass is run with a chosen idle period.
#![cfg(unix)] // unix stat/lock semantics; the Windows face is its own port

use layerfs_api::home::{self};

use layerfs_bridge::control::SdkValue;
use layerfs_control::common::home::lease;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-reaper-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn ok(home: &Path, name: &str, args: &[(&str, &str)]) -> SdkValue {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), SdkValue::text(*v)))
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {})
        .unwrap_or_else(|r| panic!("{name}: {r:?}"))
}

#[test]
fn the_reaper_closes_an_unused_store() {
    let home = tempdir("home");
    let folder = tempdir("folder");
    std::fs::write(folder.join("README.md"), "hello\n").unwrap();
    home::open(&home).unwrap();
    ok(&home, "store.create", &[("name", "shop")]);
    let folder_text = folder.display().to_string();
    ok(
        &home,
        "project.init",
        &[("project", "shop/api"), ("folder", &folder_text)],
    );
    ok(&home, "branch.list", &[("project", "shop/api")]);
    // the calls left the Store open in this process: a pass with a long
    // period keeps it (it was just used)
    assert_eq!(
        layerfs_control::store::close_idle(Duration::from_secs(3600)),
        0
    );
    // the reaper's pass once it is unused past its period closes it, and
    // nothing is left open after
    assert!(layerfs_control::store::close_idle(Duration::ZERO) >= 1);
    assert_eq!(layerfs_control::store::close_idle(Duration::ZERO), 0);
    assert_eq!(lease::holders(&home, "shop").unwrap(), Vec::<u32>::new());
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&folder).unwrap();
}
