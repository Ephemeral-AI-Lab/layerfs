#![cfg(target_os = "macos")]

use layerfs_core::inode::{inode_table_lookup, InodeTableCounters, InodeTableRoot};
use layerfs_core::namespace::{directory_lookup, DirectoryStateRoot, NamespaceCounters};
use layerfs_core::namespace_codec::{decode_inode_record, decode_namespace_root};
use layerfs_core::CanonicalName;
use layerfs_sdk::LayerFs;
use layerfs_vfs::VfsError;
use std::fs;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn ordinary_apple_workspace_captures_bash_reopens_and_reads_history() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-sdk-workflow-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let store = base.join("store.sqlite");
    let opened = LayerFs::open(&store).unwrap();
    let mut workspace = opened
        .fs
        .materialize_external(opened.head, &base.join("workspace"))
        .unwrap();
    fs::create_dir_all(workspace.path().join("nested/scripts")).unwrap();
    fs::write(workspace.path().join("empty"), []).unwrap();
    fs::write(
        workspace.path().join("nested/large.bin"),
        vec![0x5a; 1_048_576],
    )
    .unwrap();
    fs::write(
        workspace.path().join("nested/scripts/run.sh"),
        b"#!/bin/bash\nprintf initial\n",
    )
    .unwrap();
    fs::set_permissions(
        workspace.path().join("nested/scripts/run.sh"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let script = workspace.path().join("nested/scripts/run.sh");
    assert!(Command::new("chmod")
        .args(["+a", "everyone allow read"])
        .arg(&script)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("chflags")
        .arg("hidden")
        .arg(&script)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("xattr")
        .args([
            "-wx",
            "com.apple.FinderInfo",
            "0000000000000000000000000000000000000000000000000000000000000001",
        ])
        .arg(&script)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("xattr")
        .args(["-w", "com.apple.ResourceFork", "resource-fork"])
        .arg(&script)
        .status()
        .unwrap()
        .success());
    symlink("nested/large.bin", workspace.path().join("relative-link")).unwrap();
    symlink(
        "/tmp/layerfs-absolute-target",
        workspace.path().join("absolute-link"),
    )
    .unwrap();
    symlink("missing-target", workspace.path().join("dangling-link")).unwrap();
    fs::hard_link(
        workspace.path().join("nested/large.bin"),
        workspace.path().join("large-hardlink"),
    )
    .unwrap();
    assert!(Command::new("xattr")
        .args(["-w", "com.layerfs.workflow", "exact-xattr"])
        .arg(workspace.path().join("nested/large.bin"))
        .status()
        .unwrap()
        .success());
    fs::File::open(&script)
        .unwrap()
        .set_times(
            fs::FileTimes::new()
                .set_modified(UNIX_EPOCH.checked_sub(Duration::from_secs(2)).unwrap()),
        )
        .unwrap();
    let root_a = workspace.capture_quiescent().unwrap();
    assert_ne!(
        root_a, opened.head,
        "nonempty capture returned the empty root"
    );
    assert!(matches!(
        workspace.capture_quiescent(),
        Err(VfsError::InvalidState)
    ));
    let workspace_path = workspace.path().to_owned();
    drop(workspace);
    let mut workspace = opened.fs.open_external(&workspace_path).unwrap();

    let substitution_path = base.join("substitution");
    let moved_substitution = base.join("substitution-moved");
    let mut substituted = opened
        .fs
        .materialize_external(root_a, &substitution_path)
        .unwrap();
    fs::rename(&substitution_path, &moved_substitution).unwrap();
    fs::create_dir(&substitution_path).unwrap();
    fs::write(substitution_path.join("wrong"), b"substitute").unwrap();
    assert!(substituted.capture_quiescent().is_err());
    assert_eq!(LayerFs::open(&store).unwrap().head, root_a);
    drop(substituted);
    fs::remove_dir_all(&substitution_path).unwrap();
    fs::remove_dir_all(&moved_substitution).unwrap();

    let a_path = base.join("root-a");
    let _a = opened.fs.materialize_external(root_a, &a_path).unwrap();
    assert_eq!(
        fs::read(a_path.join("nested/large.bin")).unwrap(),
        vec![0x5a; 1_048_576]
    );
    let first = fs::metadata(a_path.join("nested/large.bin")).unwrap();
    let alias = fs::metadata(a_path.join("large-hardlink")).unwrap();
    assert_eq!((first.dev(), first.ino()), (alias.dev(), alias.ino()));
    let materialized_script = a_path.join("nested/scripts/run.sh");
    assert_eq!(fs::metadata(&materialized_script).unwrap().mtime(), -2);
    assert!(String::from_utf8_lossy(
        &Command::new("ls")
            .arg("-le")
            .arg(&materialized_script)
            .output()
            .unwrap()
            .stdout
    )
    .contains("everyone allow read"));
    assert!(String::from_utf8_lossy(
        &Command::new("stat")
            .args(["-f", "%Sf"])
            .arg(&materialized_script)
            .output()
            .unwrap()
            .stdout
    )
    .contains("hidden"));
    assert_eq!(
        Command::new("xattr")
            .args(["-p", "com.apple.ResourceFork"])
            .arg(&materialized_script)
            .output()
            .unwrap()
            .stdout,
        b"resource-fork\n"
    );
    assert_eq!(
        Command::new("xattr")
            .args(["-p", "com.layerfs.workflow"])
            .arg(a_path.join("nested/large.bin"))
            .output()
            .unwrap()
            .stdout,
        b"exact-xattr\n"
    );

    let duplicate = opened.fs.open_external(workspace.path()).unwrap();
    let lease = duplicate.register_writer().unwrap();
    assert!(matches!(
        workspace.capture_quiescent(),
        Err(VfsError::WorkspaceBusy)
    ));
    drop(lease);
    drop(duplicate);

    let failed = Command::new("/bin/bash")
        .current_dir(workspace.path())
        .arg("-c")
        .arg("exit 7")
        .status()
        .unwrap();
    assert_eq!(failed.code(), Some(7));
    assert_eq!(LayerFs::open(&store).unwrap().head, root_a);

    let status = Command::new("/bin/bash").current_dir(workspace.path()).arg("-c").arg("printf shell > shell.txt; dd if=/dev/zero of=nested/large.bin bs=4096 count=1 conv=notrunc 2>/dev/null; mkdir made; mv shell.txt made/moved.txt; rm empty; chmod 700 nested/scripts/run.sh; ln -s ../made/moved.txt nested/shell-link").status().unwrap();
    assert!(status.success());
    let mmap = Command::new("/usr/bin/python3")
        .current_dir(workspace.path())
        .arg("-c")
        .arg("import mmap; f=open('nested/large.bin','r+b'); m=mmap.mmap(f.fileno(),0); m[8192:8196]=b'MMAP'; m.flush(); m.close(); f.close()")
        .status()
        .unwrap();
    assert!(mmap.success());
    let root_b = workspace.capture_quiescent().unwrap();
    assert!(matches!(
        workspace.register_writer(),
        Err(VfsError::InvalidState)
    ));

    assert!(matches!(
        opened.fs.materialize_managed(root_a),
        Err(VfsError::ExternalDirtyConflict)
    ));
    let mut dirty = opened.fs.materialize_managed(root_b).unwrap();
    assert!(dirty.replace("missing", 0, 0, b"partial").is_err());
    assert!(matches!(
        dirty.capture(),
        Err(VfsError::ExternalDirtyConflict)
    ));
    dirty.discard().unwrap();
    let mut managed = opened.fs.materialize_managed(root_b).unwrap();
    managed.replace("nested/large.bin", 0, 4, b"EDIT").unwrap();
    managed
        .replace("nested/large.bin", 100, 0, b"INSERT")
        .unwrap();
    managed.replace("nested/large.bin", 200, 7, b"").unwrap();
    managed
        .replace("nested/large.bin", 900_000, 1_048_575 - 900_000, b"")
        .unwrap();
    managed
        .rename("made/moved.txt", "made/managed-moved.txt")
        .unwrap();
    managed
        .rename("made/managed-moved.txt", "nested/managed-moved.txt")
        .unwrap();
    let root_c = managed.capture().unwrap();
    managed.discard().unwrap();
    let mut discarded = opened.fs.materialize_managed(root_c).unwrap();
    discarded.discard().unwrap();
    assert!(matches!(discarded.capture(), Err(VfsError::InvalidState)));
    assert!(!fs::read_dir(&base).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".layerfs-managed-spool-")));
    let q = opened.fs.diagnostics().unwrap();
    assert_eq!(q.operation_q_current_bytes, 0);
    assert_eq!(q.operation_q_high_water_bytes, q.operation_q_bound_bytes);
    drop(workspace);
    drop(_a);
    drop(opened);

    let reopened = LayerFs::open(&store).unwrap();
    assert_eq!(reopened.head, root_c);
    let old = reopened
        .fs
        .materialize_external(root_a, &base.join("old"))
        .unwrap();
    let new = reopened
        .fs
        .materialize_external(root_b, &base.join("new"))
        .unwrap();
    let managed_result = reopened
        .fs
        .materialize_external(root_c, &base.join("managed-result"))
        .unwrap();
    let external_path = base.join("external-discard");
    let mut external = reopened
        .fs
        .materialize_external(root_a, &external_path)
        .unwrap();
    external.discard().unwrap();
    assert!(external_path.exists());
    assert!(old.path().join("empty").exists());
    assert!(!new.path().join("empty").exists());
    assert_eq!(
        fs::read(new.path().join("made/moved.txt")).unwrap(),
        b"shell"
    );
    assert_eq!(
        fs::read_link(new.path().join("nested/shell-link")).unwrap(),
        std::path::PathBuf::from("../made/moved.txt")
    );
    assert_eq!(
        &fs::read(new.path().join("nested/large.bin")).unwrap()[8192..8196],
        b"MMAP"
    );
    let managed_bytes = fs::read(managed_result.path().join("nested/large.bin")).unwrap();
    assert_eq!(managed_bytes.len(), 900_000);
    assert_eq!(&managed_bytes[..4], b"EDIT");
    assert_eq!(
        fs::read(managed_result.path().join("nested/managed-moved.txt")).unwrap(),
        b"shell"
    );
    assert_ne!(
        fs::metadata(new.path().join("nested/large.bin"))
            .unwrap()
            .modified()
            .unwrap(),
        fs::metadata(managed_result.path().join("nested/large.bin"))
            .unwrap()
            .modified()
            .unwrap(),
        "managed content mutation preserved the old metadata root"
    );
    assert_ne!(
        fs::metadata(new.path().join("made"))
            .unwrap()
            .modified()
            .unwrap(),
        fs::metadata(managed_result.path().join("made"))
            .unwrap()
            .modified()
            .unwrap(),
        "managed rename preserved the old parent metadata root"
    );
    assert!(!managed_result.path().join("made/moved.txt").exists());
    drop(old);
    drop(new);
    drop(managed_result);
    drop(external);
    let compacted = reopened.fs.compact(&store).unwrap();
    assert_eq!(compacted.head, root_c);
    let retained = compacted
        .fs
        .materialize_external(root_a, &base.join("post-compact-old"))
        .unwrap();
    assert!(retained.path().join("empty").exists());
    drop(retained);
    drop(compacted);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn hard_link_flags_are_finalized_after_the_last_alias_and_owned_discard_clears_them() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-sdk-hardlink-flags-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let opened = LayerFs::open(&base.join("store.sqlite")).unwrap();
    let mut source = opened
        .fs
        .materialize_external(opened.head, &base.join("source"))
        .unwrap();
    fs::write(source.path().join("representative"), b"linked").unwrap();
    fs::hard_link(
        source.path().join("representative"),
        source.path().join("alias"),
    )
    .unwrap();
    assert!(Command::new("chflags")
        .arg("uchg,uappnd")
        .arg(source.path().join("representative"))
        .status()
        .unwrap()
        .success());
    let root = source.capture_quiescent().unwrap();
    let managed = opened.fs.materialize_managed(root).unwrap();
    let mut managed = managed.into_external().unwrap();
    assert_eq!(
        fs::metadata(managed.path().join("representative"))
            .unwrap()
            .ino(),
        fs::metadata(managed.path().join("alias")).unwrap().ino()
    );
    let flags = Command::new("stat")
        .args(["-f", "%Sf"])
        .arg(managed.path().join("representative"))
        .output()
        .unwrap();
    let flags = String::from_utf8(flags.stdout).unwrap();
    assert!(flags.contains("uchg") && flags.contains("uappnd"));
    let managed_path = managed.path().to_owned();
    managed.discard().unwrap();
    assert!(!managed_path.exists());
    assert!(Command::new("chflags")
        .args(["nouchg,nouappnd"])
        .arg(source.path().join("representative"))
        .status()
        .unwrap()
        .success());
    drop(source);
    drop(opened);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn provenance_value_does_not_change_the_canonical_root() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-sdk-provenance-root-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let store = base.join("store");
    let opened = LayerFs::open(&store).unwrap();
    let mut first = opened
        .fs
        .materialize_external(opened.head, &base.join("first"))
        .unwrap();
    fs::write(first.path().join("file"), b"same").unwrap();
    let root = first.capture_quiescent().unwrap();
    drop(first);
    let mut second = opened
        .fs
        .materialize_external(root, &base.join("second"))
        .unwrap();
    let _ = Command::new("xattr")
        .args(["-wx", "com.apple.provenance", "090807"])
        .arg(second.path().join("file"))
        .status();
    assert_eq!(second.capture_quiescent().unwrap(), root);
    drop(second);
    drop(opened);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn new_earlier_hard_link_alias_preserves_only_live_topology_authority() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-sdk-live-hardlink-id-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let store = base.join("store");
    let opened = LayerFs::open(&store).unwrap();
    let mut initial = opened
        .fs
        .materialize_external(opened.head, &base.join("initial"))
        .unwrap();
    fs::write(initial.path().join("z-old"), b"linked").unwrap();
    fs::hard_link(initial.path().join("z-old"), initial.path().join("z-alias")).unwrap();
    let root_a = initial.capture_quiescent().unwrap();
    let inode_a = top_level_inode(&store, root_a, "z-old");

    let mut live = opened
        .fs
        .materialize_external(root_a, &base.join("live"))
        .unwrap();
    fs::hard_link(live.path().join("z-old"), live.path().join("a-new")).unwrap();
    let root_b = live.capture_quiescent().unwrap();
    assert_eq!(top_level_inode(&store, root_b, "a-new"), inode_a);
    assert_eq!(top_level_inode(&store, root_b, "z-old"), inode_a);

    opened.fs.rollback(root_a).unwrap();
    let initial_path = initial.path().to_owned();
    drop(initial);
    let mut reopened = opened.fs.open_external(&initial_path).unwrap();
    fs::hard_link(reopened.path().join("z-old"), reopened.path().join("a-new")).unwrap();
    let root_c = reopened.capture_quiescent().unwrap();
    assert_ne!(top_level_inode(&store, root_c, "a-new"), inode_a);
    drop(live);
    drop(reopened);
    drop(opened);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn live_hard_link_split_never_reuses_one_inode_for_two_native_keys() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-sdk-hardlink-split-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let store = base.join("store");
    let opened = LayerFs::open(&store).unwrap();
    let mut source = opened
        .fs
        .materialize_external(opened.head, &base.join("source"))
        .unwrap();
    fs::write(source.path().join("a"), b"linked").unwrap();
    fs::hard_link(source.path().join("a"), source.path().join("b")).unwrap();
    let root_a = source.capture_quiescent().unwrap();
    let inode = top_level_inode(&store, root_a, "a");

    let mut live = opened
        .fs
        .materialize_external(root_a, &base.join("live"))
        .unwrap();
    fs::remove_file(live.path().join("b")).unwrap();
    fs::write(live.path().join("b"), b"replacement").unwrap();
    let root_b = live.capture_quiescent().unwrap();
    assert_eq!(top_level_inode(&store, root_b, "a"), inode);
    assert_ne!(top_level_inode(&store, root_b, "b"), inode);
    drop(source);
    drop(live);
    drop(opened);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn into_external_rename_preserves_inode_by_native_hard_link_key() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-sdk-hardlink-rename-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let store = base.join("store");
    let opened = LayerFs::open(&store).unwrap();
    let mut source = opened
        .fs
        .materialize_external(opened.head, &base.join("source"))
        .unwrap();
    fs::write(source.path().join("z"), b"rename").unwrap();
    let root_a = source.capture_quiescent().unwrap();
    let inode = top_level_inode(&store, root_a, "z");

    let managed = opened.fs.materialize_managed(root_a).unwrap();
    let mut live = managed.into_external().unwrap();
    fs::rename(live.path().join("z"), live.path().join("a")).unwrap();
    let root_b = live.capture_quiescent().unwrap();
    assert_eq!(top_level_inode(&store, root_b, "a"), inode);
    drop(source);
    drop(live);
    drop(opened);
    fs::remove_dir_all(base).unwrap();
}

fn top_level_inode(
    store: &std::path::Path,
    root: layerfs_vfs::RootId,
    name: &str,
) -> layerfs_core::inode::InodeId {
    let engine = layerfs_engine::generation::open_current(
        store,
        layerfs_engine::integrity::IntegrityMode::Verified,
    )
    .unwrap();
    let namespace =
        decode_namespace_root(&engine.load_object(root).unwrap().canonical_bytes).unwrap();
    let table = InodeTableRoot(namespace.inode_table_root);
    let record_id = inode_table_lookup(
        &engine,
        table,
        namespace.root_directory_inode,
        &mut InodeTableCounters::default(),
    )
    .unwrap()
    .unwrap();
    let record =
        decode_inode_record(&engine.load_object(record_id).unwrap().canonical_bytes).unwrap();
    directory_lookup(
        &engine,
        DirectoryStateRoot(record.content_root),
        &CanonicalName::new(name).unwrap(),
        &mut NamespaceCounters::default(),
    )
    .unwrap()
    .unwrap()
}

#[test]
fn warm_materialization_is_exact_no_rewrite_and_mismatch_rolls_back() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-sdk-warm-noop-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let store = base.join("store");
    let opened = LayerFs::open(&store).unwrap();
    let mut source = opened
        .fs
        .materialize_external(opened.head, &base.join("source"))
        .unwrap();
    fs::write(source.path().join("file"), b"warm").unwrap();
    let root = source.capture_quiescent().unwrap();
    let cold = opened
        .fs
        .materialize_external(root, &base.join("projection"))
        .unwrap();
    let before = fs::metadata(cold.path().join("file")).unwrap();
    let ref_before = layerfs_engine::generation::open_current(
        &store,
        layerfs_engine::integrity::IntegrityMode::Verified,
    )
    .unwrap()
    .read_ref("main")
    .unwrap()
    .unwrap();
    let warm = opened.fs.materialize_external(root, cold.path()).unwrap();
    let after = fs::metadata(warm.path().join("file")).unwrap();
    assert_eq!(after.ino(), before.ino());
    assert_eq!(after.modified().unwrap(), before.modified().unwrap());
    let ref_after = layerfs_engine::generation::open_current(
        &store,
        layerfs_engine::integrity::IntegrityMode::Verified,
    )
    .unwrap()
    .read_ref("main")
    .unwrap()
    .unwrap();
    assert_eq!(ref_after, ref_before);

    fs::write(warm.path().join("file"), b"dirty").unwrap();
    assert!(matches!(
        opened.fs.materialize_external(root, warm.path()),
        Err(VfsError::ExternalDirtyConflict)
    ));
    assert_eq!(
        layerfs_engine::generation::open_current(
            &store,
            layerfs_engine::integrity::IntegrityMode::Verified,
        )
        .unwrap()
        .read_ref("main")
        .unwrap()
        .unwrap(),
        ref_before
    );
    drop(source);
    drop(cold);
    drop(warm);
    drop(opened);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn stale_managed_replay_marks_native_mutation_dirty_and_retry_fails_closed() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-sdk-stale-replay-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let opened = LayerFs::open(&base.join("store")).unwrap();
    let mut source = opened
        .fs
        .materialize_external(opened.head, &base.join("source"))
        .unwrap();
    fs::write(source.path().join("file"), vec![0x5a; 8192]).unwrap();
    let root = source.capture_quiescent().unwrap();

    let mut stale = opened.fs.materialize_managed(root).unwrap();
    stale.replace("file", 4096, 5, b"stale").unwrap();
    let mut winner = opened.fs.materialize_managed(root).unwrap();
    winner.replace("file", 4096, 6, b"winner").unwrap();
    winner.capture().unwrap();
    assert!(stale.capture().is_err());
    assert!(matches!(
        stale.capture(),
        Err(VfsError::ExternalDirtyConflict)
    ));
    stale.discard().unwrap();
    drop(source);
    drop(opened);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn into_external_preserves_inode_after_clone_and_in_place_fallback() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-sdk-managed-key-transfer-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let store = base.join("store");
    let opened = LayerFs::open(&store).unwrap();
    let mut source = opened
        .fs
        .materialize_external(opened.head, &base.join("source"))
        .unwrap();
    fs::write(source.path().join("clone"), b"clone-old").unwrap();
    fs::write(source.path().join("linked"), b"linked-old").unwrap();
    fs::hard_link(
        source.path().join("linked"),
        source.path().join("linked-alias"),
    )
    .unwrap();
    let root = source.capture_quiescent().unwrap();
    let clone_inode = top_level_inode(&store, root, "clone");
    let linked_inode = top_level_inode(&store, root, "linked");

    let mut managed = opened.fs.materialize_managed(root).unwrap();
    managed.replace("clone", 0, 9, b"clone-new").unwrap();
    managed.replace("linked", 0, 10, b"linked-new").unwrap();
    let mut external = managed.into_external().unwrap();
    let next = external.capture_quiescent().unwrap();
    assert_eq!(top_level_inode(&store, next, "clone"), clone_inode);
    assert_eq!(top_level_inode(&store, next, "linked"), linked_inode);
    assert_eq!(top_level_inode(&store, next, "linked-alias"), linked_inode);
    drop(source);
    drop(external);
    drop(opened);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn restrictive_file_allows_exact_noop_refuses_mutation_and_captures_read_only() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-sdk-native-protected-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let opened = LayerFs::open(&base.join("store")).unwrap();
    let mut source = opened
        .fs
        .materialize_external(opened.head, &base.join("source"))
        .unwrap();
    fs::write(source.path().join("file"), b"protected").unwrap();
    assert!(Command::new("chflags")
        .arg("uchg")
        .arg(source.path().join("file"))
        .status()
        .unwrap()
        .success());
    let root = source.capture_quiescent().unwrap();

    let managed = opened.fs.materialize_managed(root).unwrap();
    let mut external = managed.into_external().unwrap();
    assert_eq!(external.capture_quiescent().unwrap(), root);

    let mut rename = opened.fs.materialize_managed(root).unwrap();
    assert!(matches!(
        rename.rename("file", "moved"),
        Err(VfsError::NativeProtected)
    ));
    assert_eq!(rename.capture().unwrap(), root);

    let mut exact = opened.fs.materialize_managed(root).unwrap();
    exact.replace("file", 0, 9, b"protected").unwrap();
    assert_eq!(exact.capture().unwrap(), root);

    let mut refused = opened.fs.materialize_managed(root).unwrap();
    assert!(matches!(
        refused.replace("file", 0, 9, b"different"),
        Err(VfsError::NativeProtected)
    ));
    refused.discard().unwrap();
    assert!(Command::new("chflags")
        .arg("nouchg")
        .arg(source.path().join("file"))
        .status()
        .unwrap()
        .success());
    drop(source);
    drop(opened);
    fs::remove_dir_all(base).unwrap();
}
