//! #510: a view opens a pushed Store file in place, whole or as pieces,
//! read-only, with the push's binding; files that are no Store, or carry
//! no binding, are refused; remote failures map to the view's kinds.
#![cfg(any(target_os = "macos", target_os = "linux"))]
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::remote::RemoteFailureKind;
use layerfs_persistence::{
    Handles, PersistenceConfig, SqliteAcquisitionSchema, SqlitePackLayout, SqlitePersistenceProfile,
};
use layerfs_storage::StoragePolicy;
use layerfs_store_service::remote::push::BINDING;
use layerfs_store_service::remote::storage::local_folder::LocalFolder;
use layerfs_store_service::remote::storage::{pieces, Limits, Metadata, RemoteError};
use layerfs_store_service::remote::view::failure_kind;
use layerfs_store_service::remote::{open_view, ViewFailure};
use std::fs;
use std::path::{Path, PathBuf};

const KEY: &[u8] = b"remote-view-service";

fn folder(root: &Path, piece_size: u64) -> LocalFolder {
    LocalFolder::new(
        &root.join("remote"),
        Limits {
            max_file: piece_size,
            max_request: piece_size,
            piece_size,
        },
    )
    .unwrap()
}

fn pushed_store(root: &Path) -> PathBuf {
    let handles = Handles::create(
        PersistenceConfig::sqlite(root.join("store"))
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
            .with_sqlite_pack_layout(SqlitePackLayout::GroupRowsIndexed)
            .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: KEY.to_vec(),
            cursor_key: [7; 32],
            incarnation: 1,
        },
    )
    .unwrap();
    handles.snapshot(&root.join("pushed")).unwrap().path
}

#[test]
fn a_view_opens_a_pushed_store_read_only() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("remote-view-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("remote")).unwrap();
    let pushed = pushed_store(&root);
    let size = fs::metadata(&pushed).unwrap().len();
    let binding = Metadata::from([(BINDING.to_owned(), pieces::hex(KEY))]);
    // whole, and as pieces of 64 KiB
    let whole =
        pieces::upload(&folder(&root, 1 << 30), "w/store.sqlite", &pushed, &binding).unwrap();
    let pieced =
        pieces::upload(&folder(&root, 65536), "p/store.sqlite", &pushed, &binding).unwrap();
    assert!(pieced.pieces > 1);
    for (name, moved) in [("w/store.sqlite", &whole), ("p/store.sqlite", &pieced)] {
        let view = open_view(Box::new(folder(&root, 65536)), name, 4 << 20, [9; 32]).unwrap();
        assert!(view.service.is_read_only());
        assert_eq!(view.bytes(), size);
        assert_eq!(view.binding, KEY);
        assert_eq!(view.sha256(), Some(pieces::hex(&moved.sha256).as_str()));
        assert!(view.etag().is_some() && view.modified().is_some());
        let stats = view.stats();
        assert!(stats.requests >= 2 && stats.bytes > 0, "{stats:?}");
        assert!(stats.bytes < size || size <= 1 << 20, "{stats:?}");
    }

    // no binding recorded, and a file that is no Store
    pieces::upload(
        &folder(&root, 1 << 30),
        "nb/store.sqlite",
        &pushed,
        &Metadata::new(),
    )
    .unwrap();
    fs::write(root.join("junk"), vec![0x5a; 100_000]).unwrap();
    pieces::upload(
        &folder(&root, 1 << 30),
        "junk/store.sqlite",
        &root.join("junk"),
        &binding,
    )
    .unwrap();
    for name in ["nb/store.sqlite", "junk/store.sqlite"] {
        let opened = open_view(Box::new(folder(&root, 1 << 30)), name, 4 << 20, [9; 32]);
        assert!(matches!(opened, Err(ViewFailure::Invalid(_))), "{name}");
    }
    let missing = open_view(
        Box::new(folder(&root, 1 << 30)),
        "none/store.sqlite",
        1 << 20,
        [9; 32],
    );
    assert!(matches!(
        missing,
        Err(ViewFailure::Remote(RemoteError::Missing))
    ));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn remote_failures_map_to_view_kinds() {
    let refused = |status| RemoteError::Refused {
        status,
        code: String::new(),
    };
    let cases = [
        (
            RemoteError::Unavailable("down".into()),
            RemoteFailureKind::Unavailable,
        ),
        (refused(401), RemoteFailureKind::Denied),
        (refused(403), RemoteFailureKind::Denied),
        (RemoteError::Changed, RemoteFailureKind::Changed),
        (
            RemoteError::Uncertain("reset".into()),
            RemoteFailureKind::Unknown,
        ),
        (refused(500), RemoteFailureKind::Failed),
        (RemoteError::Missing, RemoteFailureKind::Failed),
    ];
    for (error, kind) in cases {
        assert_eq!(failure_kind(&error), kind, "{error}");
    }
}
