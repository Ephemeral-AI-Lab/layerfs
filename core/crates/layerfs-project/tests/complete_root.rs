//! Native full-root acquisition over public constructors and real host storage.
#![cfg(unix)]
mod support;
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::{read_all, FilesystemRead, LogicalPath};
use layerfs_history::HistoryCatalog;
#[cfg(target_os = "macos")]
use layerfs_history::HistoryCatalogConfig;
#[cfg(target_os = "macos")]
use layerfs_persistence::{
    Handles, PersistenceConfig, SqliteAcquisitionSchema, SqlitePersistenceProfile,
};
use layerfs_storage::port::acquisition::Acquisition;
use layerfs_storage::Storage;
#[cfg(target_os = "macos")]
use layerfs_storage::StoragePolicy;
use layerfs_telemetry::timer::Timing;
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs,
    os::unix::{ffi::OsStrExt, fs::symlink},
    sync::Arc,
};
use support::{
    memory_acquisition::MemoryAcquisition, memory_history::MemoryHistory,
    memory_metadata::MemoryMetadata, Fixture,
};

fn acquire_and_verify(
    fixture: &Fixture,
    store: &Storage,
    history: &dyn HistoryCatalog,
    acquisition: &dyn Acquisition,
) {
    let mut files = BTreeMap::from([
        (
            ".gitignore",
            b"node_modules/\n.cache/\noutput/\nignored.bin\n".to_vec(),
        ),
        (".git/index", b"binary git index\0\x01".to_vec()),
        (".git/objects/aa/object", b"git object".to_vec()),
        ("node_modules/pkg/index.js", b"module.exports = 42".to_vec()),
        (".cache/tool/data", b"cache contents".to_vec()),
        ("output/build/result", b"build output".to_vec()),
        ("ignored.bin", vec![0, 255, 1, 0, 127]),
    ]);
    for (path, body) in &files {
        let native = fixture.source.join(path);
        fs::create_dir_all(native.parent().unwrap()).unwrap();
        fs::write(native, body).unwrap();
    }
    // Native identity, rather than equal bytes, defines hard-link aliases.
    fs::hard_link(
        fixture.source.join(".git/index"),
        fixture.source.join(".cache/index-alias"),
    )
    .unwrap();
    fs::hard_link(
        fixture.source.join(".git/index"),
        fixture.path.join("outside-index"),
    )
    .unwrap();
    let index_bytes = files[".git/index"].clone();
    fs::write(fixture.source.join("output/index-copy"), &index_bytes).unwrap();
    files.insert(".cache/index-alias", index_bytes.clone());
    files.insert("output/index-copy", index_bytes);
    fs::create_dir(fixture.source.join("empty")).unwrap();
    let links = BTreeMap::from([
        ("dependency-link", b"node_modules/pkg".to_vec()),
        ("broken", b"missing-target".to_vec()),
        ("self", b"self".to_vec()),
        ("outside", b"../external-never-imported".to_vec()),
        ("opaque", vec![b'x', b'/', 0xff]),
    ]);
    fs::write(
        fixture.path.join("external-never-imported"),
        b"outside bytes",
    )
    .unwrap();
    for (name, target) in &links {
        symlink(OsStr::from_bytes(target), fixture.source.join(name)).unwrap();
    }
    let initialized = fixture.run_with(store, history, acquisition);
    assert_eq!(initialized.namespace_work.regular_aliases, 1);
    assert_eq!(initialized.namespace_work.unique_files, files.len() - 1);
    // All later reads use the published complete root, with no native fallback.
    fs::remove_dir_all(&fixture.source).unwrap();
    let reader = store.reader().unwrap();
    let mut view = FilesystemRead::new(
        &reader,
        layerfs_content::filesystem::root::FilesystemRootId(initialized.root),
    )
    .unwrap();
    let index = view
        .resolve(&LogicalPath::new(".git/index").unwrap())
        .unwrap();
    let alias = view
        .resolve(&LogicalPath::new(".cache/index-alias").unwrap())
        .unwrap();
    let copied = view
        .resolve(&LogicalPath::new("output/index-copy").unwrap())
        .unwrap();
    assert_eq!(index.serial, alias.serial);
    assert_ne!(index.serial, copied.serial);
    assert_eq!(index.value.content_root, copied.value.content_root);
    assert_eq!(index.value.namespace_ref_count, 2);
    assert_eq!(copied.value.namespace_ref_count, 1);
    let mut pending = vec![String::new()];
    let mut seen_files = BTreeMap::new();
    let mut seen_links = BTreeMap::new();
    let mut seen = 0;
    while let Some(path) = pending.pop() {
        seen += 1;
        let logical = LogicalPath::new(&path).unwrap();
        let inode = view.resolve(&logical).unwrap();
        match inode.value.kind {
            InodeKind::Directory => {
                let mut after = None;
                loop {
                    let page = view.list(&logical, after.as_ref(), 2, 1024).unwrap();
                    for (name, _) in page.entries {
                        pending.push(if path.is_empty() {
                            name.as_str().to_owned()
                        } else {
                            format!("{path}/{}", name.as_str())
                        });
                    }
                    match page.continuation {
                        Some(next) => after = Some(next),
                        None => break,
                    }
                }
            }
            InodeKind::RegularFile => {
                let mut body = Vec::new();
                Timing::disabled("full-root.read", |scope| {
                    read_all(
                        &reader,
                        inode.value.content_root,
                        &mut body,
                        scope.child("file"),
                    )
                })
                .0
                .unwrap();
                seen_files.insert(path, body);
            }
            InodeKind::Symlink => {
                let portable = layerfs_content::filesystem::attributes::read::read_portable(
                    &reader,
                    inode.value.metadata_root,
                    InodeKind::Symlink,
                    &mut layerfs_content::filesystem::attributes::read::AttributeReadWork::default(
                    ),
                )
                .unwrap();
                assert_eq!(portable.mode, 0o777);
                seen_links.insert(path, view.readlink(&logical).unwrap().as_bytes().to_vec());
            }
        }
    }
    assert_eq!(
        seen_files,
        files.into_iter().map(|(p, b)| (p.to_owned(), b)).collect()
    );
    assert_eq!(
        seen_links,
        links.into_iter().map(|(p, b)| (p.to_owned(), b)).collect()
    );
    assert_eq!(initialized.entries, seen);
    assert_eq!(
        view.resolve(&LogicalPath::new("empty").unwrap())
            .unwrap()
            .value
            .kind,
        InodeKind::Directory
    );
    assert!(view
        .resolve(&LogicalPath::new("external-never-imported").unwrap())
        .is_err());
    println!(
        "S9_COMPLETE_ROOT entries={} work={:?}",
        initialized.entries, initialized.namespace_work
    );
}

#[test]
fn native_import_keeps_ignored_git_dependencies_outputs_and_exact_link_targets() {
    let fixture = Fixture::new(0);
    let store = support::storage(Arc::new(MemoryMetadata::default()));
    let acquisition = MemoryAcquisition::default();
    acquire_and_verify(&fixture, &store, &MemoryHistory::default(), &acquisition);
    assert!(acquisition.operations().is_empty());
}

#[test]
#[cfg(target_os = "macos")]
fn real_host_profiles_acquire_and_reuse_the_exact_complete_root() {
    for profile in DEVELOPMENT_PROFILES {
        let fixture = Fixture::new(0);
        let handles = Handles::create(
            PersistenceConfig::sqlite(fixture.path.join("store.sqlite"))
                .with_sqlite_profile(profile)
                .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
            StoragePolicy::frozen_default(),
            &HistoryCatalogConfig {
                binding_key: b"complete-root".to_vec(),
                cursor_key: [18; 32],
                incarnation: 1,
            },
        )
        .unwrap();
        let store = support::storage(handles.storage.clone());
        acquire_and_verify(&fixture, &store, &handles.history, &handles.acquisition);
        use layerfs_storage::port::acquisition::Acquisition as _;
        assert!(handles.acquisition.abandoned(None, 8).unwrap().is_empty());
    }
}

/// Owner direction 2026-10-07: Disposable is the sole active development
/// verification profile. Durable execution of these bodies is NOT_RUN —
/// deferred by owner for Disposable-only development. Durable support and its
/// retained historical receipts are unchanged.
const DEVELOPMENT_PROFILES: [SqlitePersistenceProfile; 1] = [SqlitePersistenceProfile::Disposable];
