#![cfg(target_os = "macos")]

use layerfs_os::apple::AppleDriver;
use layerfs_vfs::driver::NativeMetadata;
use layerfs_vfs::driver::{DriverError, ProjectionDriver, WorkspacePolicy};
use layerfs_vfs::workspace::LayerVfs;
use layerfs_vfs::NativeRoute;
use std::fs;
use std::io::Cursor;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn refresh_covers_lifecycle_metadata_and_hard_links() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-refresh-lifecycle-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let vfs = LayerVfs::open(&base.join("store"), Arc::new(AppleDriver::default())).unwrap();
    let empty = vfs.current_head("main").unwrap();
    let mut source = vfs
        .materialize_external(empty.root, &base.join("source"))
        .unwrap();
    fs::write(source.path().join("file"), b"short").unwrap();
    fs::write(source.path().join("delete"), b"gone").unwrap();
    fs::create_dir(source.path().join("gone-dir")).unwrap();
    fs::create_dir(source.path().join("dir")).unwrap();
    fs::write(source.path().join("dir/child"), b"child").unwrap();
    fs::write(source.path().join("shared"), b"shared-content").unwrap();
    fs::hard_link(source.path().join("shared"), source.path().join("alias")).unwrap();
    symlink("file", source.path().join("link")).unwrap();
    let root_a = source.capture_quiescent().unwrap();
    let state_a = vfs.current_head("main").unwrap();
    let mut managed = vfs.materialize_managed(root_a).unwrap();

    let mut target = vfs
        .materialize_external(root_a, &base.join("target"))
        .unwrap();
    fs::write(
        target.path().join("file"),
        b"a substantially longer replacement",
    )
    .unwrap();
    fs::remove_file(target.path().join("delete")).unwrap();
    fs::write(target.path().join("created"), b"new").unwrap();
    fs::remove_dir(target.path().join("gone-dir")).unwrap();
    fs::create_dir(target.path().join("new-dir")).unwrap();
    fs::rename(target.path().join("dir"), target.path().join("moved")).unwrap();
    fs::remove_file(target.path().join("link")).unwrap();
    symlink("created", target.path().join("link")).unwrap();
    let expected_link_metadata = symlink_metadata(target.path(), vfs.store_id().unwrap());
    fs::remove_file(target.path().join("alias")).unwrap();
    fs::hard_link(
        target.path().join("shared"),
        target.path().join("alias-two"),
    )
    .unwrap();
    fs::set_permissions(target.path(), fs::Permissions::from_mode(0o711)).unwrap();
    let root_b = target.capture_quiescent().unwrap();
    let state_b = vfs.current_head("main").unwrap();
    assert_eq!(state_a.generation + 1, state_b.generation);

    let counters = managed.refresh(&state_b).unwrap();
    assert_eq!(counters.native.route, Some(NativeRoute::FullFallback));
    assert_eq!(counters.full_fallback_files, 1);
    assert_eq!(counters.scratch_tables, 1);
    assert!(counters.scratch_statements > 0);
    assert!(counters.scratch_rows > 0);
    assert!(counters.scratch_high_water_bytes > 0);
    assert_eq!(
        counters.plan_scratch_high_water_bytes,
        counters.scratch_high_water_bytes
    );
    let mut target_two = vfs
        .materialize_external(root_b, &base.join("target-two"))
        .unwrap();
    fs::write(
        target_two.path().join("moved/child"),
        b"second refresh uses rotated topology",
    )
    .unwrap();
    let root_c = target_two.capture_quiescent().unwrap();
    let state_c = vfs.current_head("main").unwrap();
    let second = managed.refresh(&state_c).unwrap();
    assert_eq!(second.native.route, Some(NativeRoute::FullFallback));
    let mut refreshed = managed.into_external().unwrap();
    assert_eq!(
        fs::read(refreshed.path().join("file")).unwrap(),
        b"a substantially longer replacement"
    );
    assert!(!refreshed.path().join("delete").exists());
    assert_eq!(fs::read(refreshed.path().join("created")).unwrap(), b"new");
    assert!(!refreshed.path().join("gone-dir").exists());
    assert!(refreshed.path().join("new-dir").is_dir());
    assert_eq!(
        fs::read(refreshed.path().join("moved/child")).unwrap(),
        b"second refresh uses rotated topology"
    );
    assert_eq!(
        fs::read_link(refreshed.path().join("link")).unwrap(),
        Path::new("created")
    );
    assert_eq!(
        symlink_metadata(refreshed.path(), vfs.store_id().unwrap()),
        expected_link_metadata
    );
    let shared = fs::metadata(refreshed.path().join("shared")).unwrap();
    let alias = fs::metadata(refreshed.path().join("alias-two")).unwrap();
    assert_eq!(shared.ino(), alias.ino());
    assert_eq!(shared.nlink(), 2);
    assert!(!refreshed.path().join("alias").exists());
    assert_eq!(
        fs::metadata(refreshed.path()).unwrap().mode() & 0o777,
        0o711
    );
    assert_eq!(vfs.current_head("main").unwrap().root, root_c);
    refreshed.discard().unwrap();

    drop(target);
    drop(target_two);
    drop(source);
    drop(vfs);
    fs::remove_dir_all(base).unwrap();
}

fn symlink_metadata(path: &Path, store_id: [u8; 32]) -> NativeMetadata {
    let workspace = AppleDriver::default()
        .open_workspace(path, WorkspacePolicy::ExternalCooperative, store_id)
        .unwrap();
    let root = workspace.root_directory().unwrap();
    let token = workspace.token_at(root.as_ref(), b"link").unwrap();
    workspace
        .read_metadata_at(root.as_ref(), b"link", Some(&token))
        .unwrap()
}

#[test]
fn identity_checked_removal_never_unlinks_a_replacement() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-refresh-remove-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    fs::write(base.join("file"), b"retained").unwrap();
    let driver = AppleDriver::default();
    let workspace = driver
        .open_workspace(&base, WorkspacePolicy::ManagedPrivate, [7; 32])
        .unwrap();
    let root = workspace.root_directory().unwrap();
    let identity = workspace.identity_at(root.as_ref(), b"file").unwrap();
    let mut wrong = identity.clone();
    wrong[0] ^= 1;
    assert!(matches!(
        workspace.unlink_regular_at(root.as_ref(), b"file", &wrong),
        Err(DriverError::Conflict)
    ));
    assert_eq!(fs::read(base.join("file")).unwrap(), b"retained");
    workspace
        .unlink_regular_at(root.as_ref(), b"file", &identity)
        .unwrap();
    assert!(!base.join("file").exists());
    drop(workspace);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn selective_refresh_resolves_more_than_sixteen_topology_edges() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-refresh-deep-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let vfs = LayerVfs::open(&base.join("store"), Arc::new(AppleDriver::default())).unwrap();
    let empty = vfs.current_head("main").unwrap();
    let mut source = vfs
        .materialize_external(empty.root, &base.join("source"))
        .unwrap();
    let components = (0..20)
        .map(|index| format!("d{index:02}"))
        .collect::<Vec<_>>();
    let relative = format!("{}/file", components.join("/"));
    let mut directory = source.path().to_owned();
    for component in &components {
        directory.push(component);
        fs::create_dir(&directory).unwrap();
    }
    fs::write(directory.join("file"), b"before").unwrap();
    let root_a = source.capture_quiescent().unwrap();
    let state_a = vfs.current_head("main").unwrap();
    let mut managed = vfs.materialize_managed(root_a).unwrap();
    let path = layerfs_vfs::CanonicalPath::new(&relative).unwrap();
    let (state_b, _) = vfs
        .replace_file(&state_a, &path, Cursor::new(b"after and longer"))
        .unwrap();
    managed.refresh(&state_b).unwrap();
    let mut bytes = Vec::new();
    managed.read_to(&path, &mut bytes).unwrap();
    assert_eq!(bytes, b"after and longer");
    managed.discard().unwrap();

    drop(source);
    drop(vfs);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn nested_directory_delta_applies_when_root_directory_mapping_is_equal() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-refresh-nested-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let vfs = LayerVfs::open(&base.join("store"), Arc::new(AppleDriver::default())).unwrap();
    let empty = vfs.current_head("main").unwrap();
    let mut source = vfs
        .materialize_external(empty.root, &base.join("source"))
        .unwrap();
    fs::create_dir_all(source.path().join("a/remove-me")).unwrap();
    fs::create_dir(source.path().join("a/move-me")).unwrap();
    fs::write(source.path().join("a/move-me/child"), b"child").unwrap();
    let root_a = source.capture_quiescent().unwrap();
    let mut managed = vfs.materialize_managed(root_a).unwrap();

    let mut target = vfs
        .materialize_external(root_a, &base.join("target"))
        .unwrap();
    fs::remove_dir(target.path().join("a/remove-me")).unwrap();
    fs::create_dir(target.path().join("a/created")).unwrap();
    fs::rename(
        target.path().join("a/move-me"),
        target.path().join("a/moved"),
    )
    .unwrap();
    target.capture_quiescent().unwrap();
    let state_b = vfs.current_head("main").unwrap();
    managed.refresh(&state_b).unwrap();
    let mut refreshed = managed.into_external().unwrap();
    assert!(!refreshed.path().join("a/remove-me").exists());
    assert!(refreshed.path().join("a/created").is_dir());
    assert_eq!(
        fs::read(refreshed.path().join("a/moved/child")).unwrap(),
        b"child"
    );
    refreshed.discard().unwrap();

    drop(target);
    drop(source);
    drop(vfs);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn symlink_metadata_only_refresh_syncs_parent_and_survives_fresh_open() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-refresh-symlink-metadata-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let vfs = LayerVfs::open(&base.join("store"), Arc::new(AppleDriver::default())).unwrap();
    let empty = vfs.current_head("main").unwrap();
    let mut source = vfs
        .materialize_external(empty.root, &base.join("source"))
        .unwrap();
    fs::write(source.path().join("file"), b"target").unwrap();
    symlink("file", source.path().join("link")).unwrap();
    let root_a = source.capture_quiescent().unwrap();
    let mut managed = vfs.materialize_managed(root_a).unwrap();
    let mut target = vfs
        .materialize_external(root_a, &base.join("target"))
        .unwrap();
    assert!(Command::new("touch")
        .args(["-h", "-t", "202001010101.01"])
        .arg(target.path().join("link"))
        .status()
        .unwrap()
        .success());
    let expected = symlink_metadata(target.path(), vfs.store_id().unwrap());
    target.capture_quiescent().unwrap();
    let state_b = vfs.current_head("main").unwrap();
    let counters = managed.refresh(&state_b).unwrap();
    assert_eq!(counters.native.replace_calls, 0);
    assert!(counters.native.metadata_calls > 0);
    assert!(counters.native.sync_calls > 0);
    let mut refreshed = managed.into_external().unwrap();
    assert_eq!(
        symlink_metadata(refreshed.path(), vfs.store_id().unwrap()),
        expected
    );
    refreshed.discard().unwrap();

    drop(target);
    drop(source);
    drop(vfs);
    fs::remove_dir_all(base).unwrap();
}
