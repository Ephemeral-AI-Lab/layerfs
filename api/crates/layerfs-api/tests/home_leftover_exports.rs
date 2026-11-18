//! v19: the image export containers killed copies left in the host's Docker
//! are `home leftovers` entries of kind `export_container` (machine
//! `native`, no path, no size), asked of Docker only once an image copy
//! has run from the home; a Docker that does not answer names `native` in
//! `not_listed`, blocked by `docker`'s `engine_down`; `home prune` removes
//! one with `docker rm -v` while no copy of its image holds the lock. The
//! Docker here is a stand-in shell script on `PATH` (one test owns `PATH`).
#![cfg(feature = "machine")]
#![allow(deprecated)] // nix's flock, as the product uses it
use layerfs_bridge::control::SdkValue;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const HEX: &str = "abababababababababababababababababababababababababababababababab";

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-lexp-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, (String, String)> {
    let detail = ("detail".to_owned(), SdkValue::Bool(true));
    let args = SdkValue::Map(
        (args.iter().map(|(n, v)| ((*n).to_owned(), v.clone())))
            .chain([detail])
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

fn items(listed: &SdkValue) -> Vec<SdkValue> {
    match listed.get("items") {
        Some(SdkValue::List(items)) => items.clone(),
        _ => Vec::new(),
    }
}

/// The stand-in: `down` makes every call fail as an Engine that is not
/// running; otherwise it lists one legacy export container (gone once
/// `rm -v` removed it) and records each `rm`. Only shell built-ins: the
/// product runs it with the machine tools' `PATH`, which is this folder.
fn standin(dir: &Path) {
    let script = format!(
        r#"#!/bin/sh
D="{dir}"
[ "$1" = "-H" ] && shift 2
[ -f "$D/down" ] && {{ echo "Cannot connect to the Docker daemon" >&2; exit 1; }}
case "$1" in
ps)
  case "$*" in
  *name=layerfs-export-*) [ -f "$D/removed" ] || printf 'layerfs-export-docker-{HEX}\t\n' ;;
  esac ;;
container)
  [ -f "$D/removed" ] && {{ echo "Error: No such container: $6" >&2; exit 1; }}
  printf '/layerfs-export-docker-{HEX}\t2026-11-19T09:58:35.123Z\n' ;;
rm) echo "$*" >> "$D/rm.log"; : > "$D/removed"; echo "$3" ;;
*) exit 1 ;;
esac
"#,
        dir = dir.display()
    );
    let path = dir.join("docker");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn export_containers_are_listed_once_a_copy_ran_and_pruned_under_the_copy_lock() {
    let tools = tempdir("tools");
    standin(&tools);
    std::env::set_var("PATH", &tools);
    std::env::set_var("LAYERFS_TOOL_FALLBACKS", "");
    let home = tempdir("home");
    layerfs_api::home::open(&home).unwrap();
    let native = [("machine", t("native"))];
    let id = format!("native:export_container:layerfs-export-docker-{HEX}");
    let exports = |listed: &SdkValue| {
        items(listed)
            .into_iter()
            .filter(|item| item.get_text("kind") == Some("export_container"))
            .collect::<Vec<_>>()
    };

    // no image copy ran from this home: Docker is not asked
    std::fs::write(tools.join("down"), b"").unwrap();
    let listed = call(&home, "home.leftovers", &native).unwrap();
    assert!(exports(&listed).is_empty(), "{listed:?}");
    assert_eq!(
        listed.get("not_listed"),
        Some(&SdkValue::List(Vec::new())),
        "{listed:?}"
    );

    // once one ran, a Docker that does not answer blocks native's listing
    let locks = home.join("run/image-lock/docker");
    std::fs::create_dir_all(&locks).unwrap();
    let listed = call(&home, "home.leftovers", &native).unwrap();
    let Some(SdkValue::List(not_listed)) = listed.get("not_listed") else {
        panic!("native is not listed: {listed:?}");
    };
    assert_eq!(not_listed.len(), 1, "{listed:?}");
    assert_eq!(not_listed[0].get_text("machine"), Some("native"));
    assert_eq!(
        not_listed[0].get("blocked_by"),
        Some(&SdkValue::map([
            ("machine", t("docker")),
            ("reason", t("engine_down"))
        ]))
    );

    // a Docker that answers: the killed copy's container, removable
    std::fs::remove_file(tools.join("down")).unwrap();
    let listed = call(&home, "home.leftovers", &native).unwrap();
    let found = exports(&listed);
    assert_eq!(found.len(), 1, "{listed:?}");
    let item = &found[0];
    assert_eq!(item.get_text("leftover"), Some(id.as_str()));
    assert_eq!(item.get_text("machine"), Some("native"));
    assert_eq!(item.get("path"), Some(&SdkValue::Null));
    assert_eq!(item.get("size"), Some(&SdkValue::Null));
    assert_eq!(
        item.get("report_only"),
        Some(&SdkValue::Bool(false)),
        "{item:?}"
    );
    assert_eq!(item.get_text("modified_at"), Some("2026-11-19T09:58:35Z"));

    // a copy of its image to its machine holds the lock: InUse, nothing run
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(locks.join(format!("{HEX}.lock")))
        .unwrap();
    nix_lock(&lock, true);
    let (code, message) = call(&home, "home.prune", &[("leftover", t(&id))]).unwrap_err();
    assert_eq!(code, "InUse", "{message}");
    assert!(!tools.join("rm.log").exists(), "nothing was removed");
    nix_lock(&lock, false);

    // free: `docker rm -v` removes it, and it is gone
    let pruned = call(&home, "home.prune", &[("leftover", t(&id))]).unwrap();
    assert_eq!(
        pruned.get_text("kind"),
        Some("export_container"),
        "{pruned:?}"
    );
    assert_eq!(pruned.get("path"), Some(&SdkValue::Null));
    let log = std::fs::read_to_string(tools.join("rm.log")).unwrap();
    assert_eq!(log.trim(), format!("rm -v layerfs-export-docker-{HEX}"));
    let (code, _) = call(&home, "home.prune", &[("leftover", t(&id))]).unwrap_err();
    assert_eq!(code, "NotFound");
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(tools).unwrap();
}

/// Takes (or releases) an exclusive `flock` on `file`, as a copy does.
fn nix_lock(file: &std::fs::File, take: bool) {
    use nix::fcntl::{flock, FlockArg};
    use std::os::fd::AsRawFd;
    let arg = match take {
        true => FlockArg::LockExclusiveNonblock,
        false => FlockArg::Unlock,
    };
    flock(file.as_raw_fd(), arg).unwrap();
}
