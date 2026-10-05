//! Public S3 proof over content-built trees; fixture memory is external to product.
mod common;
use common::{file, fixture, name, Store};
use layerfs_content::filesystem::{
    scope_for_seed, update_filesystem, DirectoryUpdate, FilesystemInput, FilesystemObjects,
    FilesystemResources, InodeUpdate,
};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, ObjectId};
use layerfs_overlay::{Overlay, ProfileConfig};
use layerfs_workspace::{BaseView, CanonicalClient, Workspace};
use std::sync::{atomic::Ordering, Arc, Mutex};

#[test]
fn full_root_binding_aliases_metadata_symlinks_and_eof_use_content_apis() {
    let f = fixture();
    let client = Arc::new(CanonicalClient::with_lengths(
        Arc::new(f.store.clone()),
        Arc::new(f.store.clone()),
        1024 * 1024,
    ));
    let before = f.store.demand.load(Ordering::Relaxed);
    let base = BaseView::open(client.clone(), f.root, f.scope).unwrap();
    assert_eq!(
        f.store.demand.load(Ordering::Relaxed) - before,
        1,
        "root binding must not scan"
    );
    let a = base.child(1, &name("file")).unwrap();
    assert_eq!(base.child(1, &name("alias")).unwrap().serial, a.serial);
    assert_eq!(a.value.namespace_ref_count, 2);
    assert_eq!(base.readlink(3).unwrap().as_bytes(), b"../.git/index");
    let stat = base.stat(1).unwrap();
    assert_eq!(stat.metadata.mode, 0o1777);
    assert_eq!(stat.metadata.mtime_seconds, i64::MAX);
    let mut all = Vec::new();
    let mut after = None;
    loop {
        let page = base.list(1, after.as_ref(), 2, 4096).unwrap();
        all.extend(page.entries.iter().map(|(n, _)| n.as_str().to_owned()));
        if page.continuation.is_none() {
            break;
        }
        after = page.continuation;
    }
    assert_eq!(
        all,
        vec![
            ".git",
            "alias",
            "cache",
            "file",
            "node_modules",
            "output",
            "symlink"
        ]
    );
    for (parent, child) in [(4, "index"), (5, "pkg"), (6, "state"), (7, "result")] {
        assert_eq!(base.child(parent, &name(child)).unwrap().serial, 8);
    }
    let plan = base.plan_read(8, 399_980, 128 * 1024).unwrap();
    assert_eq!(plan.length(), 20);
    let mut bytes = Vec::new();
    plan.emit(&mut bytes).unwrap();
    assert_eq!(bytes, f.bytes[399_980..]);
    assert_eq!(base.plan_read(8, u64::MAX, 32).unwrap().length(), 0);
    assert_eq!(
        base.child(1, &name("missing")).unwrap_err(),
        ContentError::PathNotFound
    );
    assert!(BaseView::open(client, f.root, scope_for_seed([19; 32])).is_err());
}

#[test]
fn retained_old_plan_and_new_root_use_exact_immutable_cache_identities() {
    let f = fixture();
    let client = Arc::new(CanonicalClient::with_lengths(
        Arc::new(f.store.clone()),
        Arc::new(f.store.clone()),
        512,
    ));
    let old = BaseView::open(client.clone(), f.root, f.scope).unwrap();
    let retained = old.plan_read(2, 0, 64).unwrap();
    let new_file = file(&f.store, b"changed");
    let mut value = old.inode(2).unwrap().value;
    value.content_root = new_file;
    let inodes = [InodeUpdate { serial: 2, value }];
    let mut sink = f.store.clone();
    let mut objects = FilesystemObjects::new(&f.store, &mut sink);
    let input = FilesystemInput {
        base: Some(f.root),
        scope: f.scope,
        root_serial: 1,
        directories: &[],
        inodes: &inodes,
        new_inodes: &[],
        resources: FilesystemResources::default(),
    };
    let root = update_filesystem(&mut objects, &input, None).unwrap().root;
    let new = BaseView::open(client.clone(), root, f.scope).unwrap();
    let mut bytes = Vec::new();
    retained.emit(&mut bytes).unwrap();
    assert_eq!(bytes, b"original\0\xff");
    bytes.clear();
    new.plan_read(2, 0, 64).unwrap().emit(&mut bytes).unwrap();
    assert_eq!(bytes, b"changed");
    assert_eq!(old.identity(), f.root);
    assert_ne!(new.identity(), f.root);
    assert!(client.diagnostics().unwrap().charged_cache_bytes <= 512);
    assert!(client.diagnostics().unwrap().evictions > 0);
}

#[test]
fn logical_workspace_open_binds_real_root_without_schema_or_namespace_import() {
    let f = fixture();
    let client = Arc::new(CanonicalClient::with_lengths(
        Arc::new(f.store.clone()),
        Arc::new(f.store.clone()),
        0,
    ));
    let path =
        std::env::temp_dir().join(format!("layerfs-base-open-{}.sqlite", std::process::id()));
    let overlay = Overlay::create(&path, ProfileConfig::default()).unwrap();
    let before = f.store.demand.load(Ordering::Relaxed);
    let workspace = Workspace::open(&overlay, client, f.root, f.scope, [91; 32]).unwrap();
    assert_eq!(f.store.demand.load(Ordering::Relaxed) - before, 1);
    assert_eq!(
        overlay.state(workspace.route()).unwrap().base_root,
        f.root.0.to_bytes()
    );
    assert_eq!(overlay.state(workspace.route()).unwrap().dirty_inodes, 0);
    drop(overlay);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn known_install_advances_selected_base_and_preserves_retained_plans_and_later_rows() {
    use layerfs_overlay::{
        Cell, Inode, InodeKind as LocalKind, OverlayError, CELL_BYTES, MASK_BYTES,
    };
    use layerfs_workspace::WorkspaceError;
    let f = fixture();
    let client = Arc::new(CanonicalClient::with_lengths(
        Arc::new(f.store.clone()),
        Arc::new(f.store.clone()),
        512,
    ));
    let path = std::env::temp_dir().join(format!(
        "layerfs-base-install-{}.sqlite",
        std::process::id()
    ));
    let overlay = Overlay::create(&path, ProfileConfig::default()).unwrap();
    let workspace = Workspace::open(&overlay, client.clone(), f.root, f.scope, [92; 32]).unwrap();
    let other = Workspace::open(&overlay, client, f.root, f.scope, [93; 32]).unwrap();
    let old = workspace.base().unwrap();
    let retained = old.plan_read(2, 0, 64).unwrap();
    let changed = b"changed";
    let mut data = Box::new([0; CELL_BYTES]);
    data[..changed.len()].copy_from_slice(changed);
    let mut validity = Box::new([0; MASK_BYTES]);
    validity[0] = 0x7f;
    let changed_inode = Inode {
        serial: 2,
        kind: LocalKind::File,
        mode: 0o644,
        mtime_seconds: i64::MAX,
        mtime_nanoseconds: 999_999_999,
        nlink: 2,
        size: changed.len() as u64,
        inherited_cutoff: 0,
        born: 0,
        entries: 0,
    };
    let publication = overlay
        .publish(
            workspace.route(),
            &changed_inode,
            None,
            Some(&Cell {
                offset: 0,
                data,
                validity,
            }),
        )
        .unwrap();
    overlay.reply_attempted(publication).unwrap();
    let capture = overlay.capture(workspace.route()).unwrap();
    let mut value = old.inode(2).unwrap().value;
    value.content_root = file(&f.store, changed);
    let updates = [InodeUpdate { serial: 2, value }];
    let mut sink = f.store.clone();
    let mut objects = FilesystemObjects::new(&f.store, &mut sink);
    let input = FilesystemInput {
        base: Some(f.root),
        scope: f.scope,
        root_serial: 1,
        directories: &[],
        inodes: &updates,
        new_inodes: &[],
        resources: FilesystemResources::default(),
    };
    let root = update_filesystem(&mut objects, &input, None).unwrap().root;
    let before = f.store.demand.load(Ordering::Relaxed);
    let prepared = workspace.prepare_base_install(capture, root).unwrap();
    assert_eq!(
        f.store.demand.load(Ordering::Relaxed) - before,
        1,
        "prepare must only acquire the new root"
    );
    assert_eq!(prepared.capture(), capture);
    assert_eq!(prepared.root(), root);
    assert_eq!(
        overlay.state(workspace.route()).unwrap().base_root,
        f.root.0.to_bytes()
    );
    let error = other
        .install_prepared_base(&overlay, prepared)
        .err()
        .unwrap();
    assert!(matches!(
        error.0,
        WorkspaceError::Overlay(OverlayError::Stale)
    ));
    assert_eq!(error.1.root(), root);
    drop(error);
    assert_eq!(other.base().unwrap().identity(), f.root);
    assert_eq!(
        overlay.retained_capture(workspace.route()).unwrap(),
        Some(capture)
    );
    let mut malformed = capture;
    malformed.revision += 1;
    let invalid = workspace.prepare_base_install(malformed, root).unwrap();
    let error = workspace
        .install_prepared_base(&overlay, invalid)
        .err()
        .unwrap();
    assert!(matches!(
        error.0,
        WorkspaceError::Overlay(OverlayError::Stale)
    ));
    assert_eq!(error.1.capture(), malformed);
    assert_eq!(error.1.root(), root);
    drop(error);
    assert_eq!(workspace.base().unwrap().identity(), f.root);
    assert_eq!(
        overlay.retained_capture(workspace.route()).unwrap(),
        Some(capture)
    );
    let prepared = workspace.prepare_base_install(capture, root).unwrap();
    let mut later = changed_inode;
    later.size = 10;
    later.mtime_seconds = 23;
    let later_publication = overlay
        .publish(workspace.route(), &later, None, None)
        .unwrap();
    workspace
        .install_prepared_base(&overlay, prepared)
        .map_err(|(error, _)| error)
        .unwrap();
    assert_eq!(workspace.base().unwrap().identity(), root);
    assert_eq!(
        overlay.state(workspace.route()).unwrap().base_root,
        root.0.to_bytes()
    );
    // The later row is a new payload layer over the sealed 7-byte view, which
    // the installed root now holds: unwritten bytes below 7 fall through.
    later.inherited_cutoff = 7;
    assert_eq!(overlay.inode(workspace.route(), 2).unwrap(), Some(later));
    assert_eq!(
        overlay.pending_publications(workspace.route(), 0).unwrap(),
        [later_publication]
    );
    let current = workspace.base().unwrap();
    assert_eq!(current.child(1, &name("file")).unwrap().serial, 2);
    assert_eq!(current.child(1, &name("alias")).unwrap().serial, 2);
    assert_eq!(current.readlink(3).unwrap().as_bytes(), b"../.git/index");
    let mut bytes = Vec::new();
    current
        .plan_read(2, 0, 64)
        .unwrap()
        .emit(&mut bytes)
        .unwrap();
    assert_eq!(bytes, changed);
    bytes.clear();
    retained.emit(&mut bytes).unwrap();
    assert_eq!(bytes, b"original\0\xff");
    assert_eq!(old.identity(), f.root);
    assert_eq!(old.inode(2).unwrap().value.namespace_ref_count, 2);
    assert_eq!(other.base().unwrap().identity(), f.root);
    drop(overlay);
    std::fs::remove_file(path).unwrap();
}

fn local(serial: u64, size: u64, links: u64) -> layerfs_overlay::Inode {
    layerfs_overlay::Inode {
        serial,
        kind: layerfs_overlay::InodeKind::File,
        mode: 0o600,
        mtime_seconds: 77,
        mtime_nanoseconds: 9,
        nlink: links,
        size,
        inherited_cutoff: size,
        born: 0,
        entries: 0,
    }
}
fn publish_name(
    db: &Overlay,
    route: layerfs_overlay::Route,
    inode: &layerfs_overlay::Inode,
    key: &[u8],
    target: Option<u64>,
) {
    let p = db
        .publish(
            route,
            inode,
            Some(&layerfs_overlay::Dentry {
                parent: 1,
                name: key.to_vec(),
                serial: target,
            }),
            None,
        )
        .unwrap();
    db.reply_attempted(p).unwrap();
}
#[test]
fn effective_alias_metadata_whiteouts_and_three_way_pages_advance_with_bounded_hidden_work() {
    let mut f = fixture();
    let mut sink = f.store.clone();
    let mut objects = FilesystemObjects::new(&f.store, &mut sink);
    let changes = [DirectoryUpdate {
        parent: 1,
        changes: (0..500)
            .map(|n| (name(&format!("z{n:04}")), Some(8)))
            .collect(),
    }];
    f.root = update_filesystem(
        &mut objects,
        &FilesystemInput {
            base: Some(f.root),
            scope: f.scope,
            root_serial: 1,
            directories: &changes,
            inodes: &[],
            new_inodes: &[],
            resources: FilesystemResources::default(),
        },
        None,
    )
    .unwrap()
    .root;
    let path = std::env::temp_dir().join(format!("layerfs-merge-{}.sqlite", std::process::id()));
    let db = Overlay::create(&path, ProfileConfig::default()).unwrap();
    let client = Arc::new(CanonicalClient::with_lengths(
        Arc::new(f.store.clone()),
        Arc::new(f.store.clone()),
        0,
    ));
    let ws = Workspace::open(&db, client, f.root, f.scope, [101; 32]).unwrap();
    for n in 0..130 {
        publish_name(
            &db,
            ws.route(),
            &local(8, 400000, 504 - n - 1),
            format!("z{n:04}").as_bytes(),
            None,
        );
    }
    publish_name(&db, ws.route(), &local(2, 21, 1), b"file", None);
    let capture = db.capture(ws.route()).unwrap();
    publish_name(&db, ws.route(), &local(999, 123, 1), b"file", Some(999));
    let source = db.acquire_base_source(ws.route(), 7).unwrap();
    let view = ws.view_for_source(source).unwrap();
    assert_eq!(view.lookup(&db, 1, &name("file")).unwrap().serial, 999);
    let alias = view.lookup(&db, 1, &name("alias")).unwrap();
    assert_eq!(
        (
            alias.serial,
            alias.metadata.mode,
            alias.logical_len,
            alias.namespace_refs
        ),
        (2, 0o600, 21, 1)
    );
    assert!(matches!(
        view.lookup(&db, 1, &name("z0000")),
        Err(layerfs_workspace::WorkspaceError::Content(
            ContentError::PathNotFound
        ))
    ));
    assert_eq!(
        view.lookup(&db, 1, &name("symlink")).unwrap().kind,
        InodeKind::Symlink
    );
    assert_eq!(
        view.base().readlink(3).unwrap().as_bytes(),
        b"../.git/index"
    );
    let mut after = None;
    let mut result = Vec::new();
    let mut empty_with_progress = 0;
    let mut visited = 0;
    loop {
        let page = view.list(&db, 1, after.as_deref()).unwrap();
        assert!(page.visited <= 64);
        visited += page.visited;
        if page.entries.is_empty() && page.continuation.is_some() {
            empty_with_progress += 1;
        }
        result.extend(page.entries);
        if let Some(next) = page.continuation {
            assert!(after.as_ref().is_none_or(|old: &Vec<u8>| old < &next));
            after = Some(next);
        } else {
            break;
        }
    }
    assert!(empty_with_progress >= 1);
    assert_eq!(visited, 507);
    assert_eq!(result.len(), 377);
    assert!(result.windows(2).all(|p| p[0].0 < p[1].0));
    assert_eq!(result.iter().find(|(n, _)| n == b"file").unwrap().1, 999);
    assert_eq!(result.last().unwrap().0, b"z0499");
    println!(
        "S3_MERGE visited={visited} visible={} empty_progress_pages={empty_with_progress}",
        result.len()
    );
    db.release_base_source(source).unwrap();
    db.close(ws.route()).unwrap();
    db.release_closed_capture(capture).unwrap();
    drop(db);
    std::fs::remove_file(path).unwrap();
}

struct Gate {
    store: Store,
    root: ObjectId,
    state: Mutex<(bool, bool)>,
    wake: std::sync::Condvar,
    entered: std::sync::mpsc::SyncSender<()>,
}
impl AuthenticatedObjects for Gate {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        let mut state = self.state.lock().unwrap();
        if ids.contains(&self.root) && state.0 {
            state.0 = false;
            self.entered.send(()).unwrap();
            while !state.1 {
                state = self.wake.wait(state).unwrap();
            }
        }
        drop(state);
        self.store.read_canonical_batch(ids)
    }
}
fn actor_submit(
    client: &layerfs_daemon::OwnerClient,
    route: Option<layerfs_overlay::Route>,
    command: layerfs_daemon::Command,
) -> layerfs_daemon::Pending {
    client
        .try_submit(route, command)
        .unwrap_or_else(|(error, _)| panic!("{error:?}"))
}
#[test]
fn actual_provider_wait_allows_shared_progress_and_actor_install_pairs_the_selected_base() {
    use layerfs_daemon::{Command, Owner, OwnerConfig, Response};
    use layerfs_overlay::{Cell, CELL_BYTES, MASK_BYTES};
    let f = fixture();
    let (entered, waiting) = std::sync::mpsc::sync_channel(1);
    let gate = Arc::new(Gate {
        store: f.store.clone(),
        root: f.root.0,
        state: Mutex::new((false, false)),
        wake: std::sync::Condvar::new(),
        entered,
    });
    let client = Arc::new(CanonicalClient::with_lengths(
        gate.clone(),
        Arc::new(f.store.clone()),
        0,
    ));
    let base = BaseView::open(client, f.root, f.scope).unwrap();
    let path =
        std::env::temp_dir().join(format!("layerfs-actor-view-{}.sqlite", std::process::id()));
    let owner = Owner::start(&path, ProfileConfig::default(), OwnerConfig::default()).unwrap();
    let service = owner.client();
    let opened = actor_submit(
        &service,
        None,
        Command::Open {
            incarnation: [102; 32],
            base_root: f.root.0.to_bytes(),
        },
    )
    .wait()
    .unwrap();
    let route = match opened.result() {
        Ok(Response::Opened(r)) => *r,
        x => panic!("{x:?}"),
    };
    drop(opened);
    let ws = Arc::new(Workspace::bind(route, base));
    let old = ws.base().unwrap().plan_read(2, 0, 64).unwrap();
    let bytes = b"changed";
    let mut data = Box::new([0; CELL_BYTES]);
    data[..bytes.len()].copy_from_slice(bytes);
    let mut mask = Box::new([0; MASK_BYTES]);
    mask[0] = 127;
    let mut inode = local(2, 7, 2);
    inode.mode = 0o644;
    inode.mtime_seconds = i64::MAX;
    inode.mtime_nanoseconds = 999999999;
    inode.inherited_cutoff = 0;
    let done = actor_submit(
        &service,
        Some(route),
        Command::Publish {
            inode,
            name: None,
            cell: Some(Cell {
                offset: 0,
                data,
                validity: mask,
            }),
        },
    )
    .wait()
    .unwrap();
    let p = match done.result() {
        Ok(Response::Published(p)) => *p,
        x => panic!("{x:?}"),
    };
    drop(done);
    assert!(
        actor_submit(&service, Some(route), Command::ReplyAttempted(p))
            .wait()
            .unwrap()
            .result()
            .is_ok()
    );
    let captured = actor_submit(&service, Some(route), Command::Capture)
        .wait()
        .unwrap();
    let capture = match captured.result() {
        Ok(Response::Captured(c)) => *c,
        x => panic!("{x:?}"),
    };
    drop(captured);
    let mut value = ws.base().unwrap().inode(2).unwrap().value;
    value.content_root = file(&f.store, bytes);
    let updates = [InodeUpdate { serial: 2, value }];
    let mut sink = f.store.clone();
    let mut objects = FilesystemObjects::new(&f.store, &mut sink);
    let root = update_filesystem(
        &mut objects,
        &FilesystemInput {
            base: Some(f.root),
            scope: f.scope,
            root_serial: 1,
            directories: &[],
            inodes: &updates,
            new_inodes: &[],
            resources: FilesystemResources::default(),
        },
        None,
    )
    .unwrap()
    .root;
    let prepared = ws.prepare_base_install(capture, root).unwrap();
    let source_done = actor_submit(
        &service,
        Some(route),
        Command::AcquireBaseSource { owner: 7 },
    )
    .wait()
    .unwrap();
    let source = match source_done.result() {
        Ok(Response::BaseSource(s)) => *s,
        x => panic!("{x:?}"),
    };
    drop(source_done);
    let view = ws.view_for_source(source).unwrap();
    gate.state.lock().unwrap().0 = true;
    let other_service = service.clone();
    let worker = std::thread::spawn(move || view.lookup(&other_service, 1, &name("alias")));
    waiting
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
    let install = actor_submit(
        &service,
        Some(route),
        Command::InstallPrepared {
            workspace: ws.clone(),
            input: prepared,
        },
    );
    let later_source = actor_submit(
        &service,
        Some(route),
        Command::AcquireBaseSource { owner: 8 },
    );
    assert!(install.try_complete().unwrap().is_none());
    assert!(later_source.try_complete().unwrap().is_none());
    let observed = actor_submit(&service, Some(route), Command::State)
        .wait()
        .unwrap();
    assert!(matches!(observed.result(),Ok(Response::State(s)) if s.base_root==f.root.0.to_bytes()));
    drop(observed);
    let done = actor_submit(
        &service,
        Some(route),
        Command::Publish {
            inode: local(2, 22, 2),
            name: None,
            cell: None,
        },
    )
    .wait()
    .unwrap();
    let later = match done.result() {
        Ok(Response::Published(p)) => *p,
        x => panic!("{x:?}"),
    };
    drop(done);
    assert!(
        actor_submit(&service, Some(route), Command::ReplyAttempted(later))
            .wait()
            .unwrap()
            .result()
            .is_ok()
    );
    let unrelated = actor_submit(
        &service,
        None,
        Command::Open {
            incarnation: [103; 32],
            base_root: f.root.0.to_bytes(),
        },
    )
    .wait()
    .unwrap();
    assert!(unrelated.result().is_ok());
    drop(unrelated);
    gate.state.lock().unwrap().1 = true;
    gate.wake.notify_all();
    let stat = worker.join().unwrap().unwrap();
    assert_eq!(
        (stat.serial, stat.logical_len, stat.metadata.mode),
        (2, 22, 0o600)
    );
    assert!(
        actor_submit(&service, Some(route), Command::ReleaseBaseSource(source))
            .wait()
            .unwrap()
            .result()
            .is_ok()
    );
    assert!(install.wait().unwrap().result().is_ok());
    let next = later_source.wait().unwrap();
    let next_source = match next.result() {
        Ok(Response::BaseSource(s)) => *s,
        x => panic!("{x:?}"),
    };
    drop(next);
    assert_eq!(ws.base().unwrap().identity(), root);
    assert_eq!(next_source.root(), root.0.to_bytes());
    assert!(actor_submit(
        &service,
        Some(route),
        Command::ReleaseBaseSource(next_source)
    )
    .wait()
    .unwrap()
    .result()
    .is_ok());
    let mut output = Vec::new();
    old.emit(&mut output).unwrap();
    assert_eq!(output, b"original\0\xff");
    output.clear();
    ws.base()
        .unwrap()
        .plan_read(2, 0, 64)
        .unwrap()
        .emit(&mut output)
        .unwrap();
    assert_eq!(output, bytes);
    owner.stop().unwrap();
    std::fs::remove_file(path).unwrap();
}

struct Refused;
impl AuthenticatedObjects for Refused {
    fn read_canonical_batch(&self, _: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        Err(ContentError::ProviderFailure {
            what: "owning source refused",
        })
    }
}
#[test]
fn source_errors_and_unattempted_actor_install_preserve_exact_custody() {
    use layerfs_daemon::{Command, Owner, OwnerConfig, OwnerError, Response};
    let f = fixture();
    let client = Arc::new(CanonicalClient::with_lengths(
        Arc::new(f.store.clone()),
        Arc::new(f.store.clone()),
        0,
    ));
    let base = BaseView::open(client, f.root, f.scope).unwrap();
    let path =
        std::env::temp_dir().join(format!("layerfs-view-cancel-{}.sqlite", std::process::id()));
    let owner = Owner::start(&path, ProfileConfig::default(), OwnerConfig::default()).unwrap();
    let service = owner.client();
    let done = actor_submit(
        &service,
        None,
        Command::Open {
            incarnation: [104; 32],
            base_root: f.root.0.to_bytes(),
        },
    )
    .wait()
    .unwrap();
    let route = match done.result() {
        Ok(Response::Opened(r)) => *r,
        x => panic!("{x:?}"),
    };
    drop(done);
    let ws = Arc::new(Workspace::bind(route, base));
    let done = actor_submit(&service, Some(route), Command::Capture)
        .wait()
        .unwrap();
    let capture = match done.result() {
        Ok(Response::Captured(c)) => *c,
        x => panic!("{x:?}"),
    };
    drop(done);
    let input = ws.prepare_base_install(capture, f.root).unwrap();
    let done = actor_submit(
        &service,
        Some(route),
        Command::AcquireBaseSource { owner: 8 },
    )
    .wait()
    .unwrap();
    let source = match done.result() {
        Ok(Response::BaseSource(s)) => *s,
        x => panic!("{x:?}"),
    };
    drop(done);
    let pending = actor_submit(
        &service,
        Some(route),
        Command::InstallPrepared {
            workspace: ws.clone(),
            input,
        },
    );
    assert!(pending.try_complete().unwrap().is_none());
    owner.stop().unwrap();
    let done = pending.wait().unwrap();
    assert!(
        matches!(done.result(),Err(OwnerError::Unattempted{cause,command}) if matches!(cause.as_ref(),OwnerError::Stopped)&&matches!(command.as_ref(),Command::InstallPrepared{input,..} if input.root()==f.root&&input.capture()==capture))
    );
    assert_eq!(ws.base().unwrap().identity(), f.root);
    drop(done);
    std::fs::remove_file(path).unwrap();
    // The source error remains a source failure, never logical absence or fallback.
    let client = Arc::new(CanonicalClient::new(Arc::new(Refused), 0));
    assert!(matches!(
        BaseView::open(client, f.root, f.scope),
        Err(ContentError::ProviderFailure {
            what: "owning source refused"
        })
    ));
    assert_eq!(source.root(), f.root.0.to_bytes());
}
