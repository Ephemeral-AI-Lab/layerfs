//! New page owners finish with their first ledger write on real Linux backing.
#![cfg(target_os = "linux")]

use layerfs_workspace::{
    sequence::{
        metadata_pages::{self, Cell, PageRef},
        Budget, MetadataHost, PayloadHost,
    },
    Piece, PieceKind,
};
use std::{
    fs,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[test]
fn first_owner_write_covers_keyed_extent_and_shared_custody() {
    let parent = std::env::var("LAYERFS_OWNER_TEST_ROOT").expect("ext4 test root");
    let path =
        std::path::Path::new(&parent).join(format!("owner-finalization-{}", std::process::id()));
    use std::os::unix::fs::DirBuilderExt;
    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700).create(&path).unwrap();
    let incarnation = [41; 32];
    let budget = Budget::new(256 << 20);
    let payloads = PayloadHost::new(path.join("payloads"), 256 << 20, &budget).unwrap();
    let directory = payloads.directory(path.join("pages"), incarnation).unwrap();
    payloads.initialize(&directory).unwrap();
    let host = MetadataHost::new(payloads.clone()).unwrap();
    let arena = host.arena(directory.clone()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    let payload = payloads
        .acquire(
            directory,
            1,
            &mut &b"x"[..],
            deadline,
            &AtomicBool::new(false),
        )
        .unwrap();
    let payload_id = *payloads
        .state
        .lock()
        .unwrap()
        .records
        .keys()
        .next()
        .unwrap();
    let mut lease = payloads.window(1, 3).unwrap();
    let window = lease.window.as_mut().unwrap();

    let keyed = host.candidate(&arena, 1, false, None).unwrap();
    let key = metadata_pages::dirty_key(1, 1);
    let keyed_root = keyed
        .update(
            PageRef::NULL,
            vec![Cell::new(&key, &[1]).unwrap()],
            window,
            deadline,
        )
        .unwrap();
    assert!(
        arena
            .read_owner(keyed_root, window, deadline)
            .unwrap()
            .edges
    );
    keyed.seal(keyed_root, window, deadline).unwrap();

    let old = host.candidate(&arena, 1, false, None).unwrap();
    let custody = arena.custody(&old, &payload, window, deadline).unwrap();
    let piece = Piece {
        kind: PieceKind::Local,
        length: 1,
        offset: 0,
        payload: payload_id,
        custody,
    };
    let old_root = old.build_pieces(&[piece], 1, window).unwrap();
    assert!(arena.read_owner(old_root, window, deadline).unwrap().edges);
    old.seal(old_root, window, deadline).unwrap();
    let old_bytes = arena.load_raw(old_root, window, deadline).unwrap();

    let new = host.candidate(&arena, 2, false, Some(old.clone())).unwrap();
    assert_eq!(
        arena.custody(&new, &payload, window, deadline).unwrap(),
        custody
    );
    let new_root = new.build_pieces(&[piece], 1, window).unwrap();
    new.seal(new_root, window, deadline).unwrap();
    assert_ne!(old_root, new_root);
    assert_eq!(
        arena.load_raw(old_root, window, deadline).unwrap(),
        old_bytes
    );
    assert_eq!(arena.owner_finalizations.load(Ordering::Relaxed), 3);

    drop(lease);
    drop(new);
    drop(old);
    drop(keyed);
    drop(payload);
    let cleaned = host.reclaim(incarnation, deadline).unwrap();
    assert_eq!(cleaned.remaining_roots, 0);
    assert_eq!(host.status().unwrap().allocated_pages, 0);
    payloads.reclaim(incarnation, deadline).unwrap();
    assert_eq!(payloads.status().unwrap().payloads, 0);
    drop(host);
    drop(arena);
    drop(payloads);
    fs::remove_dir_all(path).unwrap();
}
