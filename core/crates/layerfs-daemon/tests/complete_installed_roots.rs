//! Q1 full native roots read through the installed daemon's direct Store ports.
#[path = "support/complete_bytes.rs"]
mod bytes;
#[path = "support/complete_fixture.rs"]
mod fixture;
#[path = "support/complete_oracle.rs"]
mod oracle;
#[allow(dead_code)]
#[path = "support/native_install.rs"]
mod support;
use layerfs_daemon::{store::BindRequest, Command, Owner, OwnerConfig, Response};
use layerfs_history::WorkspaceId;
fn proof(shape: fixture::Shape) {
    let f = fixture::Fixture::new(shape);
    prove_fixture(f);
}
fn prove_fixture(f: fixture::Fixture) {
    let shape = f.shape;
    let owner = Owner::start(
        &f.directory.join("overlay.sqlite"),
        layerfs_overlay::ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let store = &f.opened.store;
    let branch = layerfs_history::BranchId::from_slice(&f.manifest.branch).unwrap();
    let before = store.work();
    let first = store
        .bind(
            owner.client(),
            BindRequest {
                branch,
                workspace: WorkspaceId::from_authority([41; 32]).unwrap(),
            },
        )
        .unwrap();
    let post_install = store.work().object_batches - before.object_batches;
    assert!(post_install <= 16);
    let before = store.work();
    let second = store
        .bind(
            owner.client(),
            BindRequest {
                branch,
                workspace: WorkspaceId::from_authority([42; 32]).unwrap(),
            },
        )
        .unwrap();
    assert_eq!(store.work(), before, "warm bind demand count");
    let operation = first.workspace.operation().unwrap();
    oracle::run(&f, &operation);
    let cache = store.cache_work().unwrap();
    assert!(cache.charged_cache_bytes <= 8 * 1024 * 1024);
    println!("Q1_COUNTS shape={shape:?} post_install_mount_batches={post_install} warm_mount_batches=0 work={:?} cache={cache:?}",store.work());
    drop(operation);
    for bound in [&first, &second] {
        assert!(matches!(
            owner
                .client()
                .try_submit(Some(bound.workspace.route()), Command::Close)
                .unwrap()
                .wait()
                .unwrap()
                .result(),
            Ok(Response::Done)
        ));
    }
    drop((first, second));
    owner.stop().unwrap();
    f.cleanup();
}
#[test]
fn all_supported_native_paths_survive_install_with_full_oracles() {
    proof(fixture::Shape::Mixed)
}
#[test]
fn huge_native_namespace_is_complete_after_install() {
    let prepared = std::env::var_os("LAYERFS_Q1_PREPARED")
        .expect("explicit closed preparation for clone setup");
    prove_fixture(fixture::Fixture::prepared_wide(std::path::Path::new(
        &prepared,
    )));
}
#[test]
fn dense_500000000_native_bytes_survive_install() {
    proof(fixture::Shape::Dense)
}
#[test]
fn sparse_native_bytes_and_holes_survive_install() {
    proof(fixture::Shape::Sparse)
}

#[test]
#[ignore = "explicit sealed preparation, not an independent proof or measurement"]
fn prepare_huge_native_namespace() {
    let f = fixture::Fixture::new(fixture::Shape::Wide);
    println!("Q1_PREPARED {}", f.directory.display());
    drop(f);
}
