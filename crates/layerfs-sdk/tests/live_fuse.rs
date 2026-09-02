use layerfs_sdk::{
    Client, CommitId, CreateWorkspaceSession, EndWorkspaceMode, EntityName,
    LayerStackInitialization, LayerStackStore, LocalForkSource, WorkspaceCommitResult,
    WorkspacePlacement, WorkspaceProjection,
};
use std::os::unix::fs::FileExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::sync::Arc;

const DATA_DIRECTORIES: usize = 100;
const FILES_PER_DIRECTORY: usize = 100;
const BYTES_PER_FILE: usize = 2_500;
const EDIT_TARGET: &str = "data-0000/file-00000.bin";
const EDIT_MARKER: &[u8; 10] = b"E000000001";

#[test]
fn namespace_10000_materialization_and_real_fuse_have_one_canonical_root() {
    if std::env::var_os("LAYERFS_LIVE_FUSE").is_none() {
        return;
    }
    let root = temp("fuse-equality");
    let fixture = root.join("fixture");
    build_fixture(&fixture);
    let store_path = root.join("store.sqlite");
    let store = Arc::new(LayerStackStore::create(&store_path).unwrap());
    let client = Client::connect(store.clone()).unwrap();
    let initialized = client
        .initialize_layerstack(
            EntityName::new("project").unwrap(),
            LayerStackInitialization::Directory(fixture),
        )
        .unwrap();
    let materialized_branch = client
        .fork_branch(
            EntityName::new("materialized").unwrap(),
            LocalForkSource::Layer {
                layer_id: initialized.genesis_layer_id,
            },
        )
        .unwrap();
    let fuse_branch = client
        .fork_branch(
            EntityName::new("fuse").unwrap(),
            LocalForkSource::Layer {
                layer_id: initialized.genesis_layer_id,
            },
        )
        .unwrap();

    let materialized = root.join("materialized");
    let materialized_session = client
        .create_workspace_session(CreateWorkspaceSession {
            branch_id: materialized_branch,
            placement: WorkspacePlacement::Host {
                root: materialized.clone(),
            },
            projection: Some(WorkspaceProjection::Materialize),
        })
        .unwrap();
    overwrite(&materialized.join(EDIT_TARGET));
    let materialized_commit = commit(&client, materialized_session.id);
    verify_namespace(&materialized);

    let mount = root.join("fuse");
    let fuse_session = client
        .create_workspace_session(CreateWorkspaceSession {
            branch_id: fuse_branch,
            placement: WorkspacePlacement::Host {
                root: mount.clone(),
            },
            projection: Some(WorkspaceProjection::Fuse),
        })
        .unwrap();
    assert!(is_fuse_mount(&mount));
    overwrite(&mount.join(EDIT_TARGET));
    let fuse_commit = commit(&client, fuse_session.id);
    verify_namespace(&mount);
    assert_same_metadata(&materialized, &mount);
    assert_eq!(
        store.commit(materialized_commit).unwrap().unwrap().root_id,
        store.commit(fuse_commit).unwrap().unwrap().root_id
    );
    client
        .end_workspace_session(materialized_session.id, EndWorkspaceMode::Clean)
        .unwrap();
    client
        .end_workspace_session(fuse_session.id, EndWorkspaceMode::Clean)
        .unwrap();
    assert!(!is_mounted(&mount));

    drop(client);
    drop(store);
    let store = Arc::new(LayerStackStore::connect(&store_path).unwrap());
    let client = Client::connect(store.clone()).unwrap();
    reopen_and_verify(
        &client,
        materialized_branch,
        root.join("materialized-reopen"),
        WorkspaceProjection::Materialize,
    );
    let reopen_mount = root.join("fuse-reopen");
    reopen_and_verify(
        &client,
        fuse_branch,
        reopen_mount.clone(),
        WorkspaceProjection::Fuse,
    );
    assert!(!is_mounted(&reopen_mount));

    drop(client);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

fn assert_same_metadata(left: &std::path::Path, right: &std::path::Path) {
    assert_eq!(metadata(left), metadata(right), "fixture root");
    for directory in 0..DATA_DIRECTORIES {
        let directory = format!("data-{directory:04}");
        assert_eq!(
            metadata(&left.join(&directory)),
            metadata(&right.join(&directory)),
            "{directory}"
        );
        for file in 0..FILES_PER_DIRECTORY {
            let path = format!("{directory}/file-{file:05}.bin");
            assert_eq!(
                metadata(&left.join(&path)),
                metadata(&right.join(&path)),
                "{path}"
            );
        }
    }
}

fn metadata(path: &std::path::Path) -> (u32, i64, i64) {
    let metadata = std::fs::symlink_metadata(path).unwrap();
    (
        metadata.permissions().mode(),
        metadata.mtime(),
        metadata.mtime_nsec(),
    )
}

fn build_fixture(root: &std::path::Path) {
    std::fs::create_dir(root).unwrap();
    for directory in 0..DATA_DIRECTORIES {
        let data = root.join(format!("data-{directory:04}"));
        std::fs::create_dir(&data).unwrap();
        for file in 0..FILES_PER_DIRECTORY {
            let path = format!("data-{directory:04}/file-{file:05}.bin");
            std::fs::write(root.join(&path), file_bytes(&path)).unwrap();
        }
    }
}

fn file_bytes(path: &str) -> Vec<u8> {
    let path = path.as_bytes();
    let mut bytes = (0..BYTES_PER_FILE)
        .map(|index| path[index % path.len()] ^ (index as u8).wrapping_mul(31))
        .collect::<Vec<_>>();
    bytes[..path.len()].copy_from_slice(path);
    bytes
}

fn overwrite(path: &std::path::Path) {
    let offset = 2_654_435_761_u64 % (BYTES_PER_FILE as u64 - EDIT_MARKER.len() as u64);
    let file = std::fs::File::options()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    file.write_all_at(EDIT_MARKER, offset).unwrap();
    file.sync_all().unwrap();
    file.set_times(std::fs::FileTimes::new().set_modified(
        std::time::UNIX_EPOCH + std::time::Duration::new(1_700_000_000, 123_456_789),
    ))
    .unwrap();
}

fn expected_file(path: &str) -> Vec<u8> {
    let mut bytes = file_bytes(path);
    if path == EDIT_TARGET {
        let offset =
            (2_654_435_761_u64 % (BYTES_PER_FILE as u64 - EDIT_MARKER.len() as u64)) as usize;
        bytes[offset..offset + EDIT_MARKER.len()].copy_from_slice(EDIT_MARKER);
    }
    bytes
}

fn verify_namespace(root: &std::path::Path) {
    let mut directories = names(root);
    let expected_directories = (0..DATA_DIRECTORIES)
        .map(|directory| format!("data-{directory:04}"))
        .collect::<Vec<_>>();
    assert_eq!(directories, expected_directories);
    let mut files = 0_usize;
    let mut bytes = 0_u64;
    for directory in 0..DATA_DIRECTORIES {
        let name = format!("data-{directory:04}");
        directories = names(&root.join(&name));
        let expected = (0..FILES_PER_DIRECTORY)
            .map(|file| format!("file-{file:05}.bin"))
            .collect::<Vec<_>>();
        assert_eq!(directories, expected);
        for file in 0..FILES_PER_DIRECTORY {
            let path = format!("{name}/file-{file:05}.bin");
            let actual = std::fs::read(root.join(&path)).unwrap();
            assert_eq!(actual, expected_file(&path), "{path}");
            files += 1;
            bytes += actual.len() as u64;
        }
    }
    assert_eq!(files, DATA_DIRECTORIES * FILES_PER_DIRECTORY);
    assert_eq!(bytes, 25_000_000);
}

fn names(root: &std::path::Path) -> Vec<String> {
    let mut names = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn commit(client: &Client, workspace: layerfs_sdk::WorkspaceId) -> CommitId {
    match client.commit_workspace_session(workspace).unwrap() {
        WorkspaceCommitResult::Created { commit_id, .. } => commit_id,
        result => panic!("unexpected Commit: {result:?}"),
    }
}

fn reopen_and_verify(
    client: &Client,
    branch_id: layerfs_sdk::BranchId,
    root: std::path::PathBuf,
    projection: WorkspaceProjection,
) {
    let session = client
        .create_workspace_session(CreateWorkspaceSession {
            branch_id,
            placement: WorkspacePlacement::Host { root: root.clone() },
            projection: Some(projection),
        })
        .unwrap();
    if projection == WorkspaceProjection::Fuse {
        assert!(is_fuse_mount(&root));
    }
    verify_namespace(&root);
    client
        .end_workspace_session(session.id, EndWorkspaceMode::Clean)
        .unwrap();
}

fn is_fuse_mount(root: &std::path::Path) -> bool {
    mount_line(root).is_some_and(|line| {
        line.split_once(" - ")
            .and_then(|(_, tail)| tail.split_whitespace().next())
            .is_some_and(|filesystem| filesystem.starts_with("fuse"))
    })
}

fn is_mounted(root: &std::path::Path) -> bool {
    mount_line(root).is_some()
}

fn mount_line(root: &std::path::Path) -> Option<String> {
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_owned());
    let root = root.to_string_lossy();
    std::fs::read_to_string("/proc/self/mountinfo")
        .ok()?
        .lines()
        .find(|line| line.split_whitespace().nth(4) == Some(root.as_ref()))
        .map(str::to_owned)
}

fn temp(label: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "layerfs-sdk-v4-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}
